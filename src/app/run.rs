//! Replays every configured feed through its own trading session.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::error::AppError;
use crate::app::session::TradingSession;
use crate::config::engine_config::EngineConfig;
use crate::market::feed::stream::{MarketFeed, open_feeds};
use crate::market::snapshot::{MarketSnapshot, SnapshotDefect};
use crate::report::ledger_journal::LedgerJournal;
use crate::report::run_summary::RunSummary;
use crate::report::{event_log, text_report};

pub fn run(config_path: &Path) -> Result<(), AppError> {
    let config = EngineConfig::load(config_path)?;
    for feed in open_feeds(&config.feed)? {
        let (summary, journal) = replay(&config, feed)?;
        println!("{}", text_report::render(&summary));
        if let Some(path) = journal {
            println!("  ledger       {}", path.display());
        }
        println!();
    }
    Ok(())
}

/// Validates each snapshot before the session sees it. Refused input is
/// logged, counted and skipped: one bad line from a connector must not stop
/// a live engine, and nothing is ever decided on it.
fn replay(
    config: &EngineConfig,
    feed: MarketFeed,
) -> Result<(RunSummary, Option<PathBuf>), AppError> {
    let MarketFeed {
        name,
        snapshots,
        clock_window_secs,
    } = feed;
    let mut session = TradingSession::open(config, &name)?;
    let mut journal = match &config.report.ledger_dir {
        Some(directory) => Some(LedgerJournal::open(
            directory,
            &name,
            session.resumed_at().is_some(),
        )?),
        None => None,
    };
    let mut summary = RunSummary::new(name.clone());
    let mut previous_timestamp = session.resumed_at();
    for item in snapshots {
        let snapshot = match item {
            Ok(snapshot) => snapshot,
            Err(AppError::FeedParse { line, source, .. }) => {
                eprintln!("[{name}] skipped line {line}: {source}");
                summary.skip("invalid_json");
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Err(defect) = accept(&snapshot, previous_timestamp, clock_window_secs) {
            eprintln!(
                "[{name} @ {}] skipped snapshot: {defect}",
                snapshot.timestamp
            );
            summary.skip(defect.label());
            continue;
        }
        previous_timestamp = Some(snapshot.timestamp);
        let report = session.step(&snapshot)?;
        if let Some(journal) = journal.as_mut() {
            journal.record(session.ledger())?;
        }
        for event in &report.events {
            eprintln!("{}", event_log::line(&name, snapshot.timestamp, *event));
        }
        summary.record(&snapshot, &report);
    }
    Ok((summary, journal.map(|journal| journal.path().to_path_buf())))
}

fn accept(
    snapshot: &MarketSnapshot,
    previous_timestamp: Option<u64>,
    clock_window_secs: Option<u64>,
) -> Result<(), SnapshotDefect> {
    snapshot.validate(previous_timestamp)?;
    match clock_window_secs {
        Some(max_age_secs) => snapshot.check_clock(unix_now(), max_age_secs),
        None => Ok(()),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

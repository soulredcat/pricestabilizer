//! Replays every configured feed through its own trading session.

use std::path::Path;

use crate::app::error::AppError;
use crate::app::session::TradingSession;
use crate::config::engine_config::EngineConfig;
use crate::market::feed::stream::{MarketFeed, open_feeds};
use crate::report::run_summary::RunSummary;
use crate::report::{event_log, ledger_export, text_report};

pub fn run(config_path: &Path) -> Result<(), AppError> {
    let config = EngineConfig::load(config_path)?;
    for feed in open_feeds(&config.feed)? {
        let (summary, session) = replay(&config, feed)?;
        println!("{}", text_report::render(&summary));
        if let Some(directory) = &config.report.ledger_dir {
            let path = ledger_export::write(directory, &summary.feed_name, session.ledger())?;
            println!("  ledger       {}", path.display());
        }
        println!();
    }
    Ok(())
}

/// Validates each snapshot before the session sees it.
fn replay(
    config: &EngineConfig,
    feed: MarketFeed,
) -> Result<(RunSummary, TradingSession), AppError> {
    let MarketFeed { name, snapshots } = feed;
    let mut session = TradingSession::open(config, &name)?;
    let mut summary = RunSummary::new(name.clone());
    let mut previous_timestamp = None;
    for (index, snapshot) in snapshots.enumerate() {
        let snapshot = snapshot?;
        snapshot
            .validate(previous_timestamp)
            .map_err(|reason| AppError::SnapshotInvalid {
                feed: name.clone(),
                index,
                reason,
            })?;
        previous_timestamp = Some(snapshot.timestamp);
        let report = session.step(&snapshot)?;
        for event in &report.events {
            eprintln!("{}", event_log::line(&name, snapshot.timestamp, *event));
        }
        summary.record(&snapshot, &report);
    }
    Ok((summary, session))
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;

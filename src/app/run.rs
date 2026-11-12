//! Replays every configured feed through a fresh pipeline and executor.

use std::path::Path;

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::engine::strategy::pipeline::StrategyPipeline;
use crate::execution::executor::Executor;
use crate::market::feed::stream::{MarketFeed, open_feeds};
use crate::report::run_summary::RunSummary;
use crate::report::{ledger_export, text_report};
use crate::venue::venue_factory::open_venue;

pub fn run(config_path: &Path) -> Result<(), AppError> {
    let config = EngineConfig::load(config_path)?;
    for feed in open_feeds(&config.feed)? {
        let (summary, executor) = replay(&config, feed)?;
        println!("{}", text_report::render(&summary));
        if let Some(directory) = &config.report.ledger_dir {
            let path = ledger_export::write(directory, &summary.feed_name, executor.ledger())?;
            println!("  ledger       {}", path.display());
        }
        println!();
    }
    Ok(())
}

/// Validates each snapshot, decides, executes, and feeds the execution
/// result back into the next decision.
fn replay(config: &EngineConfig, feed: MarketFeed) -> Result<(RunSummary, Executor), AppError> {
    let MarketFeed { name, snapshots } = feed;
    let mut pipeline = StrategyPipeline::new(config);
    let mut executor =
        Executor::open(config, open_venue(config)).map_err(|error| AppError::VenueUnavailable {
            feed: name.clone(),
            reason: error.label(),
        })?;
    let mut summary = RunSummary::new(name.clone());
    let mut feedback = ExecutionFeedback::default();
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
        let decision = pipeline.on_snapshot(&snapshot, feedback);
        let outcome = executor.on_decision(&snapshot, decision.liquidity);
        feedback = outcome.feedback;
        summary.record(&snapshot, &decision, &outcome);
    }
    Ok((summary, executor))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::replay;
    use crate::config::engine_config::EngineConfig;
    use crate::market::feed::stream::open_feeds;
    use crate::report::run_summary::RunSummary;
    use crate::report::text_report;

    const EXAMPLE: &str = include_str!("../../config/simulation.example.toml");

    fn run_scenario(name: &str) -> RunSummary {
        let config = EngineConfig::parse(EXAMPLE, Path::new("example")).expect("valid example");
        let feed = open_feeds(&config.feed)
            .expect("scenario feeds open")
            .into_iter()
            .find(|feed| feed.name == name)
            .expect("scenario is listed in the example config");
        replay(&config, feed).expect("scenario replays").0
    }

    fn protection_cause(name: &str) -> Option<&'static str> {
        run_scenario(name).protection.map(|trip| trip.cause)
    }

    #[test]
    fn emergencies_trip_protection_with_the_right_cause() {
        assert_eq!(protection_cause("flash_crash"), Some("extreme_move"));
        assert_eq!(protection_cause("stable_depeg"), Some("stable_depeg"));
        assert_eq!(protection_cause("liquidity_exit"), Some("depth_collapse"));
    }

    #[test]
    fn calm_markets_never_trip_protection() {
        for name in ["steady_bull", "sideways", "liquidity_inflow"] {
            assert_eq!(protection_cause(name), None, "{name}");
        }
    }

    #[test]
    fn books_always_reconcile_with_the_venue() {
        for name in ["steady_bull", "steady_bear", "pump_retrace", "data_gap"] {
            let cause = protection_cause(name);
            assert_ne!(cause, Some("ledger_mismatch"), "{name}");
            assert_ne!(cause, Some("venue_failures"), "{name}");
        }
    }

    #[test]
    fn thin_market_never_gets_a_band() {
        let summary = run_scenario("low_liquidity");
        assert!(summary.latest_band.is_none());
        assert_eq!(summary.price_issues.get("thin_depth"), Some(&summary.ticks));
        assert!(summary.orders.is_empty());
    }

    #[test]
    fn trending_market_moves_its_band_and_earns_fees() {
        let summary = run_scenario("steady_bull");
        assert!(
            summary
                .liquidity
                .get("reposition")
                .is_some_and(|count| *count > 0)
        );
        let books = summary.books.expect("books were marked");
        assert!(books.fees_claimed + books.unclaimed_fees > 0.0);
        assert!(books.action_costs > 0.0);
    }

    #[test]
    fn feed_outage_is_flagged() {
        let summary = run_scenario("data_gap");
        assert_eq!(summary.price_issues.get("sample_gap"), Some(&1));
    }

    #[test]
    fn report_names_the_feed_protection_and_books() {
        let rendered = text_report::render(&run_scenario("flash_crash"));
        assert!(rendered.starts_with("== flash_crash =="));
        assert!(rendered.contains("latched after"));
        assert!(rendered.contains("equity"));
    }
}

//! Replays every configured feed through its own fresh strategy pipeline.

use std::path::Path;

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::engine::strategy::pipeline::StrategyPipeline;
use crate::market::feed::stream::{MarketFeed, open_feeds};
use crate::report::run_summary::RunSummary;
use crate::report::text_report;

pub fn run(config_path: &Path) -> Result<(), AppError> {
    let config = EngineConfig::load(config_path)?;
    for feed in open_feeds(&config.feed)? {
        let summary = replay(&config, feed)?;
        println!("{}\n", text_report::render(&summary));
    }
    Ok(())
}

/// Validates each snapshot before the engine sees it.
fn replay(config: &EngineConfig, feed: MarketFeed) -> Result<RunSummary, AppError> {
    let MarketFeed { name, snapshots } = feed;
    let mut pipeline = StrategyPipeline::new(config);
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
        summary.record(&snapshot, &pipeline.on_snapshot(&snapshot));
    }
    Ok(summary)
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
        replay(&config, feed).expect("scenario replays")
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
        for name in ["steady_bull", "steady_bear", "sideways", "liquidity_inflow"] {
            assert_eq!(protection_cause(name), None, "{name}");
        }
    }

    #[test]
    fn thin_market_never_gets_a_band() {
        let summary = run_scenario("low_liquidity");
        assert!(summary.latest_band.is_none());
        assert_eq!(summary.price_issues.get("thin_depth"), Some(&summary.ticks));
    }

    #[test]
    fn trending_market_moves_its_band() {
        let summary = run_scenario("steady_bull");
        assert!(
            summary
                .liquidity
                .get("reposition")
                .is_some_and(|count| *count > 0)
        );
    }

    #[test]
    fn feed_outage_is_flagged() {
        let summary = run_scenario("data_gap");
        assert_eq!(summary.price_issues.get("sample_gap"), Some(&1));
    }

    #[test]
    fn report_names_the_feed_and_protection() {
        let rendered = text_report::render(&run_scenario("flash_crash"));
        assert!(rendered.starts_with("== flash_crash =="));
        assert!(rendered.contains("latched after"));
    }
}

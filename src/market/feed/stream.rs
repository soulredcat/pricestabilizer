//! Opens the configured feeds as boxed snapshot streams.

use std::time::Duration;

use crate::app::error::AppError;
use crate::config::feed_section::FeedConfig;
use crate::market::feed::scenario::generator;
use crate::market::feed::{follow, jsonl};
use crate::market::snapshot::MarketSnapshot;

pub type SnapshotStream = Box<dyn Iterator<Item = Result<MarketSnapshot, AppError>>>;

/// A named stream. Each feed is replayed through its own fresh engine.
pub struct MarketFeed {
    pub name: String,
    pub snapshots: SnapshotStream,
    /// Live feeds only: how far from the wall clock a snapshot may be.
    pub clock_window_secs: Option<u64>,
}

pub fn open_feeds(config: &FeedConfig) -> Result<Vec<MarketFeed>, AppError> {
    match config {
        FeedConfig::Scenario(settings) => Ok(settings
            .scenarios
            .iter()
            .map(|kind| generator::feed(*kind, settings))
            .collect()),
        FeedConfig::Jsonl(settings) => Ok(vec![jsonl::open(&settings.path)?]),
        FeedConfig::Stdin(settings) => Ok(vec![jsonl::stdin(settings.max_snapshot_age_secs)]),
        FeedConfig::Follow(settings) => Ok(vec![follow::follow(
            &settings.path,
            Duration::from_millis(settings.poll_millis),
            settings.max_snapshot_age_secs,
        )?]),
    }
}

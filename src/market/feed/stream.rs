//! Opens the configured feeds as boxed snapshot streams.

use crate::app::error::AppError;
use crate::config::feed_section::FeedConfig;
use crate::market::feed::jsonl;
use crate::market::feed::scenario::generator;
use crate::market::snapshot::MarketSnapshot;

pub type SnapshotStream = Box<dyn Iterator<Item = Result<MarketSnapshot, AppError>>>;

/// A named stream. Each feed is replayed through its own fresh engine.
pub struct MarketFeed {
    pub name: String,
    pub snapshots: SnapshotStream,
}

pub fn open_feeds(config: &FeedConfig) -> Result<Vec<MarketFeed>, AppError> {
    match config {
        FeedConfig::Scenario(settings) => Ok(settings
            .scenarios
            .iter()
            .map(|kind| generator::feed(*kind, settings))
            .collect()),
        FeedConfig::Jsonl(settings) => Ok(vec![jsonl::open(&settings.path)?]),
    }
}

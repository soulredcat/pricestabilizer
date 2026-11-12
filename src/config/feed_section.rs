//! Where snapshots come from. The engine cannot tell the sources apart.

use std::path::PathBuf;

use serde::Deserialize;

use crate::market::feed::scenario::kind::ScenarioKind;

#[derive(Debug, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum FeedConfig {
    /// Synthetic stress scenarios, for backtesting without any chain.
    Scenario(ScenarioFeedConfig),
    /// Snapshots written by an external connector, one JSON object per line.
    Jsonl(JsonlFeedConfig),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFeedConfig {
    /// Every listed scenario runs on its own fresh engine.
    pub scenarios: Vec<ScenarioKind>,
    pub seed: u64,
    pub steps: u64,
    pub interval_secs: u64,
    pub start_timestamp: u64,
    pub start_price: f64,
    pub start_quote_depth: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonlFeedConfig {
    pub path: PathBuf,
}

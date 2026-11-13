//! Treasury accumulation policy and the operator command channel.

use std::path::PathBuf;

use serde::Deserialize;

/// Accumulation is only ever *proposed*; an operator must approve each one.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreasuryConfig {
    /// Total quote the treasury may spend on accumulation.
    pub budget_quote: f64,
    /// Quote size of one proposal.
    pub proposal_quote: f64,
    /// Minimum time between two proposals.
    pub min_proposal_interval_secs: u64,
    /// A proposal not approved within this time expires; so does an approved
    /// one that buys nothing for this long.
    pub proposal_ttl_secs: u64,
    /// Largest single purchase; an approved proposal is bought in slices.
    pub slice_quote: f64,
    /// Minimum time between two slices.
    pub min_slice_interval_secs: u64,
    /// Treasury purchases per UTC day stay below this share of the market's
    /// estimated daily volume.
    pub max_daily_volume_share: f64,
    /// Buy only while the price is outside the project's own band, so the
    /// treasury never trades against the project's own liquidity.
    pub only_outside_own_band: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OperatorConfig {
    /// JSON-lines file of operator commands. Lines appended while the engine
    /// runs are picked up on the next snapshot.
    pub commands_path: Option<PathBuf>,
}

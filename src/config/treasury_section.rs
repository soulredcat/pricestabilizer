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
    /// A proposal not approved within this time expires.
    pub proposal_ttl_secs: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OperatorConfig {
    /// JSON-lines file of operator commands. Lines appended while the engine
    /// runs are picked up on the next snapshot.
    pub commands_path: Option<PathBuf>,
}

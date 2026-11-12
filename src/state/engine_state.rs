//! Everything a restart must not reset: the protection latch, the band,
//! loss baselines, ledger balances, order numbering, the treasury budget and
//! how far the operator's command file was read. Market windows are left
//! out on purpose: after a restart they warm up again.

use serde::{Deserialize, Serialize};

use crate::engine::strategy::pipeline::PipelineState;
use crate::execution::executor::ExecutionState;
use crate::treasury::desk::TreasuryState;

/// Bumped whenever the layout changes; an unknown version refuses to load.
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineState {
    pub version: u32,
    /// Timestamp of the last snapshot handled; older ones are refused.
    pub saved_at: u64,
    pub pipeline: PipelineState,
    pub execution: ExecutionState,
    pub treasury: TreasuryState,
    pub commands_consumed: usize,
}

//! What of the order runner survives a restart, and the operator's
//! reconciliation of the ledger with the venue.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::engine_config::EngineConfig;
use crate::execution::order_runner::OrderRunner;
use crate::portfolio::ledger::{Account, BalanceRow, Ledger};
use crate::portfolio::pnl_tracker::FillTotals;
use crate::venue::adapter::Venue;
use crate::venue::order::{VenueError, Wallet};

/// Order numbering and today's order count, alerts not yet acknowledged,
/// fee and cost totals, the ledger balances and the venue's own state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerState {
    pub next_order_id: u64,
    pub orders_today: Option<(u64, u32)>,
    pub consecutive_failures: u32,
    pub ledger_broken: bool,
    pub anomaly: bool,
    pub order_burst: bool,
    pub totals: FillTotals,
    pub balances: Vec<BalanceRow>,
    /// The venue's own state, for venues that keep any (the paper venue).
    pub venue: Option<Value>,
}

impl OrderRunner {
    /// Picks up a saved runner. No deposits are booked: the next
    /// reconciliation compares the saved ledger with what the venue holds.
    pub fn restore(
        config: &EngineConfig,
        mut venue: Box<dyn Venue>,
        state: RunnerState,
    ) -> Result<Self, VenueError> {
        if let Some(saved) = state.venue {
            venue.restore_state(saved)?;
        }
        let mut runner = Self::assemble(config, venue, Ledger::from_balances(&state.balances));
        runner.guards.resume_from(state.next_order_id);
        runner.orders_today = state.orders_today;
        runner.consecutive_failures = state.consecutive_failures;
        runner.ledger_broken = state.ledger_broken;
        runner.anomaly = state.anomaly;
        runner.order_burst = state.order_burst;
        runner.totals = state.totals;
        Ok(runner)
    }

    pub fn state(&self) -> RunnerState {
        RunnerState {
            next_order_id: self.guards.next_order_id(),
            orders_today: self.orders_today,
            consecutive_failures: self.consecutive_failures,
            ledger_broken: self.ledger_broken,
            anomaly: self.anomaly,
            order_burst: self.order_burst,
            totals: self.totals,
            balances: self.ledger.balances(),
            venue: self.venue.saved_state(),
        }
    }

    /// Adopts the venue's wallets in the ledger: the operator's answer to a
    /// mismatch caused by transfers made outside the engine.
    pub fn reconcile(&mut self, timestamp: u64) -> Result<(), VenueError> {
        let operational = self.venue.balances(Wallet::Operational)?;
        let treasury = self.venue.balances(Wallet::Treasury)?;
        self.ledger
            .adjust_to(timestamp, Account::Operational, operational);
        self.ledger
            .adjust_to(timestamp, Account::Treasury, treasury);
        self.ledger_broken = false;
        Ok(())
    }
}

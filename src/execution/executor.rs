//! Turns liquidity intents into venue orders. Every order passes the risk
//! gate, every fill is booked, and after each snapshot the books are marked
//! and reconciled with the venue. Problems become alerts that trip
//! protection on the next snapshot.

use serde::{Deserialize, Serialize};

use crate::config::engine_config::EngineConfig;
use crate::config::engine_sections::LiquidityConfig;
use crate::config::execution_sections::RiskConfig;
use crate::engine::liquidity::action::LiquidityAction;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::execution::band_funding::Amounts;
use crate::execution::band_mover::move_band;
use crate::execution::inspection::inspect;
use crate::execution::order_runner::OrderRunner;
use crate::execution::order_runner::persistence::RunnerState;
use crate::execution::outcome::ExecutionOutcome;
use crate::execution::request::ExecutionRequest;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::Asset;
use crate::portfolio::ledger::Ledger;
use crate::portfolio::pnl_tracker::{PnlState, PnlTracker};
use crate::venue::adapter::Venue;
use crate::venue::order::{VenueError, Wallet};

pub struct Executor {
    runner: OrderRunner,
    pnl: PnlTracker,
    liquidity: LiquidityConfig,
    risk: RiskConfig,
    initial: Amounts,
}

/// What of the executor survives a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    /// The starting holdings: the hold benchmark.
    pub initial: Amounts,
    pub pnl: PnlState,
    pub runner: RunnerState,
}

impl Executor {
    /// Opens fresh books from the venue's balances, or picks up `saved` ones.
    pub fn open(
        config: &EngineConfig,
        venue: Box<dyn Venue>,
        saved: Option<ExecutionState>,
    ) -> Result<Self, VenueError> {
        let mut pnl = PnlTracker::new(&config.risk);
        let (runner, initial) = match saved {
            Some(state) => {
                pnl.restore(state.pnl);
                (
                    OrderRunner::restore(config, venue, state.runner)?,
                    state.initial,
                )
            }
            None => {
                let (runner, holding) = OrderRunner::open(config, venue)?;
                let initial = Amounts::from_holding(holding, runner.scale());
                (runner, initial)
            }
        };
        Ok(Self {
            runner,
            pnl,
            liquidity: config.liquidity.clone(),
            risk: config.risk.clone(),
            initial,
        })
    }

    pub fn state(&self) -> ExecutionState {
        ExecutionState {
            initial: self.initial,
            pnl: self.pnl.state(),
            runner: self.runner.state(),
        }
    }

    /// Operator reconciliation; `false` when the venue could not be read.
    pub fn reconcile(&mut self, timestamp: u64) -> bool {
        self.runner.reconcile(timestamp).is_ok()
    }

    pub fn ledger(&self) -> &Ledger {
        self.runner.ledger()
    }

    pub fn on_decision(
        &mut self,
        market: &MarketSnapshot,
        request: &ExecutionRequest,
    ) -> ExecutionOutcome {
        let mut outcome = ExecutionOutcome::default();
        self.runner.observe(market, &mut outcome);
        if let LiquidityAction::Open(band) | LiquidityAction::Reposition(band) = request.action {
            move_band(
                &mut self.runner,
                band,
                market,
                &self.liquidity,
                &self.risk,
                &mut outcome,
            );
        }
        if let Some(quote) = request.treasury_buy_quote {
            outcome.treasury_spent =
                self.treasury_buy(quote, market, request.valuation_price, &mut outcome);
        }
        let inspection = inspect(
            &mut self.runner,
            &mut self.pnl,
            market,
            request.valuation_price,
            self.initial,
            &mut outcome,
        );
        let (reconciled, loss_breached) = match &inspection {
            Some(inspection) => (inspection.reconciled, inspection.loss_breached),
            None => (true, false),
        };
        if let Some(inspection) = inspection {
            outcome.books = Some(inspection.books);
            outcome.feedback.band_held = inspection.band_held;
        }
        outcome.feedback.alert = self.alert(reconciled, loss_breached);
        outcome
    }

    /// An operator resumed: losses so far, past failures and anomalies are
    /// acknowledged. A broken ledger is not; it needs a reconciliation.
    pub fn acknowledge_losses(&mut self) {
        self.pnl.reset_baseline();
        self.runner.acknowledge();
    }

    /// Buys with treasury quote, never more than the treasury wallet holds,
    /// anchored to the TWAP; returns the quote actually spent.
    fn treasury_buy(
        &mut self,
        quote: f64,
        market: &MarketSnapshot,
        twap: f64,
        outcome: &mut ExecutionOutcome,
    ) -> Option<f64> {
        let scale = self.runner.scale();
        let held = self.runner.balances(Wallet::Treasury, outcome)?;
        let units = scale.to_units(Asset::Quote, quote).min(held.quote);
        let fill =
            self.runner
                .swap(Wallet::Treasury, Asset::Quote, units, market, twap, outcome)?;
        let spent = fill.spent.quote.saturating_sub(fill.action_cost);
        Some(scale.to_amount(Asset::Quote, spent))
    }

    fn alert(&self, reconciled: bool, loss_breached: bool) -> Option<EmergencyCause> {
        if self.runner.ledger_broken() || !reconciled {
            Some(EmergencyCause::LedgerMismatch)
        } else if self.runner.anomaly() {
            Some(EmergencyCause::ExecutionAnomaly)
        } else if self.runner.order_burst() {
            Some(EmergencyCause::OrderBurst)
        } else if self.runner.consecutive_failures() >= self.risk.max_consecutive_failures {
            Some(EmergencyCause::VenueFailures)
        } else if loss_breached {
            Some(EmergencyCause::LossLimit)
        } else {
            None
        }
    }
}

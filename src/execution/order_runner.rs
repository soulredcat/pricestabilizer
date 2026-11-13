//! Sends single orders: risk gate first, then the venue, then the ledger.
//! It owns the venue and the books, so every fill is booked exactly once,
//! checked against its order, and every venue failure is counted. What of
//! it survives a restart lives in `persistence`.

mod dispatch;
pub mod persistence;

use crate::config::engine_config::EngineConfig;
use crate::config::venue_section::VenueConfig;
use crate::execution::guard_rules::GuardRules;
use crate::execution::outcome::ExecutionOutcome;
use crate::execution::risk_gate::RiskGate;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::UnitScale;
use crate::portfolio::holding::Holding;
use crate::portfolio::ledger::{Account, Ledger};
use crate::portfolio::pnl_tracker::FillTotals;
use crate::venue::adapter::Venue;
use crate::venue::order::{PositionView, VenueError, Wallet};

pub struct OrderRunner {
    venue: Box<dyn Venue>,
    ledger: Ledger,
    gate: RiskGate,
    guards: GuardRules,
    scale: UnitScale,
    /// `false`: orders are planned and gated but withheld (dry run).
    send_orders: bool,
    consecutive_failures: u32,
    ledger_broken: bool,
    /// A venue answered with a fill that does not match its order.
    anomaly: bool,
    max_orders_per_day: u32,
    /// UTC day and orders sent on it.
    orders_today: Option<(u64, u32)>,
    /// The daily order limit was hit: a runaway loop until proven otherwise.
    order_burst: bool,
    totals: FillTotals,
}

impl OrderRunner {
    /// Opens the books with whatever the venue holds right now; also returns
    /// that starting holding across both wallets.
    pub fn open(
        config: &EngineConfig,
        mut venue: Box<dyn Venue>,
    ) -> Result<(Self, Holding), VenueError> {
        let operational = venue.balances(Wallet::Operational)?;
        let treasury = venue.balances(Wallet::Treasury)?;
        let mut ledger = Ledger::default();
        let opened_at = config.growth.start_timestamp;
        ledger.deposit(opened_at, Account::Operational, operational);
        ledger.deposit(opened_at, Account::Treasury, treasury);
        Ok((
            Self::assemble(config, venue, ledger),
            operational.plus(treasury),
        ))
    }

    fn assemble(config: &EngineConfig, venue: Box<dyn Venue>, ledger: Ledger) -> Self {
        let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
        let send_orders = match &config.venue {
            VenueConfig::Simulated(_) => true,
            VenueConfig::External(settings) => settings.send_orders,
        };
        Self {
            venue,
            ledger,
            gate: RiskGate::new(&config.risk, scale, config.price.depth_reference_move),
            guards: GuardRules::new(&config.risk, scale),
            scale,
            send_orders,
            consecutive_failures: 0,
            ledger_broken: false,
            anomaly: false,
            max_orders_per_day: config.risk.max_orders_per_day,
            orders_today: None,
            order_burst: false,
            totals: FillTotals::default(),
        }
    }

    pub fn scale(&self) -> UnitScale {
        self.scale
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn totals(&self) -> FillTotals {
        self.totals
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn ledger_broken(&self) -> bool {
        self.ledger_broken
    }

    pub fn anomaly(&self) -> bool {
        self.anomaly
    }

    pub fn order_burst(&self) -> bool {
        self.order_burst
    }

    /// An operator looked: past failures, anomalies and an order burst are
    /// acknowledged. Today's order count stays: the limit still applies.
    pub fn acknowledge(&mut self) {
        self.consecutive_failures = 0;
        self.anomaly = false;
        self.order_burst = false;
    }

    pub fn observe(&mut self, market: &MarketSnapshot, outcome: &mut ExecutionOutcome) {
        if let Err(error) = self.venue.observe(market) {
            self.fail(error, outcome);
        }
    }

    /// A wallet as the venue reports it; a failure is counted and recorded.
    pub fn balances(&mut self, wallet: Wallet, outcome: &mut ExecutionOutcome) -> Option<Holding> {
        self.venue
            .balances(wallet)
            .map_err(|error| self.fail(error, outcome))
            .ok()
    }

    /// The band position, `Err` when the venue could not be read.
    pub fn position(
        &mut self,
        market: &MarketSnapshot,
        outcome: &mut ExecutionOutcome,
    ) -> Result<Option<PositionView>, ()> {
        self.venue
            .position(market)
            .map_err(|error| self.fail(error, outcome))
    }

    /// Both wallets agree with the ledger, unit for unit.
    pub fn reconciled(&self, operational: Holding, treasury: Holding) -> bool {
        operational == self.ledger.holding(Account::Operational)
            && treasury == self.ledger.holding(Account::Treasury)
    }
}

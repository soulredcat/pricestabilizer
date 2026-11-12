//! Sends single orders: risk gate first, then the venue, then the ledger.
//! It owns the venue and the books, so every fill is booked exactly once
//! and every venue failure is counted.

use crate::config::engine_config::EngineConfig;
use crate::execution::booking;
use crate::execution::outcome::{ExecutionOutcome, OrderResult};
use crate::execution::risk_gate::{OrderContext, RiskGate};
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::holding::Holding;
use crate::portfolio::ledger::{Account, Ledger};
use crate::portfolio::pnl_tracker::FillTotals;
use crate::venue::adapter::Venue;
use crate::venue::order::{Fill, PositionView, VenueError, VenueOrder, Wallet};

pub struct OrderRunner {
    venue: Box<dyn Venue>,
    ledger: Ledger,
    gate: RiskGate,
    scale: UnitScale,
    consecutive_failures: u32,
    ledger_broken: bool,
    totals: FillTotals,
}

impl OrderRunner {
    /// Opens the books with whatever the venue holds right now; also returns
    /// that starting holding across both wallets.
    pub fn open(
        config: &EngineConfig,
        mut venue: Box<dyn Venue>,
    ) -> Result<(Self, Holding), VenueError> {
        let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
        let operational = venue.balances(Wallet::Operational)?;
        let treasury = venue.balances(Wallet::Treasury)?;
        let mut ledger = Ledger::default();
        let opened_at = config.growth.start_timestamp;
        ledger.deposit(opened_at, Account::Operational, operational);
        ledger.deposit(opened_at, Account::Treasury, treasury);
        let runner = Self {
            venue,
            ledger,
            gate: RiskGate::new(&config.risk, scale, config.price.depth_reference_move),
            scale,
            consecutive_failures: 0,
            ledger_broken: false,
            totals: FillTotals::default(),
        };
        Ok((runner, operational.plus(treasury)))
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

    pub fn reset_failures(&mut self) {
        self.consecutive_failures = 0;
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

    /// Gate, execute and book one order; the fill when it went through.
    pub fn run(
        &mut self,
        order: VenueOrder,
        market: &MarketSnapshot,
        outcome: &mut ExecutionOutcome,
    ) -> Option<Fill> {
        let context = match self.order_context(market) {
            Ok(context) => context,
            Err(error) => {
                self.fail(error, outcome);
                return None;
            }
        };
        if let Err(breach) = self.gate.check(&order, &context) {
            outcome.orders.push(OrderResult::Blocked(breach));
            return None;
        }
        match self.venue.execute(&order, market) {
            Ok(fill) => {
                self.consecutive_failures = 0;
                self.totals.add(
                    self.scale.to_amount(Asset::Quote, fill.fees_claimed),
                    self.scale.to_amount(Asset::Quote, fill.action_cost),
                    fill.swap_cost_quote,
                );
                if booking::book(&mut self.ledger, market.timestamp, &order, &fill).is_err() {
                    self.ledger_broken = true;
                }
                outcome.orders.push(OrderResult::Filled(order.label()));
                Some(fill)
            }
            Err(error) => {
                self.fail(error, outcome);
                None
            }
        }
    }

    pub fn fail(&mut self, error: VenueError, outcome: &mut ExecutionOutcome) {
        self.consecutive_failures += 1;
        outcome.orders.push(OrderResult::Failed(error));
    }

    /// Both wallets agree with the ledger, unit for unit.
    pub fn reconciled(&self, operational: Holding, treasury: Holding) -> bool {
        operational == self.ledger.holding(Account::Operational)
            && treasury == self.ledger.holding(Account::Treasury)
    }

    fn order_context(&mut self, market: &MarketSnapshot) -> Result<OrderContext, VenueError> {
        let operational = self.venue.balances(Wallet::Operational)?;
        let treasury = self.venue.balances(Wallet::Treasury)?;
        let band_token = self
            .venue
            .position(market)?
            .map_or(0.0, |position| position.token);
        Ok(OrderContext {
            price: market.spot_price,
            quote_depth: market.quote_depth,
            operational,
            token_elsewhere: band_token + self.scale.to_amount(Asset::Token, treasury.token),
        })
    }
}

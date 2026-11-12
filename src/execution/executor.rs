//! Turns liquidity intents into venue orders. Every order passes the risk
//! gate, every fill is booked, and after each snapshot the books are marked
//! and reconciled with the venue. Problems become alerts that trip
//! protection on the next snapshot.

use crate::config::engine_config::EngineConfig;
use crate::config::engine_sections::LiquidityConfig;
use crate::config::execution_sections::RiskConfig;
use crate::engine::liquidity::action::LiquidityAction;
use crate::engine::liquidity::price_band::PriceBand;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::execution::band_funding::{self, Amounts};
use crate::execution::booking;
use crate::execution::outcome::{ExecutionOutcome, OrderResult};
use crate::execution::risk_gate::{OrderContext, RiskGate};
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::ledger::{Account, Ledger};
use crate::portfolio::pnl_tracker::{BooksSnapshot, PnlTracker};
use crate::venue::adapter::Venue;
use crate::venue::order::{VenueError, VenueOrder, Wallet};

pub struct Executor {
    venue: Box<dyn Venue>,
    ledger: Ledger,
    gate: RiskGate,
    pnl: PnlTracker,
    scale: UnitScale,
    liquidity: LiquidityConfig,
    risk: RiskConfig,
    initial: Amounts,
    consecutive_failures: u32,
    ledger_broken: bool,
}

struct Inspection {
    books: BooksSnapshot,
    band_held: bool,
    reconciled: bool,
    loss_breached: bool,
}

impl Executor {
    /// Opens the books with whatever the venue holds right now.
    pub fn open(config: &EngineConfig, mut venue: Box<dyn Venue>) -> Result<Self, VenueError> {
        let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
        let operational = venue.balances(Wallet::Operational)?;
        let treasury = venue.balances(Wallet::Treasury)?;
        let mut ledger = Ledger::default();
        ledger.deposit(
            config.growth.start_timestamp,
            Account::Operational,
            operational,
        );
        ledger.deposit(config.growth.start_timestamp, Account::Treasury, treasury);
        Ok(Self {
            venue,
            ledger,
            gate: RiskGate::new(&config.risk, scale, config.price.depth_reference_move),
            pnl: PnlTracker::new(&config.risk),
            scale,
            liquidity: config.liquidity.clone(),
            risk: config.risk.clone(),
            initial: Amounts::from_holding(operational.plus(treasury), scale),
            consecutive_failures: 0,
            ledger_broken: false,
        })
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Runs this snapshot's liquidity action and, when given, an approved
    /// treasury purchase of `treasury_buy_quote`.
    pub fn on_decision(
        &mut self,
        market: &MarketSnapshot,
        action: LiquidityAction,
        treasury_buy_quote: Option<f64>,
    ) -> ExecutionOutcome {
        let mut outcome = ExecutionOutcome::default();
        if let Err(error) = self.venue.observe(market) {
            self.fail(error, &mut outcome);
        }
        if let LiquidityAction::Open(band) | LiquidityAction::Reposition(band) = action {
            self.move_band(band, market, &mut outcome);
        }
        if let Some(quote) = treasury_buy_quote {
            let buy = VenueOrder::Swap {
                wallet: Wallet::Treasury,
                sell: Asset::Quote,
                units: self.scale.to_units(Asset::Quote, quote),
            };
            outcome.treasury_filled = self.run(buy, market, &mut outcome);
        }
        match self.inspect(market) {
            Ok(inspection) => {
                outcome.books = Some(inspection.books);
                outcome.feedback.band_held = inspection.band_held;
                outcome.feedback.alert =
                    self.alert(inspection.reconciled, inspection.loss_breached);
            }
            Err(error) => {
                self.fail(error, &mut outcome);
                outcome.feedback.alert = self.alert(true, false);
            }
        }
        outcome
    }

    /// Withdraw the old band, swap toward the new mix, deposit. Stops at the
    /// first order that is blocked or fails.
    fn move_band(
        &mut self,
        band: PriceBand,
        market: &MarketSnapshot,
        outcome: &mut ExecutionOutcome,
    ) {
        match self.venue.position(market) {
            Ok(Some(_)) if !self.run(VenueOrder::RemoveBand, market, outcome) => return,
            Ok(_) => {}
            Err(error) => return self.fail(error, outcome),
        }
        let price = market.spot_price;
        let Some(wallet) = self.operational_amounts(outcome) else {
            return;
        };
        let target =
            band_funding::band_target(band, price, wallet, self.liquidity.band_capital_share);
        if let Some((sell, amount)) =
            band_funding::rebalance_swap(target, wallet, price, &self.liquidity, &self.risk)
        {
            let units = self.scale.to_units(sell, amount);
            let swap = VenueOrder::Swap {
                wallet: Wallet::Operational,
                sell,
                units,
            };
            if !self.run(swap, market, outcome) {
                return;
            }
        }
        let Some(wallet) = self.operational_amounts(outcome) else {
            return;
        };
        let funded = band_funding::deposit(target, wallet, self.risk.min_quote_reserve);
        let add = VenueOrder::AddBand {
            band,
            token_units: self.scale.to_units(Asset::Token, funded.token),
            quote_units: self.scale.to_units(Asset::Quote, funded.quote),
        };
        self.run(add, market, outcome);
    }

    /// Gate, execute and book one order; `true` when it filled.
    fn run(
        &mut self,
        order: VenueOrder,
        market: &MarketSnapshot,
        outcome: &mut ExecutionOutcome,
    ) -> bool {
        let context = match self.order_context(market) {
            Ok(context) => context,
            Err(error) => {
                self.fail(error, outcome);
                return false;
            }
        };
        if let Err(breach) = self.gate.check(&order, &context) {
            outcome.orders.push(OrderResult::Blocked(breach));
            return false;
        }
        match self.venue.execute(&order, market) {
            Ok(fill) => {
                self.consecutive_failures = 0;
                self.pnl.record_fill(
                    self.scale.to_amount(Asset::Quote, fill.fees_claimed),
                    self.scale.to_amount(Asset::Quote, fill.action_cost),
                    fill.swap_cost_quote,
                );
                if booking::book(&mut self.ledger, market.timestamp, &order, &fill).is_err() {
                    self.ledger_broken = true;
                }
                outcome.orders.push(OrderResult::Filled(order.label()));
                true
            }
            Err(error) => {
                self.fail(error, outcome);
                false
            }
        }
    }

    /// An operator resumed: losses so far and past failures are acknowledged.
    /// A broken ledger is not; it needs a reconciliation, not a resume.
    pub fn acknowledge_losses(&mut self) {
        self.pnl.reset_baseline();
        self.consecutive_failures = 0;
    }

    fn fail(&mut self, error: VenueError, outcome: &mut ExecutionOutcome) {
        self.consecutive_failures += 1;
        outcome.orders.push(OrderResult::Failed(error));
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

    fn operational_amounts(&mut self, outcome: &mut ExecutionOutcome) -> Option<Amounts> {
        match self.venue.balances(Wallet::Operational) {
            Ok(holding) => Some(Amounts::from_holding(holding, self.scale)),
            Err(error) => {
                self.fail(error, outcome);
                None
            }
        }
    }

    /// Marks equity and compares the ledger with the venue's wallets.
    fn inspect(&mut self, market: &MarketSnapshot) -> Result<Inspection, VenueError> {
        let price = market.spot_price;
        let operational = self.venue.balances(Wallet::Operational)?;
        let treasury = self.venue.balances(Wallet::Treasury)?;
        let position = self.venue.position(market)?;
        let reconciled = operational == self.ledger.holding(Account::Operational)
            && treasury == self.ledger.holding(Account::Treasury);
        let (band_value, unclaimed) = position.map_or((0.0, 0.0), |position| {
            (
                position.token * price + position.quote,
                position.unclaimed_fees,
            )
        });
        let wallets = Amounts::from_holding(operational.plus(treasury), self.scale);
        let equity = wallets.value(price) + band_value + unclaimed;
        let loss_breached = self.pnl.mark(market.timestamp, equity);
        Ok(Inspection {
            books: self
                .pnl
                .snapshot(equity, self.initial.value(price), unclaimed),
            band_held: position.is_some(),
            reconciled,
            loss_breached,
        })
    }

    fn alert(&self, reconciled: bool, loss_breached: bool) -> Option<EmergencyCause> {
        if self.ledger_broken || !reconciled {
            Some(EmergencyCause::LedgerMismatch)
        } else if self.consecutive_failures >= self.risk.max_consecutive_failures {
            Some(EmergencyCause::VenueFailures)
        } else if loss_breached {
            Some(EmergencyCause::LossLimit)
        } else {
            None
        }
    }
}

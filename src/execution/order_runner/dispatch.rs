//! Sending one order: gate, daily limit, guard, venue, fill check, booking.

use crate::execution::booking;
use crate::execution::fill_check;
use crate::execution::order_runner::OrderRunner;
use crate::execution::outcome::{ExecutionOutcome, OrderResult};
use crate::execution::risk_gate::{OrderContext, RiskBreach};
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::Asset;
use crate::venue::order::{Fill, VenueError, VenueOrder, Wallet};

impl OrderRunner {
    /// Swaps `units` of `sell` at market. Its minimum out and allowed price
    /// drift are anchored to `reference_price`: spot for rebalancing, the
    /// TWAP for treasury buys, so a buy into a manipulated spike is refused.
    pub fn swap(
        &mut self,
        wallet: Wallet,
        sell: Asset,
        units: u128,
        market: &MarketSnapshot,
        reference_price: f64,
        outcome: &mut ExecutionOutcome,
    ) -> Option<Fill> {
        let min_received = self
            .guards
            .min_received(self.scale, sell, units, reference_price);
        let order = VenueOrder::Swap {
            wallet,
            sell,
            units,
            min_received,
        };
        self.run_at(order, market, reference_price, outcome)
    }

    /// Gate, guard, execute and book one order planned at spot; the fill
    /// when it went through.
    pub fn run(
        &mut self,
        order: VenueOrder,
        market: &MarketSnapshot,
        outcome: &mut ExecutionOutcome,
    ) -> Option<Fill> {
        self.run_at(order, market, market.spot_price, outcome)
    }

    fn run_at(
        &mut self,
        order: VenueOrder,
        market: &MarketSnapshot,
        reference_price: f64,
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
        if !self.send_orders {
            outcome
                .orders
                .push(OrderResult::Withheld(order.withheld_label()));
            return None;
        }
        if !self.count_against_daily_limit(&order, market.timestamp) {
            self.order_burst = true;
            outcome
                .orders
                .push(OrderResult::Blocked(RiskBreach::DailyOrderLimit));
            return None;
        }
        let guard = self.guards.next_guard(market, reference_price);
        match self.venue.execute(&order, &guard, market) {
            Ok(fill) => {
                self.consecutive_failures = 0;
                self.anomaly |= !fill_check::consistent(&order, &guard, &fill);
                self.totals.add(
                    self.scale.to_amount(Asset::Quote, fill.fees_claimed),
                    self.scale.to_amount(Asset::Quote, fill.action_cost),
                    fill.swap_cost_quote,
                );
                // Book what happened even when it looks wrong: the ledger must
                // follow the venue, and the anomaly alert stops further orders.
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

    /// Counts an order against today's limit; `false` when the limit is
    /// reached. Withdrawals are never limited: they only reduce risk.
    fn count_against_daily_limit(&mut self, order: &VenueOrder, timestamp: u64) -> bool {
        let day = timestamp / 86_400;
        let sent = match self.orders_today {
            Some((today, sent)) if today == day => sent,
            _ => 0,
        };
        let allowed = matches!(order, VenueOrder::RemoveBand) || sent < self.max_orders_per_day;
        if allowed {
            self.orders_today = Some((day, sent + 1));
        }
        allowed
    }

    pub fn fail(&mut self, error: VenueError, outcome: &mut ExecutionOutcome) {
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
}

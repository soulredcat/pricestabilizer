//! Builds the guard every order carries, from the risk limits.

use crate::config::execution_sections::RiskConfig;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::venue::order::OrderGuard;

pub struct GuardRules {
    ttl_secs: u64,
    max_price_drift: f64,
    max_action_cost: u128,
    max_swap_shortfall: f64,
    next_order_id: u64,
}

impl GuardRules {
    pub fn new(risk: &RiskConfig, scale: UnitScale) -> Self {
        Self {
            ttl_secs: risk.order_ttl_secs,
            max_price_drift: risk.max_price_drift,
            max_action_cost: scale.to_units(Asset::Quote, risk.max_action_cost_quote),
            max_swap_shortfall: risk.max_swap_shortfall,
            next_order_id: 1,
        }
    }

    pub fn next_order_id(&self) -> u64 {
        self.next_order_id
    }

    /// Continues numbering after a restart, so no order id is ever reused.
    pub fn resume_from(&mut self, next_order_id: u64) {
        self.next_order_id = self.next_order_id.max(next_order_id);
    }

    /// A fresh guard: a new order id, a deadline and the price the order is
    /// anchored to.
    pub fn next_guard(&mut self, market: &MarketSnapshot, reference_price: f64) -> OrderGuard {
        let order_id = self.next_order_id;
        self.next_order_id += 1;
        OrderGuard {
            order_id,
            deadline: market.timestamp + self.ttl_secs,
            reference_price,
            max_price_drift: self.max_price_drift,
            max_action_cost: self.max_action_cost,
        }
    }

    /// The least a swap of `units` of `sell` may return: its value at
    /// `price` minus the allowed shortfall (fees and impact together).
    pub fn min_received(&self, scale: UnitScale, sell: Asset, units: u128, price: f64) -> u128 {
        let amount = scale.to_amount(sell, units);
        let at_price = match sell {
            Asset::Token => amount * price,
            Asset::Quote => amount / price,
        };
        scale.to_units(
            sell.counterpart(),
            at_price * (1.0 - self.max_swap_shortfall),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::GuardRules;
    use crate::config::execution_sections::RiskConfig;
    use crate::market::snapshot::MarketSnapshot;
    use crate::portfolio::asset::{Asset, UnitScale};

    fn rules() -> GuardRules {
        let risk = RiskConfig {
            max_swap_quote: 1.0,
            max_slippage: 0.01,
            min_quote_reserve: 0.0,
            max_token_exposure_quote: 1.0,
            max_daily_loss_quote: 1.0,
            max_drawdown: 0.1,
            max_consecutive_failures: 3,
            order_ttl_secs: 60,
            max_price_drift: 0.01,
            max_swap_shortfall: 0.02,
            max_action_cost_quote: 0.5,
            max_orders_per_day: 100,
        };
        GuardRules::new(&risk, UnitScale::new(6, 6))
    }

    #[test]
    fn every_guard_has_a_new_id_and_a_deadline() {
        let mut rules = rules();
        let market = MarketSnapshot {
            timestamp: 1_000,
            spot_price: 2.0,
            quote_depth: 1.0,
            quote_volume: 1.0,
            quote_usd_price: 1.0,
        };
        let first = rules.next_guard(&market, 2.0);
        let second = rules.next_guard(&market, 2.0);
        assert_eq!((first.order_id, second.order_id), (1, 2));
        assert_eq!(first.deadline, 1_060);
        assert_eq!(first.max_action_cost, 500_000);
    }

    #[test]
    fn minimum_out_allows_only_the_configured_shortfall() {
        let rules = rules();
        let scale = UnitScale::new(6, 6);
        // 10 token at 2.0 is worth 20 quote; 2% shortfall allows 19.6.
        assert_eq!(
            rules.min_received(scale, Asset::Token, 10_000_000, 2.0),
            19_600_000
        );
        assert_eq!(
            rules.min_received(scale, Asset::Quote, 20_000_000, 2.0),
            9_800_000
        );
    }
}

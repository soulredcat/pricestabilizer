//! Sizes the band and plans the swap that funds it. Pure arithmetic: the
//! executor still sends every resulting order through the risk gate.

use serde::{Deserialize, Serialize};

use crate::config::engine_sections::LiquidityConfig;
use crate::config::execution_sections::RiskConfig;
use crate::engine::liquidity::band_math;
use crate::engine::liquidity::price_band::PriceBand;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::holding::Holding;

/// Human amounts of both assets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Amounts {
    pub token: f64,
    pub quote: f64,
}

impl Amounts {
    pub fn from_holding(holding: Holding, scale: UnitScale) -> Self {
        Self {
            token: scale.to_amount(Asset::Token, holding.token),
            quote: scale.to_amount(Asset::Quote, holding.quote),
        }
    }

    pub fn value(self, price: f64) -> f64 {
        self.token * price + self.quote
    }
}

/// What the band should hold: `capital_share` of the wallet's value, in the
/// mix the liquidity curve needs at `price`.
pub fn band_target(band: PriceBand, price: f64, wallet: Amounts, capital_share: f64) -> Amounts {
    let liquidity =
        wallet.value(price) * capital_share / band_math::value_per_liquidity(band, price);
    let (token, quote) = band_math::amounts_per_liquidity(band, price);
    Amounts {
        token: token * liquidity,
        quote: quote * liquidity,
    }
}

/// The swap that closes the gap between wallet and target without touching
/// the quote reserve, as (asset to sell, amount). `None` when the gap is too
/// small to be worth an order.
pub fn rebalance_swap(
    target: Amounts,
    wallet: Amounts,
    price: f64,
    liquidity: &LiquidityConfig,
    risk: &RiskConfig,
) -> Option<(Asset, f64)> {
    let slack = 1.0 + risk.max_slippage;
    let token_gap_value = (target.token - wallet.token) * price;
    let quote_gap = target.quote + risk.min_quote_reserve - wallet.quote;
    let (sell, notional) = if token_gap_value > 0.0 {
        let spendable = wallet.quote - target.quote - risk.min_quote_reserve;
        (Asset::Quote, (token_gap_value * slack).min(spendable))
    } else if quote_gap > 0.0 {
        let surplus = (wallet.token - target.token) * price;
        (Asset::Token, (quote_gap * slack).min(surplus))
    } else {
        return None;
    };
    let notional = notional.min(risk.max_swap_quote);
    if notional < target.value(price) * liquidity.rebalance_tolerance {
        return None;
    }
    Some(match sell {
        Asset::Quote => (Asset::Quote, notional),
        Asset::Token => (Asset::Token, notional / price),
    })
}

/// What actually goes in: the target, limited by the wallet and the reserve.
pub fn deposit(target: Amounts, wallet: Amounts, min_quote_reserve: f64) -> Amounts {
    Amounts {
        token: target.token.min(wallet.token),
        quote: target
            .quote
            .min((wallet.quote - min_quote_reserve).max(0.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Amounts, band_target, deposit, rebalance_swap};
    use crate::config::engine_sections::LiquidityConfig;
    use crate::config::execution_sections::RiskConfig;
    use crate::engine::liquidity::price_band::PriceBand;
    use crate::portfolio::asset::Asset;

    fn liquidity() -> LiquidityConfig {
        LiquidityConfig {
            range_sigmas: 2.0,
            range_horizon_days: 1.0,
            min_half_width: 0.02,
            max_half_width: 0.3,
            width_tolerance: 0.3,
            min_reposition_interval_secs: 0,
            band_capital_share: 0.5,
            rebalance_tolerance: 0.01,
            high_volatility_capital_share: 0.25,
            pool_fee_rate: 0.003,
            min_fee_to_lvr: 0.0,
            inventory_skew: 0.0,
        }
    }

    fn risk() -> RiskConfig {
        RiskConfig {
            max_swap_quote: 10_000.0,
            max_slippage: 0.01,
            min_quote_reserve: 100.0,
            max_token_exposure_quote: 1e9,
            max_daily_loss_quote: 1e9,
            max_drawdown: 0.5,
            max_consecutive_failures: 3,
            order_ttl_secs: 60,
            max_price_drift: 0.01,
            max_swap_shortfall: 0.02,
            max_action_cost_quote: 1.0,
            max_orders_per_day: 100,
        }
    }

    fn wallet(token: f64, quote: f64) -> Amounts {
        Amounts { token, quote }
    }

    #[test]
    fn target_takes_the_configured_share_of_value() {
        let target = band_target(
            PriceBand::around(2.0, 0.1),
            2.0,
            wallet(500.0, 1_000.0),
            0.5,
        );
        assert!((target.value(2.0) - 1_000.0).abs() < 1e-9);
    }

    #[test]
    fn quote_heavy_wallet_buys_token() {
        let band = PriceBand::around(1.0, 0.1);
        let target = band_target(band, 1.0, wallet(0.0, 2_000.0), 0.5);
        let (sell, amount) =
            rebalance_swap(target, wallet(0.0, 2_000.0), 1.0, &liquidity(), &risk()).expect("swap");
        assert_eq!(sell, Asset::Quote);
        assert!(amount > target.token && amount < 2_000.0 - target.quote - 100.0 + 1e-9);
    }

    #[test]
    fn balanced_wallet_needs_no_swap_and_keeps_the_reserve() {
        let band = PriceBand::around(1.0, 0.1);
        let target = band_target(band, 1.0, wallet(1_000.0, 1_000.0), 0.5);
        assert_eq!(
            rebalance_swap(target, wallet(1_000.0, 1_000.0), 1.0, &liquidity(), &risk()),
            None
        );
        let funded = deposit(target, wallet(1_000.0, target.quote + 50.0), 100.0);
        assert!((funded.quote - (target.quote - 50.0)).abs() < 1e-9);
    }
}

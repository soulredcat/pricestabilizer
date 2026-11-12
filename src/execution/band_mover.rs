//! Moves the band: withdraw the old one, swap toward the new mix, deposit.
//! Stops at the first order that is blocked or fails.

use crate::config::engine_sections::LiquidityConfig;
use crate::config::execution_sections::RiskConfig;
use crate::engine::liquidity::price_band::PriceBand;
use crate::execution::band_funding::{self, Amounts};
use crate::execution::order_runner::OrderRunner;
use crate::execution::outcome::ExecutionOutcome;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::Asset;
use crate::venue::order::{VenueOrder, Wallet};

pub fn move_band(
    runner: &mut OrderRunner,
    band: PriceBand,
    market: &MarketSnapshot,
    liquidity: &LiquidityConfig,
    risk: &RiskConfig,
    outcome: &mut ExecutionOutcome,
) {
    match runner.position(market, outcome) {
        Ok(Some(_))
            if runner
                .run(VenueOrder::RemoveBand, market, outcome)
                .is_none() =>
        {
            return;
        }
        Ok(_) => {}
        Err(()) => return,
    }
    let scale = runner.scale();
    let price = market.spot_price;
    let Some(holding) = runner.balances(Wallet::Operational, outcome) else {
        return;
    };
    let wallet = Amounts::from_holding(holding, scale);
    let target = band_funding::band_target(band, price, wallet, liquidity.band_capital_share);
    if let Some((sell, amount)) =
        band_funding::rebalance_swap(target, wallet, price, liquidity, risk)
    {
        let swap = VenueOrder::Swap {
            wallet: Wallet::Operational,
            sell,
            units: scale.to_units(sell, amount).min(holding.get(sell)),
        };
        if runner.run(swap, market, outcome).is_none() {
            return;
        }
    }
    let Some(holding) = runner.balances(Wallet::Operational, outcome) else {
        return;
    };
    let funded = band_funding::deposit(
        target,
        Amounts::from_holding(holding, scale),
        risk.min_quote_reserve,
    );
    // Base units of 18-decimal assets do not survive an f64 round trip;
    // never ask for more than the wallet holds.
    let add = VenueOrder::AddBand {
        band,
        token_units: scale
            .to_units(Asset::Token, funded.token)
            .min(holding.token),
        quote_units: scale
            .to_units(Asset::Quote, funded.quote)
            .min(holding.quote),
    };
    runner.run(add, market, outcome);
}

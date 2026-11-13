//! How the paper venue fills each kind of order: swaps pay fee plus
//! depth-based impact, bands follow the liquidity curve, and every order pays
//! a fixed network cost.

use crate::engine::liquidity::band_math;
use crate::engine::liquidity::price_band::PriceBand;
use crate::market::impact::estimated_impact;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::Asset;
use crate::portfolio::holding::Holding;
use crate::venue::order::{Fill, VenueError, Wallet};
use crate::venue::simulated::paper_venue::PaperVenue;
use crate::venue::simulated::position::PaperPosition;

impl PaperVenue {
    pub(super) fn wallet_mut(&mut self, wallet: Wallet) -> &mut Holding {
        match wallet {
            Wallet::Operational => &mut self.operational,
            Wallet::Treasury => &mut self.treasury,
        }
    }

    pub(super) fn action_cost(&self) -> u128 {
        self.scale
            .to_units(Asset::Quote, self.config.action_cost_quote)
    }

    /// Applies a fill to a wallet, refusing it when the wallet cannot pay.
    pub(super) fn settle(&mut self, wallet: Wallet, fill: Fill) -> Result<Fill, VenueError> {
        let balance = self.wallet_mut(wallet);
        *balance = balance
            .minus(fill.spent)
            .ok_or(VenueError::Insufficient(wallet))?
            .plus(fill.received);
        Ok(fill)
    }

    pub(super) fn swap(
        &mut self,
        wallet: Wallet,
        sell: Asset,
        units: u128,
        min_received: u128,
        market: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
        let price = market.spot_price;
        let amount = self.scale.to_amount(sell, units);
        let notional = match sell {
            Asset::Token => amount * price,
            Asset::Quote => amount,
        };
        let impact = estimated_impact(notional, market.quote_depth, self.depth_reference_move);
        if impact >= 1.0 {
            return Err(VenueError::NoLiquidity);
        }
        let kept = notional * (1.0 - impact) * (1.0 - self.config.swap_fee_rate);
        let bought = sell.counterpart();
        let bought_amount = match bought {
            Asset::Quote => kept,
            Asset::Token => kept / price,
        };
        let received = self.scale.to_units(bought, bought_amount);
        if received < min_received {
            return Err(VenueError::SlippageExceeded);
        }
        let action_cost = self.action_cost();
        let fill = Fill {
            received: Holding::of(bought, received),
            spent: Holding::of(sell, units).plus(Holding::of(Asset::Quote, action_cost)),
            fees_claimed: 0,
            action_cost,
            swap_cost_quote: notional - kept,
        };
        self.settle(wallet, fill)
    }

    pub(super) fn add_band(
        &mut self,
        band: PriceBand,
        token_units: u128,
        quote_units: u128,
        market: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
        if self.position.is_some() {
            return Err(VenueError::PositionExists);
        }
        let price = market.spot_price;
        let liquidity = band_math::liquidity_for(
            band,
            price,
            self.scale.to_amount(Asset::Token, token_units),
            self.scale.to_amount(Asset::Quote, quote_units),
        );
        let (token, quote) = band_math::amounts_per_liquidity(band, price);
        // The f64 curve may land a few units above what was offered.
        let used = Holding {
            token: self
                .scale
                .to_units(Asset::Token, token * liquidity)
                .min(token_units),
            quote: self
                .scale
                .to_units(Asset::Quote, quote * liquidity)
                .min(quote_units),
        };
        let action_cost = self.action_cost();
        let fill = Fill {
            spent: used.plus(Holding::of(Asset::Quote, action_cost)),
            action_cost,
            ..Fill::default()
        };
        let fill = self.settle(Wallet::Operational, fill)?;
        self.position = Some(PaperPosition {
            band,
            liquidity,
            unclaimed_fees: 0.0,
        });
        Ok(fill)
    }

    pub(super) fn remove_band(&mut self, market: &MarketSnapshot) -> Result<Fill, VenueError> {
        let position = self.position.as_ref().ok_or(VenueError::NoPosition)?;
        let (token, quote) = position.amounts(market.spot_price);
        let fees_claimed = self.scale.to_units(Asset::Quote, position.unclaimed_fees);
        let action_cost = self.action_cost();
        let fill = Fill {
            received: Holding {
                token: self.scale.to_units(Asset::Token, token),
                quote: self.scale.to_units(Asset::Quote, quote) + fees_claimed,
            },
            spent: Holding::of(Asset::Quote, action_cost),
            fees_claimed,
            action_cost,
            swap_cost_quote: 0.0,
        };
        let fill = self.settle(Wallet::Operational, fill)?;
        self.position = None;
        Ok(fill)
    }
}

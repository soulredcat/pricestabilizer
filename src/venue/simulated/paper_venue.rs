//! The paper venue: swaps pay fee plus depth-based impact, bands follow the
//! liquidity curve, and every order pays a fixed network cost.

use crate::config::venue_section::SimulatedVenueConfig;
use crate::engine::liquidity::band_math;
use crate::engine::liquidity::price_band::PriceBand;
use crate::market::impact::estimated_impact;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::order::{Fill, PositionView, VenueError, VenueOrder, Wallet};
use crate::venue::simulated::position::PaperPosition;

pub struct PaperVenue {
    config: SimulatedVenueConfig,
    depth_reference_move: f64,
    scale: UnitScale,
    operational: Holding,
    treasury: Holding,
    position: Option<PaperPosition>,
}

impl PaperVenue {
    pub fn new(config: &SimulatedVenueConfig, depth_reference_move: f64, scale: UnitScale) -> Self {
        let holding = |token, quote| Holding {
            token: scale.to_units(Asset::Token, token),
            quote: scale.to_units(Asset::Quote, quote),
        };
        Self {
            operational: holding(config.operational_token, config.operational_quote),
            treasury: holding(config.treasury_token, config.treasury_quote),
            config: config.clone(),
            depth_reference_move,
            scale,
            position: None,
        }
    }

    fn wallet_mut(&mut self, wallet: Wallet) -> &mut Holding {
        match wallet {
            Wallet::Operational => &mut self.operational,
            Wallet::Treasury => &mut self.treasury,
        }
    }

    fn action_cost(&self) -> u128 {
        self.scale
            .to_units(Asset::Quote, self.config.action_cost_quote)
    }

    /// Applies a fill to a wallet, refusing it when the wallet cannot pay.
    fn settle(&mut self, wallet: Wallet, fill: Fill) -> Result<Fill, VenueError> {
        let balance = self.wallet_mut(wallet);
        *balance = balance
            .minus(fill.spent)
            .ok_or(VenueError::Insufficient(wallet))?
            .plus(fill.received);
        Ok(fill)
    }

    fn swap(
        &mut self,
        wallet: Wallet,
        sell: Asset,
        units: u128,
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
        let action_cost = self.action_cost();
        let fill = Fill {
            received: Holding::of(bought, self.scale.to_units(bought, bought_amount)),
            spent: Holding::of(sell, units).plus(Holding::of(Asset::Quote, action_cost)),
            fees_claimed: 0,
            action_cost,
            swap_cost_quote: notional - kept,
        };
        self.settle(wallet, fill)
    }

    fn add_band(
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
        let used = Holding {
            token: self.scale.to_units(Asset::Token, token * liquidity),
            quote: self.scale.to_units(Asset::Quote, quote * liquidity),
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

    fn remove_band(&mut self, market: &MarketSnapshot) -> Result<Fill, VenueError> {
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

impl Venue for PaperVenue {
    fn observe(&mut self, market: &MarketSnapshot) -> Result<(), VenueError> {
        if let Some(position) = self.position.as_mut() {
            position.accrue_fees(market, self.config.fee_rate, self.depth_reference_move);
        }
        Ok(())
    }

    fn execute(&mut self, order: &VenueOrder, market: &MarketSnapshot) -> Result<Fill, VenueError> {
        match *order {
            VenueOrder::Swap {
                wallet,
                sell,
                units,
            } => self.swap(wallet, sell, units, market),
            VenueOrder::AddBand {
                band,
                token_units,
                quote_units,
            } => self.add_band(band, token_units, quote_units, market),
            VenueOrder::RemoveBand => self.remove_band(market),
        }
    }

    fn balances(&mut self, wallet: Wallet) -> Result<Holding, VenueError> {
        Ok(*self.wallet_mut(wallet))
    }

    fn position(&mut self, market: &MarketSnapshot) -> Result<Option<PositionView>, VenueError> {
        Ok(self.position.as_ref().map(|position| {
            let (token, quote) = position.amounts(market.spot_price);
            PositionView {
                token,
                quote,
                unclaimed_fees: position.unclaimed_fees,
            }
        }))
    }
}

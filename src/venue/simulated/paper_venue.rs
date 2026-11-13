//! The paper venue: swaps pay fee plus depth-based impact, bands follow the
//! liquidity curve, and every order pays a fixed network cost. It honours
//! order guards and repeated order ids exactly as a live connector must.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::venue_section::SimulatedVenueConfig;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::order::{Fill, OrderGuard, PositionView, VenueError, VenueOrder, Wallet};
use crate::venue::simulated::guard_check::check_guard;
use crate::venue::simulated::position::PaperPosition;

/// How many recent fills are remembered for answering repeated order ids.
const REMEMBERED_FILLS: usize = 1_024;

pub struct PaperVenue {
    pub(super) config: SimulatedVenueConfig,
    pub(super) depth_reference_move: f64,
    pub(super) scale: UnitScale,
    pub(super) operational: Holding,
    pub(super) treasury: Holding,
    pub(super) position: Option<PaperPosition>,
    pub(super) fills_by_order: BTreeMap<u64, Fill>,
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
            fills_by_order: BTreeMap::new(),
        }
    }
}

/// The paper venue's wallets and band: on a chain they would persist by
/// themselves, so a shadow run saves them with the engine state.
#[derive(Serialize, Deserialize)]
struct PaperState {
    operational: Holding,
    treasury: Holding,
    position: Option<PaperPosition>,
}

impl Venue for PaperVenue {
    fn observe(&mut self, market: &MarketSnapshot) -> Result<(), VenueError> {
        if let Some(position) = self.position.as_mut() {
            position.accrue_fees(market, self.config.fee_rate, self.depth_reference_move);
        }
        Ok(())
    }

    fn execute(
        &mut self,
        order: &VenueOrder,
        guard: &OrderGuard,
        market: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
        if let Some(fill) = self.fills_by_order.get(&guard.order_id) {
            return Ok(*fill);
        }
        check_guard(guard, market, self.action_cost())?;
        let fill = match *order {
            VenueOrder::Swap {
                wallet,
                sell,
                units,
                min_received,
            } => self.swap(wallet, sell, units, min_received, market),
            VenueOrder::AddBand {
                band,
                token_units,
                quote_units,
            } => self.add_band(band, token_units, quote_units, market),
            VenueOrder::RemoveBand => self.remove_band(market),
        }?;
        self.fills_by_order.insert(guard.order_id, fill);
        if self.fills_by_order.len() > REMEMBERED_FILLS {
            self.fills_by_order.pop_first();
        }
        Ok(fill)
    }

    fn balances(&mut self, wallet: Wallet) -> Result<Holding, VenueError> {
        Ok(*self.wallet_mut(wallet))
    }

    fn saved_state(&self) -> Option<Value> {
        serde_json::to_value(PaperState {
            operational: self.operational,
            treasury: self.treasury,
            position: self.position,
        })
        .ok()
    }

    fn restore_state(&mut self, state: Value) -> Result<(), VenueError> {
        let state: PaperState = serde_json::from_value(state).map_err(|_| VenueError::Connector)?;
        self.operational = state.operational;
        self.treasury = state.treasury;
        self.position = state.position;
        Ok(())
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

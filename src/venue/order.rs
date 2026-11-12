//! Vocabulary shared by the executor and every venue.

use crate::engine::liquidity::price_band::PriceBand;
use crate::portfolio::asset::Asset;
use crate::portfolio::holding::Holding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wallet {
    Operational,
    Treasury,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VenueOrder {
    /// Sell `units` of `sell` for the other asset at market.
    Swap {
        wallet: Wallet,
        sell: Asset,
        units: u128,
    },
    /// Provide liquidity over `band` from the operational wallet, using at
    /// most these units; the venue keeps whatever the band cannot use.
    AddBand {
        band: PriceBand,
        token_units: u128,
        quote_units: u128,
    },
    /// Withdraw the band position and claim its fees.
    RemoveBand,
}

impl VenueOrder {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Swap { .. } => "swap",
            Self::AddBand { .. } => "add_band",
            Self::RemoveBand => "remove_band",
        }
    }
}

/// What an order did to its wallet. Claimed fees are inside
/// `received.quote`; order costs are inside `spent.quote`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fill {
    pub received: Holding,
    pub spent: Holding,
    pub fees_claimed: u128,
    pub action_cost: u128,
    /// Swap fee plus price impact versus spot, in quote (information only).
    pub swap_cost_quote: f64,
}

/// The band position as the venue sees it, in human amounts.
#[derive(Debug, Clone, Copy)]
pub struct PositionView {
    pub token: f64,
    pub quote: f64,
    pub unclaimed_fees: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueError {
    Insufficient(Wallet),
    NoPosition,
    PositionExists,
    NoLiquidity,
}

impl VenueError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Insufficient(Wallet::Operational) => "failed_operational_funds",
            Self::Insufficient(Wallet::Treasury) => "failed_treasury_funds",
            Self::NoPosition => "failed_no_position",
            Self::PositionExists => "failed_position_exists",
            Self::NoLiquidity => "failed_no_liquidity",
        }
    }
}

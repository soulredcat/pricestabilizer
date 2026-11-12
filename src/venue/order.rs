//! Vocabulary shared by the executor and every venue. Everything here also
//! travels over the external connector protocol, so units are strings.

use serde::{Deserialize, Serialize};

use crate::engine::liquidity::price_band::PriceBand;
use crate::portfolio::asset::{Asset, deserialize_units, serialize_units};
use crate::portfolio::holding::Holding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wallet {
    Operational,
    Treasury,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VenueOrder {
    /// Sell `units` of `sell` for the other asset at market, receiving at
    /// least `min_received` units or nothing at all.
    Swap {
        wallet: Wallet,
        sell: Asset,
        #[serde(
            serialize_with = "serialize_units",
            deserialize_with = "deserialize_units"
        )]
        units: u128,
        #[serde(
            serialize_with = "serialize_units",
            deserialize_with = "deserialize_units"
        )]
        min_received: u128,
    },
    /// Provide liquidity over `band` from the operational wallet, using at
    /// most these units; the venue keeps whatever the band cannot use.
    AddBand {
        band: PriceBand,
        #[serde(
            serialize_with = "serialize_units",
            deserialize_with = "deserialize_units"
        )]
        token_units: u128,
        #[serde(
            serialize_with = "serialize_units",
            deserialize_with = "deserialize_units"
        )]
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

    /// Label for an order planned but not sent (`send_orders = false`).
    pub fn withheld_label(&self) -> &'static str {
        match self {
            Self::Swap { .. } => "withheld_swap",
            Self::AddBand { .. } => "withheld_add_band",
            Self::RemoveBand => "withheld_remove_band",
        }
    }
}

/// Protections every order carries. A venue must refuse an order it cannot
/// honour exactly, rather than execute it some other way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrderGuard {
    /// Unique per order, also across restarts. A venue that sees an id again
    /// must answer with the original result instead of executing twice.
    pub order_id: u64,
    /// Unix seconds after which the order must be refused (`expired`).
    pub deadline: u64,
    /// The price the order was planned at.
    pub reference_price: f64,
    /// Refuse (`price_moved`) when the live price is further than this,
    /// relative, from `reference_price`.
    pub max_price_drift: f64,
    /// Refuse (`cost_too_high`) when network costs would exceed this.
    #[serde(
        serialize_with = "serialize_units",
        deserialize_with = "deserialize_units"
    )]
    pub max_action_cost: u128,
}

/// What an order did to its wallet. Claimed fees are inside
/// `received.quote`; order costs are inside `spent.quote`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Fill {
    pub received: Holding,
    pub spent: Holding,
    #[serde(
        serialize_with = "serialize_units",
        deserialize_with = "deserialize_units"
    )]
    pub fees_claimed: u128,
    #[serde(
        serialize_with = "serialize_units",
        deserialize_with = "deserialize_units"
    )]
    pub action_cost: u128,
    /// Swap fee plus price impact versus spot, in quote (information only).
    pub swap_cost_quote: f64,
}

/// The band position as the venue sees it, in human amounts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
    /// Refusals: the order's guard could not be honoured; nothing executed.
    Expired,
    PriceMoved,
    SlippageExceeded,
    CostTooHigh,
    /// The connector broke the protocol, crashed or reported something else.
    Connector,
}

impl VenueError {
    const KNOWN: [Self; 9] = [
        Self::Insufficient(Wallet::Operational),
        Self::Insufficient(Wallet::Treasury),
        Self::NoPosition,
        Self::PositionExists,
        Self::NoLiquidity,
        Self::Expired,
        Self::PriceMoved,
        Self::SlippageExceeded,
        Self::CostTooHigh,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Insufficient(Wallet::Operational) => "failed_operational_funds",
            Self::Insufficient(Wallet::Treasury) => "failed_treasury_funds",
            Self::NoPosition => "failed_no_position",
            Self::PositionExists => "failed_position_exists",
            Self::NoLiquidity => "failed_no_liquidity",
            Self::Expired => "failed_expired",
            Self::PriceMoved => "failed_price_moved",
            Self::SlippageExceeded => "failed_slippage_exceeded",
            Self::CostTooHigh => "failed_cost_too_high",
            Self::Connector => "failed_connector",
        }
    }

    /// Wire code used by the connector protocol.
    pub fn code(self) -> &'static str {
        match self {
            Self::Insufficient(Wallet::Operational) => "insufficient_operational",
            Self::Insufficient(Wallet::Treasury) => "insufficient_treasury",
            Self::NoPosition => "no_position",
            Self::PositionExists => "position_exists",
            Self::NoLiquidity => "no_liquidity",
            Self::Expired => "expired",
            Self::PriceMoved => "price_moved",
            Self::SlippageExceeded => "slippage_exceeded",
            Self::CostTooHigh => "cost_too_high",
            Self::Connector => "connector",
        }
    }

    /// Any code the engine does not know is a connector problem.
    pub fn from_code(code: &str) -> Self {
        Self::KNOWN
            .into_iter()
            .find(|error| error.code() == code)
            .unwrap_or(Self::Connector)
    }
}

#[cfg(test)]
mod tests {
    use super::{VenueError, VenueOrder, Wallet};
    use crate::portfolio::asset::Asset;

    #[test]
    fn orders_travel_with_string_units() {
        let order = VenueOrder::Swap {
            wallet: Wallet::Treasury,
            sell: Asset::Quote,
            units: u128::MAX,
            min_received: 7,
        };
        let text = serde_json::to_string(&order).expect("serialises");
        assert_eq!(
            text,
            format!(
                "{{\"type\":\"swap\",\"wallet\":\"treasury\",\"sell\":\"quote\",\"units\":\"{}\",\"min_received\":\"7\"}}",
                u128::MAX
            )
        );
        assert_eq!(
            serde_json::from_str::<VenueOrder>(&text).expect("parses"),
            order
        );
    }

    #[test]
    fn every_error_code_round_trips_and_unknown_means_connector() {
        for known in VenueError::KNOWN {
            assert_eq!(VenueError::from_code(known.code()), known);
        }
        assert_eq!(VenueError::from_code("rpc timeout"), VenueError::Connector);
    }
}

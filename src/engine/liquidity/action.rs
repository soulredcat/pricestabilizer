//! What the liquidity engine wants done. Intents only: execution is a separate,
//! risk-checked stage, so nothing here can move funds.

use crate::engine::liquidity::price_band::PriceBand;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LiquidityAction {
    Hold(HoldReason),
    /// Provide liquidity for the first time in this band.
    Open(PriceBand),
    /// Move existing liquidity to this band.
    Reposition(PriceBand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldReason {
    /// Protection mode is latched.
    Protection,
    /// No trusted reference price, so no automatic change.
    UntrustedPrice,
    /// The current band still fits the market.
    InBand,
    /// A band change happened too recently.
    Cooldown,
    /// Expected fees do not cover the loss to arbitrage.
    Unprofitable,
}

impl LiquidityAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hold(HoldReason::Protection) => "hold_protection",
            Self::Hold(HoldReason::UntrustedPrice) => "hold_untrusted_price",
            Self::Hold(HoldReason::InBand) => "hold_in_band",
            Self::Hold(HoldReason::Cooldown) => "hold_cooldown",
            Self::Hold(HoldReason::Unprofitable) => "hold_unprofitable",
            Self::Open(_) => "open",
            Self::Reposition(_) => "reposition",
        }
    }
}

//! The three modes the engine can be in.

use crate::engine::regime::market_regime::EmergencyCause;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingMode {
    /// Market making, and a treasury accumulation *review* is warranted:
    /// trusted price, calm regime, reference below the growth target.
    /// A review is not an order. It never buys by itself.
    AccumulationReview,
    /// Liquidity management only.
    MarketMaking,
    /// Safe mode: no new risky actions until an operator resumes.
    Protection(EmergencyCause),
}

impl OperatingMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::AccumulationReview => "accumulation_review",
            Self::MarketMaking => "market_making",
            Self::Protection(_) => "protection",
        }
    }
}

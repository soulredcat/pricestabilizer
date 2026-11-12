//! Everything the engine concluded from one snapshot.

use crate::engine::liquidity::action::LiquidityAction;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::price::confidence::PriceAssessment;
use crate::engine::regime::classifier::RegimeReading;

#[derive(Debug, Clone)]
pub struct TickDecision {
    pub target_price: f64,
    pub price: PriceAssessment,
    pub regime: RegimeReading,
    pub mode: OperatingMode,
    pub liquidity: LiquidityAction,
    /// Whether spot sits inside the current band; `None` before any band exists.
    pub spot_in_band: Option<bool>,
}

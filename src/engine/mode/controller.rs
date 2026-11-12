//! Picks the operating mode. Protection latches: once tripped it stays on
//! even if the market calms down, because resuming needs an operator's
//! approval, and this shadow engine has no operator channel yet.

use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::regime::market_regime::{EmergencyCause, Regime};

#[derive(Debug, Default)]
pub struct ModeController {
    latched: Option<EmergencyCause>,
}

impl ModeController {
    pub fn update(&mut self, regime: Regime, reference_below_target: bool) -> OperatingMode {
        if let Regime::Emergency(cause) = regime {
            self.latched = self.latched.or(Some(cause));
        }
        if let Some(cause) = self.latched {
            return OperatingMode::Protection(cause);
        }
        let calm = matches!(regime, Regime::Bullish | Regime::Sideways | Regime::Bearish);
        if calm && reference_below_target {
            OperatingMode::AccumulationReview
        } else {
            OperatingMode::MarketMaking
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ModeController;
    use crate::engine::mode::operating_mode::OperatingMode;
    use crate::engine::regime::market_regime::{EmergencyCause, Regime};

    #[test]
    fn protection_latches_with_its_first_cause() {
        let mut controller = ModeController::default();
        controller.update(Regime::Emergency(EmergencyCause::ExtremeMove), false);
        controller.update(Regime::Emergency(EmergencyCause::StableDepeg), false);
        assert_eq!(
            controller.update(Regime::Sideways, true),
            OperatingMode::Protection(EmergencyCause::ExtremeMove)
        );
    }

    #[test]
    fn review_needs_a_calm_regime_below_target() {
        let mut controller = ModeController::default();
        assert_eq!(
            controller.update(Regime::Sideways, true),
            OperatingMode::AccumulationReview
        );
        assert_eq!(
            controller.update(Regime::HighVolatility, true),
            OperatingMode::MarketMaking
        );
        assert_eq!(
            controller.update(Regime::Bullish, false),
            OperatingMode::MarketMaking
        );
    }
}

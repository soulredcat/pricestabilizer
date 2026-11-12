//! Picks the operating mode. Protection latches: once tripped, by the market
//! or by an execution alert, it stays on even if the market calms down,
//! because resuming needs an operator's approval.

use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::regime::market_regime::{EmergencyCause, Regime};

#[derive(Debug, Default)]
pub struct ModeController {
    latched: Option<EmergencyCause>,
}

impl ModeController {
    pub fn update(
        &mut self,
        regime: Regime,
        reference_below_target: bool,
        alert: Option<EmergencyCause>,
    ) -> OperatingMode {
        let market_cause = match regime {
            Regime::Emergency(cause) => Some(cause),
            _ => None,
        };
        self.latched = self.latched.or(market_cause).or(alert);
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
        controller.update(Regime::Emergency(EmergencyCause::ExtremeMove), false, None);
        controller.update(Regime::Emergency(EmergencyCause::StableDepeg), false, None);
        assert_eq!(
            controller.update(Regime::Sideways, true, None),
            OperatingMode::Protection(EmergencyCause::ExtremeMove)
        );
    }

    #[test]
    fn execution_alerts_latch_protection_too() {
        let mut controller = ModeController::default();
        controller.update(Regime::Sideways, false, Some(EmergencyCause::LossLimit));
        assert_eq!(
            controller.update(Regime::Sideways, false, None),
            OperatingMode::Protection(EmergencyCause::LossLimit)
        );
    }

    #[test]
    fn review_needs_a_calm_regime_below_target() {
        let mut controller = ModeController::default();
        assert_eq!(
            controller.update(Regime::Sideways, true, None),
            OperatingMode::AccumulationReview
        );
        assert_eq!(
            controller.update(Regime::HighVolatility, true, None),
            OperatingMode::MarketMaking
        );
        assert_eq!(
            controller.update(Regime::Bullish, false, None),
            OperatingMode::MarketMaking
        );
    }
}

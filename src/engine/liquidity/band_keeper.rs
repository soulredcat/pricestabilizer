//! Keeps the liquidity band: opens one, holds it, or moves it when the
//! trusted reference leaves the band or volatility calls for another width.
//! The reference is the TWAP, not spot, so one manipulated print cannot force
//! a costly move. Every change, including a first open, respects the cooldown,
//! so a venue that keeps refusing is not hammered every snapshot.

use crate::config::engine_sections::LiquidityConfig;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::price_band::PriceBand;

pub struct BandKeeper {
    config: LiquidityConfig,
    current: Option<PriceBand>,
    last_change: Option<u64>,
}

impl BandKeeper {
    pub fn new(config: &LiquidityConfig) -> Self {
        Self {
            config: config.clone(),
            current: None,
            last_change: None,
        }
    }

    pub fn current(&self) -> Option<PriceBand> {
        self.current
    }

    /// The venue holds no band (an order was blocked or failed): plan from
    /// scratch next time.
    pub fn forget(&mut self) {
        self.current = None;
    }

    pub fn plan(
        &mut self,
        timestamp: u64,
        reference_price: f64,
        daily_volatility: f64,
    ) -> LiquidityAction {
        let target = PriceBand::around(reference_price, self.half_width_for(daily_volatility));
        let Some(current) = self.current else {
            if self.cooling_down(timestamp) {
                return LiquidityAction::Hold(HoldReason::Cooldown);
            }
            self.adopt(target, timestamp);
            return LiquidityAction::Open(target);
        };
        if !self.needs_move(current, reference_price, target) {
            return LiquidityAction::Hold(HoldReason::InBand);
        }
        if self.cooling_down(timestamp) {
            return LiquidityAction::Hold(HoldReason::Cooldown);
        }
        self.adopt(target, timestamp);
        LiquidityAction::Reposition(target)
    }

    /// Covers `range_sigmas` standard deviations of the move expected over the
    /// horizon, so calm markets get tight bands and wild ones get wide bands.
    fn half_width_for(&self, daily_volatility: f64) -> f64 {
        let expected_move =
            self.config.range_sigmas * daily_volatility * self.config.range_horizon_days.sqrt();
        expected_move.clamp(self.config.min_half_width, self.config.max_half_width)
    }

    fn needs_move(&self, current: PriceBand, reference_price: f64, target: PriceBand) -> bool {
        let width_change = (target.half_width() / current.half_width() - 1.0).abs();
        !current.contains(reference_price) || width_change > self.config.width_tolerance
    }

    fn cooling_down(&self, timestamp: u64) -> bool {
        self.last_change.is_some_and(|last| {
            timestamp.saturating_sub(last) < self.config.min_reposition_interval_secs
        })
    }

    fn adopt(&mut self, band: PriceBand, timestamp: u64) {
        self.current = Some(band);
        self.last_change = Some(timestamp);
    }
}

#[cfg(test)]
mod tests {
    use super::BandKeeper;
    use crate::config::engine_sections::LiquidityConfig;
    use crate::engine::liquidity::action::{HoldReason, LiquidityAction};

    fn keeper() -> BandKeeper {
        BandKeeper::new(&LiquidityConfig {
            range_sigmas: 2.0,
            range_horizon_days: 1.0,
            min_half_width: 0.02,
            max_half_width: 0.3,
            width_tolerance: 0.3,
            min_reposition_interval_secs: 3_600,
            band_capital_share: 0.5,
            rebalance_tolerance: 0.02,
        })
    }

    #[test]
    fn opens_then_holds_while_inside() {
        let mut keeper = keeper();
        assert!(matches!(
            keeper.plan(0, 1.0, 0.03),
            LiquidityAction::Open(_)
        ));
        assert_eq!(
            keeper.plan(60, 1.02, 0.03),
            LiquidityAction::Hold(HoldReason::InBand)
        );
    }

    #[test]
    fn leaving_the_band_respects_the_cooldown() {
        let mut keeper = keeper();
        keeper.plan(0, 1.0, 0.03);
        assert_eq!(
            keeper.plan(60, 1.2, 0.03),
            LiquidityAction::Hold(HoldReason::Cooldown)
        );
        let LiquidityAction::Reposition(band) = keeper.plan(3_600, 1.2, 0.03) else {
            panic!("expected a reposition after the cooldown");
        };
        assert!(band.contains(1.2));
    }

    #[test]
    fn volatility_spike_widens_the_band() {
        let mut keeper = keeper();
        keeper.plan(0, 1.0, 0.03);
        let LiquidityAction::Reposition(band) = keeper.plan(4_000, 1.0, 0.12) else {
            panic!("expected a wider band");
        };
        assert!((band.half_width() - 0.24).abs() < 1e-9);
    }
}

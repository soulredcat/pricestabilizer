//! Keeps the liquidity band: opens one, holds it, or moves it when the
//! trusted reference leaves the band or volatility calls for another width.
//! The reference is the TWAP, not spot, so one manipulated print cannot force
//! a costly move. Every change, including a first open, respects the cooldown,
//! so a venue that keeps refusing is not hammered every snapshot.

use serde::{Deserialize, Serialize};

use crate::config::engine_sections::LiquidityConfig;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::price_band::PriceBand;

pub struct BandKeeper {
    config: LiquidityConfig,
    current: Option<PriceBand>,
    last_change: Option<u64>,
}

/// What of the band keeper survives a restart.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct BandKeeperState {
    pub current: Option<PriceBand>,
    pub last_change: Option<u64>,
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

    pub fn state(&self) -> BandKeeperState {
        BandKeeperState {
            current: self.current,
            last_change: self.last_change,
        }
    }

    pub fn restore(&mut self, state: BandKeeperState) {
        self.current = state.current;
        self.last_change = state.last_change;
    }

    /// The venue holds no band (an order was blocked or failed): plan from
    /// scratch next time.
    pub fn forget(&mut self) {
        self.current = None;
    }

    /// `token_share` is the token's share of inventory value; an imbalance
    /// shifts the band centre so the excess is the side that trades first.
    pub fn plan(
        &mut self,
        timestamp: u64,
        reference_price: f64,
        daily_volatility: f64,
        token_share: Option<f64>,
    ) -> LiquidityAction {
        let half_width = self.half_width_for(daily_volatility);
        let center = reference_price * self.skew(token_share, half_width);
        let target = PriceBand::around(center, half_width);
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

    /// Token-heavy inventory lifts the band so more of it sits above the
    /// price (sells as the price rises); quote-heavy lowers it. The shift
    /// stays below the half-width, so the price is always inside the band.
    fn skew(&self, token_share: Option<f64>, half_width: f64) -> f64 {
        let imbalance = token_share.map_or(0.0, |share| ((share - 0.5) * 2.0).clamp(-1.0, 1.0));
        1.0 + self.config.inventory_skew * imbalance * half_width
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
            high_volatility_capital_share: 0.25,
            pool_fee_rate: 0.003,
            min_fee_to_lvr: 0.0,
            inventory_skew: 0.5,
        })
    }

    #[test]
    fn opens_then_holds_while_inside() {
        let mut keeper = keeper();
        assert!(matches!(
            keeper.plan(0, 1.0, 0.03, None),
            LiquidityAction::Open(_)
        ));
        assert_eq!(
            keeper.plan(60, 1.02, 0.03, None),
            LiquidityAction::Hold(HoldReason::InBand)
        );
    }

    #[test]
    fn leaving_the_band_respects_the_cooldown() {
        let mut keeper = keeper();
        keeper.plan(0, 1.0, 0.03, None);
        assert_eq!(
            keeper.plan(60, 1.2, 0.03, None),
            LiquidityAction::Hold(HoldReason::Cooldown)
        );
        let LiquidityAction::Reposition(band) = keeper.plan(3_600, 1.2, 0.03, None) else {
            panic!("expected a reposition after the cooldown");
        };
        assert!(band.contains(1.2));
    }

    #[test]
    fn excess_token_lifts_the_band_yet_keeps_the_price_inside() {
        let mut keeper = keeper();
        let LiquidityAction::Open(band) = keeper.plan(0, 1.0, 0.03, Some(1.0)) else {
            panic!("expected a band");
        };
        let center = (band.lower * band.upper).sqrt();
        assert!((center - 1.03).abs() < 1e-9, "half of the 6% half-width");
        assert!(band.contains(1.0));
    }

    #[test]
    fn volatility_spike_widens_the_band() {
        let mut keeper = keeper();
        keeper.plan(0, 1.0, 0.03, None);
        let LiquidityAction::Reposition(band) = keeper.plan(4_000, 1.0, 0.12, None) else {
            panic!("expected a wider band");
        };
        assert!((band.half_width() - 0.24).abs() < 1e-9);
    }
}

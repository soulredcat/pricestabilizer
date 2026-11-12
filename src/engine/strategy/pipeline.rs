//! Snapshot → price → regime → mode → liquidity intent.
//!
//! The pipeline only decides. It never signs or sends anything; the
//! executor's feedback from the previous snapshot tells it whether its band
//! really exists and whether the books raised an alert.

use serde::{Deserialize, Serialize};

use crate::config::engine_config::EngineConfig;
use crate::config::engine_sections::LiquidityConfig;
use crate::engine::growth::target_curve::TargetCurve;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::band_keeper::{BandKeeper, BandKeeperState};
use crate::engine::liquidity::economics;
use crate::engine::mode::controller::ModeController;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::price::assessor::PriceAssessor;
use crate::engine::price::confidence::PriceAssessment;
use crate::engine::regime::classifier::{RegimeClassifier, RegimeReading};
use crate::engine::regime::market_regime::{EmergencyCause, Regime};
use crate::engine::strategy::decision::TickDecision;
use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::market::snapshot::MarketSnapshot;

/// What of the pipeline survives a restart. Price and regime windows do not:
/// after a restart they warm up again, so nothing is decided on stale data.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PipelineState {
    pub protection: Option<EmergencyCause>,
    pub band: BandKeeperState,
}

pub struct StrategyPipeline {
    target_curve: TargetCurve,
    price: PriceAssessor,
    regime: RegimeClassifier,
    mode: ModeController,
    band: BandKeeper,
    last_regime: Option<Regime>,
    liquidity: LiquidityConfig,
    depth_reference_move: f64,
}

impl StrategyPipeline {
    pub fn new(config: &EngineConfig) -> Self {
        Self {
            target_curve: TargetCurve::new(&config.growth),
            price: PriceAssessor::new(&config.price),
            regime: RegimeClassifier::new(&config.regime),
            mode: ModeController::default(),
            band: BandKeeper::new(&config.liquidity),
            last_regime: None,
            liquidity: config.liquidity.clone(),
            depth_reference_move: config.price.depth_reference_move,
        }
    }

    pub fn state(&self) -> PipelineState {
        PipelineState {
            protection: self.mode.latched(),
            band: self.band.state(),
        }
    }

    pub fn restore(&mut self, state: PipelineState) {
        self.mode = ModeController::with_latch(state.protection);
        self.band.restore(state.band);
    }

    pub fn pause(&mut self) {
        self.mode.pause();
    }

    /// Leaves protection unless the market is still in an emergency.
    pub fn resume(&mut self) -> bool {
        if matches!(self.last_regime, Some(Regime::Emergency(_))) {
            return false;
        }
        self.mode.resume();
        true
    }

    pub fn on_snapshot(
        &mut self,
        snapshot: &MarketSnapshot,
        feedback: ExecutionFeedback,
    ) -> TickDecision {
        if !feedback.band_held {
            self.band.forget();
        }
        let price = self.price.assess(snapshot);
        let regime = self.regime.classify(snapshot, &price);
        self.last_regime = Some(regime.regime);
        let target_price = self.target_curve.price_at(snapshot.timestamp);
        let below_target = price
            .reference_price()
            .is_some_and(|reference| reference < target_price);
        let mode = self
            .mode
            .update(regime.regime, below_target, feedback.alert);
        let fee_to_lvr = economics::fee_to_lvr(
            self.liquidity.pool_fee_rate,
            price.daily_volume,
            snapshot.quote_depth,
            self.depth_reference_move,
            regime.daily_volatility,
        );
        let liquidity = self.liquidity_action(
            snapshot.timestamp,
            &price,
            regime,
            mode,
            fee_to_lvr,
            feedback.token_share,
        );
        let spot_in_band = self
            .band
            .current()
            .map(|band| band.contains(snapshot.spot_price));
        let capital_share = if regime.regime == Regime::HighVolatility {
            self.liquidity.high_volatility_capital_share
        } else {
            self.liquidity.band_capital_share
        };
        TickDecision {
            target_price,
            price,
            regime,
            mode,
            liquidity,
            spot_in_band,
            fee_to_lvr,
            capital_share,
        }
    }

    /// Holds through protection and untrusted prices, and, when the gate is
    /// on, while expected fees do not cover the loss to arbitrage.
    fn liquidity_action(
        &mut self,
        timestamp: u64,
        price: &PriceAssessment,
        regime: RegimeReading,
        mode: OperatingMode,
        fee_to_lvr: Option<f64>,
        token_share: Option<f64>,
    ) -> LiquidityAction {
        if matches!(mode, OperatingMode::Protection(_)) {
            return LiquidityAction::Hold(HoldReason::Protection);
        }
        let Some(reference) = price.reference_price() else {
            return LiquidityAction::Hold(HoldReason::UntrustedPrice);
        };
        let gate = self.liquidity.min_fee_to_lvr;
        if gate > 0.0 && fee_to_lvr.is_some_and(|ratio| ratio < gate) {
            return LiquidityAction::Hold(HoldReason::Unprofitable);
        }
        self.band
            .plan(timestamp, reference, regime.daily_volatility, token_share)
    }
}

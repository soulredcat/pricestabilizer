//! Snapshot → price → regime → mode → liquidity intent.
//!
//! The pipeline only decides. It never signs or sends anything; the
//! executor's feedback from the previous snapshot tells it whether its band
//! really exists and whether the books raised an alert.

use crate::config::engine_config::EngineConfig;
use crate::engine::growth::target_curve::TargetCurve;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::band_keeper::BandKeeper;
use crate::engine::mode::controller::ModeController;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::price::assessor::PriceAssessor;
use crate::engine::price::confidence::PriceAssessment;
use crate::engine::regime::classifier::{RegimeClassifier, RegimeReading};
use crate::engine::strategy::decision::TickDecision;
use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::market::snapshot::MarketSnapshot;

pub struct StrategyPipeline {
    target_curve: TargetCurve,
    price: PriceAssessor,
    regime: RegimeClassifier,
    mode: ModeController,
    band: BandKeeper,
}

impl StrategyPipeline {
    pub fn new(config: &EngineConfig) -> Self {
        Self {
            target_curve: TargetCurve::new(&config.growth),
            price: PriceAssessor::new(&config.price),
            regime: RegimeClassifier::new(&config.regime),
            mode: ModeController::default(),
            band: BandKeeper::new(&config.liquidity),
        }
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
        let target_price = self.target_curve.price_at(snapshot.timestamp);
        let below_target = price
            .reference_price()
            .is_some_and(|reference| reference < target_price);
        let mode = self
            .mode
            .update(regime.regime, below_target, feedback.alert);
        let liquidity = self.liquidity_action(snapshot.timestamp, &price, regime, mode);
        let spot_in_band = self
            .band
            .current()
            .map(|band| band.contains(snapshot.spot_price));
        TickDecision {
            target_price,
            price,
            regime,
            mode,
            liquidity,
            spot_in_band,
        }
    }

    fn liquidity_action(
        &mut self,
        timestamp: u64,
        price: &PriceAssessment,
        regime: RegimeReading,
        mode: OperatingMode,
    ) -> LiquidityAction {
        if matches!(mode, OperatingMode::Protection(_)) {
            return LiquidityAction::Hold(HoldReason::Protection);
        }
        match price.reference_price() {
            Some(reference) => self
                .band
                .plan(timestamp, reference, regime.daily_volatility),
            None => LiquidityAction::Hold(HoldReason::UntrustedPrice),
        }
    }
}

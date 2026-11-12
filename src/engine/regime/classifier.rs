//! Classifies each snapshot. Emergencies are checked first: they must win
//! over any market reading, trusted or not.

use crate::config::engine_sections::RegimeConfig;
use crate::engine::price::confidence::PriceAssessment;
use crate::engine::regime::market_regime::{EmergencyCause, Regime};
use crate::engine::regime::market_window::MarketWindow;
use crate::market::snapshot::MarketSnapshot;

#[derive(Debug, Clone, Copy)]
pub struct RegimeReading {
    pub regime: Regime,
    pub daily_volatility: f64,
}

pub struct RegimeClassifier {
    config: RegimeConfig,
    window: MarketWindow,
}

impl RegimeClassifier {
    pub fn new(config: &RegimeConfig) -> Self {
        Self {
            config: config.clone(),
            window: MarketWindow::new(config.window_secs),
        }
    }

    pub fn classify(
        &mut self,
        snapshot: &MarketSnapshot,
        price: &PriceAssessment,
    ) -> RegimeReading {
        self.window.push(
            snapshot.timestamp,
            snapshot.spot_price,
            snapshot.quote_depth,
        );
        let daily_volatility = self.window.daily_volatility();
        let regime = match self.emergency_cause(snapshot) {
            Some(cause) => Regime::Emergency(cause),
            None => self.market_regime(price, daily_volatility),
        };
        RegimeReading {
            regime,
            daily_volatility,
        }
    }

    fn emergency_cause(&self, snapshot: &MarketSnapshot) -> Option<EmergencyCause> {
        let peg_deviation = (snapshot.quote_usd_price / self.config.quote_peg_usd - 1.0).abs();
        if peg_deviation > self.config.depeg_threshold {
            Some(EmergencyCause::StableDepeg)
        } else if self.window.last_log_return().abs() > self.config.extreme_move {
            Some(EmergencyCause::ExtremeMove)
        } else if self.window.depth_drop() > self.config.max_depth_drop {
            Some(EmergencyCause::DepthCollapse)
        } else {
            None
        }
    }

    fn market_regime(&self, price: &PriceAssessment, daily_volatility: f64) -> Regime {
        let trend = self.window.trend();
        if !price.is_trusted() {
            Regime::LowConfidence
        } else if daily_volatility > self.config.high_daily_volatility {
            Regime::HighVolatility
        } else if trend > self.config.trend_threshold {
            Regime::Bullish
        } else if trend < -self.config.trend_threshold {
            Regime::Bearish
        } else {
            Regime::Sideways
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RegimeClassifier;
    use crate::config::engine_sections::RegimeConfig;
    use crate::engine::price::confidence::{PriceAssessment, PriceIssue};
    use crate::engine::regime::market_regime::{EmergencyCause, Regime};
    use crate::market::snapshot::MarketSnapshot;

    fn classifier() -> RegimeClassifier {
        RegimeClassifier::new(&RegimeConfig {
            window_secs: 3_600,
            trend_threshold: 0.03,
            high_daily_volatility: 0.5,
            extreme_move: 0.15,
            quote_peg_usd: 1.0,
            depeg_threshold: 0.02,
            max_depth_drop: 0.5,
        })
    }

    fn snapshot(timestamp: u64, spot_price: f64, quote_usd_price: f64) -> MarketSnapshot {
        MarketSnapshot {
            timestamp,
            spot_price,
            quote_depth: 1_000.0,
            quote_volume: 1.0,
            quote_usd_price,
        }
    }

    fn trusted() -> PriceAssessment {
        PriceAssessment {
            twap: 1.0,
            issues: vec![],
        }
    }

    #[test]
    fn rising_market_is_bullish() {
        let mut classifier = classifier();
        classifier.classify(&snapshot(0, 1.0, 1.0), &trusted());
        let reading = classifier.classify(&snapshot(3_000, 1.05, 1.0), &trusted());
        assert_eq!(reading.regime, Regime::Bullish);
    }

    #[test]
    fn depeg_beats_everything_else() {
        let mut classifier = classifier();
        let untrusted = PriceAssessment {
            twap: 1.0,
            issues: vec![PriceIssue::ThinDepth],
        };
        let reading = classifier.classify(&snapshot(0, 1.0, 0.95), &untrusted);
        assert_eq!(
            reading.regime,
            Regime::Emergency(EmergencyCause::StableDepeg)
        );
    }

    #[test]
    fn one_step_crash_is_an_extreme_move() {
        let mut classifier = classifier();
        classifier.classify(&snapshot(0, 1.0, 1.0), &trusted());
        let reading = classifier.classify(&snapshot(300, 0.7, 1.0), &trusted());
        assert_eq!(
            reading.regime,
            Regime::Emergency(EmergencyCause::ExtremeMove)
        );
    }
}

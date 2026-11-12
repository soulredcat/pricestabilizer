//! Builds a [`PriceAssessment`] from each snapshot.

use crate::config::engine_sections::PriceConfig;
use crate::engine::price::confidence::{PriceAssessment, PriceIssue};
use crate::engine::price::twap::TwapWindow;
use crate::market::snapshot::MarketSnapshot;

pub struct PriceAssessor {
    config: PriceConfig,
    twap: TwapWindow,
    previous_timestamp: Option<u64>,
}

impl PriceAssessor {
    pub fn new(config: &PriceConfig) -> Self {
        Self {
            config: config.clone(),
            twap: TwapWindow::new(config.twap_window_secs),
            previous_timestamp: None,
        }
    }

    pub fn assess(&mut self, snapshot: &MarketSnapshot) -> PriceAssessment {
        let gapped = self.previous_timestamp.is_some_and(|previous| {
            snapshot.timestamp.saturating_sub(previous) > self.config.max_sample_gap_secs
        });
        self.previous_timestamp = Some(snapshot.timestamp);
        if gapped {
            // Stale history must not vouch for the price after an outage.
            self.twap = TwapWindow::new(self.config.twap_window_secs);
        }
        self.twap.push(
            snapshot.timestamp,
            snapshot.spot_price,
            snapshot.quote_volume,
        );
        let twap = self.twap.average().unwrap_or(snapshot.spot_price);

        let checks = [
            (
                !self.twap.is_warm(snapshot.timestamp),
                PriceIssue::WarmingUp,
            ),
            (gapped, PriceIssue::SampleGap),
            (
                snapshot.quote_depth < self.config.min_quote_depth,
                PriceIssue::ThinDepth,
            ),
            (
                self.twap.quote_volume() < self.config.min_window_quote_volume,
                PriceIssue::LowVolume,
            ),
            (
                (snapshot.spot_price - twap).abs() / twap > self.config.max_spot_twap_deviation,
                PriceIssue::SpotTwapDivergence,
            ),
        ];
        PriceAssessment {
            twap,
            issues: checks
                .into_iter()
                .filter_map(|(failed, issue)| failed.then_some(issue))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PriceAssessor;
    use crate::config::engine_sections::PriceConfig;
    use crate::engine::price::confidence::PriceIssue;
    use crate::market::snapshot::MarketSnapshot;

    fn assessor() -> PriceAssessor {
        PriceAssessor::new(&PriceConfig {
            twap_window_secs: 600,
            max_sample_gap_secs: 120,
            min_quote_depth: 100.0,
            min_window_quote_volume: 5.0,
            max_spot_twap_deviation: 0.05,
            depth_reference_move: 0.02,
        })
    }

    fn at(timestamp: u64, spot_price: f64) -> MarketSnapshot {
        MarketSnapshot {
            timestamp,
            spot_price,
            quote_depth: 1_000.0,
            quote_volume: 1.0,
            quote_usd_price: 1.0,
        }
    }

    fn fill(assessor: &mut PriceAssessor, until: u64) {
        for timestamp in (0..=until).step_by(60) {
            assessor.assess(&at(timestamp, 1.0));
        }
    }

    #[test]
    fn steady_market_is_trusted_after_warm_up() {
        let mut assessor = assessor();
        assert!(!assessor.assess(&at(0, 1.0)).is_trusted());
        fill(&mut assessor, 600);
        let verdict = assessor.assess(&at(660, 1.01));
        assert_eq!(verdict.reference_price(), Some(1.0));
    }

    #[test]
    fn spike_against_twap_is_not_trusted() {
        let mut assessor = assessor();
        fill(&mut assessor, 600);
        let verdict = assessor.assess(&at(660, 1.2));
        assert_eq!(verdict.issues, vec![PriceIssue::SpotTwapDivergence]);
    }

    #[test]
    fn outage_restarts_the_twap() {
        let mut assessor = assessor();
        fill(&mut assessor, 600);
        let verdict = assessor.assess(&at(1_000, 1.0));
        assert!(verdict.issues.contains(&PriceIssue::SampleGap));
        assert!(verdict.issues.contains(&PriceIssue::WarmingUp));
    }
}

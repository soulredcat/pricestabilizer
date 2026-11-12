//! Turns a scenario shape into a reproducible snapshot stream.

use crate::config::feed_section::ScenarioFeedConfig;
use crate::market::feed::scenario::kind::ScenarioKind;
use crate::market::feed::scenario::random::SeededRandom;
use crate::market::feed::scenario::shape::ScenarioShape;
use crate::market::feed::stream::MarketFeed;
use crate::market::snapshot::MarketSnapshot;

const SECONDS_PER_DAY: f64 = 86_400.0;
/// Share of the visible depth traded per step, on average.
const VOLUME_TURNOVER: f64 = 0.004;
/// Log-normal noise applied to depth and volume.
const DEPTH_NOISE: f64 = 0.05;
const VOLUME_NOISE: f64 = 0.5;

pub fn feed(kind: ScenarioKind, settings: &ScenarioFeedConfig) -> MarketFeed {
    MarketFeed {
        name: kind.name().to_owned(),
        snapshots: Box::new(ScenarioGenerator::new(kind, settings).map(Ok)),
        clock_window_secs: None,
    }
}

struct ScenarioGenerator {
    shape: ScenarioShape,
    random: SeededRandom,
    step: u64,
    total_steps: u64,
    interval_secs: u64,
    start_timestamp: u64,
    base_depth: f64,
    log_price: f64,
    drift_per_step: f64,
    volatility_per_step: f64,
}

impl ScenarioGenerator {
    fn new(kind: ScenarioKind, settings: &ScenarioFeedConfig) -> Self {
        let shape = ScenarioShape::of(kind);
        let step_days = settings.interval_secs as f64 / SECONDS_PER_DAY;
        Self {
            drift_per_step: shape.daily_drift * step_days,
            volatility_per_step: shape.daily_volatility * step_days.sqrt(),
            shape,
            // Each scenario gets its own stream even under a shared seed.
            random: SeededRandom::new(settings.seed.wrapping_add(kind as u64)),
            step: 0,
            total_steps: settings.steps,
            interval_secs: settings.interval_secs,
            start_timestamp: settings.start_timestamp,
            base_depth: settings.start_quote_depth,
            log_price: settings.start_price.ln(),
        }
    }

    fn advance_price(&mut self, step: u64) {
        let diffusion =
            self.drift_per_step + self.volatility_per_step * self.random.next_gaussian();
        self.log_price += diffusion + self.shape.shock_return(step, self.total_steps);
    }

    fn snapshot(&mut self, step: u64, progress: f64) -> MarketSnapshot {
        let depth_noise = (DEPTH_NOISE * self.random.next_gaussian()).exp();
        let quote_depth = self.base_depth * self.shape.depth_multiplier(progress) * depth_noise;
        let volume_noise = (VOLUME_NOISE * self.random.next_gaussian()).exp();
        MarketSnapshot {
            timestamp: self.start_timestamp + step * self.interval_secs,
            spot_price: self.log_price.exp(),
            quote_depth,
            quote_volume: quote_depth * VOLUME_TURNOVER * volume_noise,
            quote_usd_price: self.shape.quote_usd_price(progress),
        }
    }
}

impl Iterator for ScenarioGenerator {
    type Item = MarketSnapshot;

    fn next(&mut self) -> Option<MarketSnapshot> {
        while self.step < self.total_steps {
            let step = self.step;
            self.step += 1;
            if step > 0 {
                self.advance_price(step);
            }
            let progress = step as f64 / self.total_steps as f64;
            if !self.shape.is_silent(progress) {
                return Some(self.snapshot(step, progress));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::ScenarioGenerator;
    use crate::config::feed_section::ScenarioFeedConfig;
    use crate::market::feed::scenario::kind::ScenarioKind;

    fn settings() -> ScenarioFeedConfig {
        ScenarioFeedConfig {
            scenarios: vec![],
            seed: 3,
            steps: 200,
            interval_secs: 300,
            start_timestamp: 1_000,
            start_price: 2.0,
            start_quote_depth: 10_000.0,
        }
    }

    #[test]
    fn starts_at_the_configured_price_and_time() {
        let first = ScenarioGenerator::new(ScenarioKind::Sideways, &settings())
            .next()
            .expect("first snapshot");
        assert_eq!(first.timestamp, 1_000);
        assert!((first.spot_price - 2.0).abs() < 1e-12);
    }

    #[test]
    fn data_gap_skips_its_silent_window() {
        let count = ScenarioGenerator::new(ScenarioKind::DataGap, &settings()).count();
        assert_eq!(count, 190);
    }
}

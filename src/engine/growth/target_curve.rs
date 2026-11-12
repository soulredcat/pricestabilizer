//! `P_target(t) = P0 × (1 + g)^weeks`: a benchmark line only. Being below it
//! is never, by itself, a reason to buy.

use crate::config::engine_sections::GrowthConfig;

const SECONDS_PER_WEEK: f64 = 7.0 * 86_400.0;

pub struct TargetCurve {
    initial_price: f64,
    weekly_growth: f64,
    start_timestamp: u64,
}

impl TargetCurve {
    pub fn new(config: &GrowthConfig) -> Self {
        Self {
            initial_price: config.initial_price,
            weekly_growth: config.weekly_growth,
            start_timestamp: config.start_timestamp,
        }
    }

    /// Target at `timestamp`; flat at P0 before the line starts.
    pub fn price_at(&self, timestamp: u64) -> f64 {
        let elapsed_weeks =
            timestamp.saturating_sub(self.start_timestamp) as f64 / SECONDS_PER_WEEK;
        self.initial_price * (1.0 + self.weekly_growth).powf(elapsed_weeks)
    }
}

#[cfg(test)]
mod tests {
    use super::{SECONDS_PER_WEEK, TargetCurve};
    use crate::config::engine_sections::GrowthConfig;

    fn curve() -> TargetCurve {
        TargetCurve::new(&GrowthConfig {
            initial_price: 1.0,
            weekly_growth: 0.01,
            start_timestamp: 1_000,
        })
    }

    fn weeks(count: f64) -> u64 {
        1_000 + (count * SECONDS_PER_WEEK) as u64
    }

    #[test]
    fn compounds_weekly() {
        assert!((curve().price_at(weeks(1.0)) - 1.01).abs() < 1e-12);
        assert!((curve().price_at(weeks(52.0)) - 1.677_688_9).abs() < 1e-6);
    }

    #[test]
    fn is_flat_before_the_start() {
        assert!((curve().price_at(0) - 1.0).abs() < 1e-12);
    }
}

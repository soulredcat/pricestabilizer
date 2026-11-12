//! The statistical recipe behind each scenario, expressed over normalised
//! progress `0.0..1.0` so it works for any run length.

use crate::market::feed::scenario::kind::ScenarioKind;

/// A log-price move of `total`, spread evenly over `steps` steps from `at`.
pub struct Shock {
    pub at: f64,
    pub total: f64,
    pub steps: u64,
}

/// A level that switches to `value` from progress `at` onwards.
#[derive(Clone, Copy)]
pub struct Shift {
    pub at: f64,
    pub value: f64,
}

pub struct ScenarioShape {
    /// Expected log return per day.
    pub daily_drift: f64,
    /// Daily volatility of log returns.
    pub daily_volatility: f64,
    pub shocks: &'static [Shock],
    /// Constant multiplier on the starting depth.
    pub depth_factor: f64,
    pub depth_shift: Option<Shift>,
    pub quote_usd_shift: Option<Shift>,
    /// Progress interval during which the feed emits nothing.
    pub silent_window: Option<(f64, f64)>,
}

const CALM: ScenarioShape = ScenarioShape {
    daily_drift: 0.0,
    daily_volatility: 0.03,
    shocks: &[],
    depth_factor: 1.0,
    depth_shift: None,
    quote_usd_shift: None,
    silent_window: None,
};

impl ScenarioShape {
    pub fn of(kind: ScenarioKind) -> Self {
        match kind {
            ScenarioKind::SteadyBull => Self {
                daily_drift: 0.05,
                daily_volatility: 0.04,
                ..CALM
            },
            ScenarioKind::SteadyBear => Self {
                daily_drift: -0.05,
                daily_volatility: 0.04,
                ..CALM
            },
            ScenarioKind::Sideways => CALM,
            ScenarioKind::FlashCrash => Self {
                shocks: &[
                    Shock {
                        at: 0.5,
                        total: -0.35,
                        steps: 1,
                    },
                    Shock {
                        at: 0.52,
                        total: 0.12,
                        steps: 6,
                    },
                ],
                ..CALM
            },
            ScenarioKind::PumpRetrace => Self {
                shocks: &[
                    Shock {
                        at: 0.3,
                        total: 0.4,
                        steps: 24,
                    },
                    Shock {
                        at: 0.5,
                        total: -0.3,
                        steps: 72,
                    },
                ],
                ..CALM
            },
            ScenarioKind::LowLiquidity => Self {
                depth_factor: 0.05,
                ..CALM
            },
            ScenarioKind::LiquidityExit => Self {
                depth_shift: Some(Shift {
                    at: 0.5,
                    value: 0.25,
                }),
                ..CALM
            },
            ScenarioKind::LiquidityInflow => Self {
                depth_shift: Some(Shift {
                    at: 0.5,
                    value: 3.0,
                }),
                ..CALM
            },
            ScenarioKind::StableDepeg => Self {
                quote_usd_shift: Some(Shift {
                    at: 0.6,
                    value: 0.93,
                }),
                ..CALM
            },
            ScenarioKind::DataGap => Self {
                silent_window: Some((0.5, 0.55)),
                ..CALM
            },
        }
    }

    /// Log return added by shocks at `step` of a run of `total_steps`.
    pub fn shock_return(&self, step: u64, total_steps: u64) -> f64 {
        self.shocks
            .iter()
            .filter(|shock| {
                let start = (shock.at * total_steps as f64).round() as u64;
                (start..start + shock.steps).contains(&step)
            })
            .map(|shock| shock.total / shock.steps as f64)
            .sum()
    }

    pub fn depth_multiplier(&self, progress: f64) -> f64 {
        self.depth_factor * level(self.depth_shift, progress)
    }

    pub fn quote_usd_price(&self, progress: f64) -> f64 {
        level(self.quote_usd_shift, progress)
    }

    pub fn is_silent(&self, progress: f64) -> bool {
        self.silent_window
            .is_some_and(|(from, to)| (from..to).contains(&progress))
    }
}

/// 1.0 until the shift kicks in, then the shifted value.
fn level(shift: Option<Shift>, progress: f64) -> f64 {
    match shift {
        Some(shift) if progress >= shift.at => shift.value,
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::ScenarioShape;
    use crate::market::feed::scenario::kind::ScenarioKind;

    #[test]
    fn shocks_spread_over_their_steps() {
        let shape = ScenarioShape::of(ScenarioKind::PumpRetrace);
        let total: f64 = (0..1_000).map(|step| shape.shock_return(step, 1_000)).sum();
        assert!(
            (total - 0.1).abs() < 1e-9,
            "net pump minus retrace, got {total}"
        );
    }

    #[test]
    fn depeg_starts_at_its_progress_point() {
        let shape = ScenarioShape::of(ScenarioKind::StableDepeg);
        assert!((shape.quote_usd_price(0.59) - 1.0).abs() < 1e-12);
        assert!((shape.quote_usd_price(0.6) - 0.93).abs() < 1e-12);
    }
}

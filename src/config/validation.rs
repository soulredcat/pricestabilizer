//! Rejects configurations the engine must never run with.

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::config::feed_section::FeedConfig;

impl EngineConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        let growth = &self.growth;
        positive("growth.initial_price", growth.initial_price)?;
        non_negative("growth.weekly_growth", growth.weekly_growth)?;

        let price = &self.price;
        nonzero("price.twap_window_secs", price.twap_window_secs)?;
        nonzero("price.max_sample_gap_secs", price.max_sample_gap_secs)?;
        non_negative("price.min_quote_depth", price.min_quote_depth)?;
        non_negative(
            "price.min_window_quote_volume",
            price.min_window_quote_volume,
        )?;
        fraction(
            "price.max_spot_twap_deviation",
            price.max_spot_twap_deviation,
        )?;

        let regime = &self.regime;
        nonzero("regime.window_secs", regime.window_secs)?;
        positive("regime.trend_threshold", regime.trend_threshold)?;
        positive("regime.high_daily_volatility", regime.high_daily_volatility)?;
        positive("regime.extreme_move", regime.extreme_move)?;
        positive("regime.quote_peg_usd", regime.quote_peg_usd)?;
        fraction("regime.depeg_threshold", regime.depeg_threshold)?;
        fraction("regime.max_depth_drop", regime.max_depth_drop)?;

        let liquidity = &self.liquidity;
        positive("liquidity.range_sigmas", liquidity.range_sigmas)?;
        positive("liquidity.range_horizon_days", liquidity.range_horizon_days)?;
        fraction("liquidity.min_half_width", liquidity.min_half_width)?;
        fraction("liquidity.max_half_width", liquidity.max_half_width)?;
        check(
            liquidity.min_half_width <= liquidity.max_half_width,
            "liquidity.min_half_width",
            "must not exceed liquidity.max_half_width",
        )?;
        positive("liquidity.width_tolerance", liquidity.width_tolerance)?;

        validate_feed(&self.feed)
    }
}

fn validate_feed(feed: &FeedConfig) -> Result<(), AppError> {
    match feed {
        FeedConfig::Scenario(settings) => {
            check(
                !settings.scenarios.is_empty(),
                "feed.scenarios",
                "must list at least one scenario",
            )?;
            nonzero("feed.steps", settings.steps)?;
            nonzero("feed.interval_secs", settings.interval_secs)?;
            positive("feed.start_price", settings.start_price)?;
            positive("feed.start_quote_depth", settings.start_quote_depth)
        }
        FeedConfig::Jsonl(_) => Ok(()),
    }
}

fn check(holds: bool, field: &'static str, rule: &'static str) -> Result<(), AppError> {
    if holds {
        Ok(())
    } else {
        Err(AppError::ConfigInvalid { field, rule })
    }
}

fn positive(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value.is_finite() && value > 0.0,
        field,
        "must be a positive finite number",
    )
}

fn non_negative(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value.is_finite() && value >= 0.0,
        field,
        "must be a finite number >= 0",
    )
}

fn fraction(field: &'static str, value: f64) -> Result<(), AppError> {
    check(
        value > 0.0 && value < 1.0,
        field,
        "must be strictly between 0 and 1",
    )
}

fn nonzero(field: &'static str, value: u64) -> Result<(), AppError> {
    check(value > 0, field, "must be greater than 0")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::app::error::AppError;
    use crate::config::engine_config::EngineConfig;

    const EXAMPLE: &str = include_str!("../../config/simulation.example.toml");

    #[test]
    fn example_config_is_valid() {
        assert!(EngineConfig::parse(EXAMPLE, Path::new("example")).is_ok());
    }

    #[test]
    fn inverted_range_bounds_are_rejected() {
        let broken = EXAMPLE.replace("min_half_width = 0.02", "min_half_width = 0.9");
        let error = EngineConfig::parse(&broken, Path::new("broken")).unwrap_err();
        assert!(matches!(
            error,
            AppError::ConfigInvalid {
                field: "liquidity.min_half_width",
                ..
            }
        ));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let typo = EXAMPLE.replace("weekly_growth", "weekly_grow");
        let error = EngineConfig::parse(&typo, Path::new("typo")).unwrap_err();
        assert!(matches!(error, AppError::ConfigParse { .. }));
    }
}

//! Checks for the decision-engine sections.

use crate::app::error::AppError;
use crate::config::engine_sections::{GrowthConfig, LiquidityConfig, PriceConfig, RegimeConfig};
use crate::config::validation::rules::{check, fraction, non_negative, nonzero, positive};

pub fn check_growth(growth: &GrowthConfig) -> Result<(), AppError> {
    positive("growth.initial_price", growth.initial_price)?;
    non_negative("growth.weekly_growth", growth.weekly_growth)
}

pub fn check_price(price: &PriceConfig) -> Result<(), AppError> {
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
    fraction("price.depth_reference_move", price.depth_reference_move)?;
    nonzero("price.min_twap_samples", price.min_twap_samples as u64)
}

pub fn check_regime(regime: &RegimeConfig) -> Result<(), AppError> {
    nonzero("regime.window_secs", regime.window_secs)?;
    positive("regime.trend_threshold", regime.trend_threshold)?;
    positive("regime.high_daily_volatility", regime.high_daily_volatility)?;
    positive("regime.extreme_move", regime.extreme_move)?;
    positive("regime.quote_peg_usd", regime.quote_peg_usd)?;
    fraction("regime.depeg_threshold", regime.depeg_threshold)?;
    fraction("regime.max_depth_drop", regime.max_depth_drop)
}

pub fn check_liquidity(liquidity: &LiquidityConfig) -> Result<(), AppError> {
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
    fraction("liquidity.band_capital_share", liquidity.band_capital_share)?;
    fraction(
        "liquidity.rebalance_tolerance",
        liquidity.rebalance_tolerance,
    )
}

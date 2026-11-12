//! Thresholds for each engine stage. The example config ships starting values
//! only; live values must come from backtests, never from guesses.

use serde::Deserialize;

/// Long-run benchmark line. It measures performance and never orders a trade.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrowthConfig {
    /// Reference price P0, in quote units, at `start_timestamp`.
    pub initial_price: f64,
    /// Compounded growth per week (0.01 = 1%).
    pub weekly_growth: f64,
    /// Unix seconds (UTC) where the benchmark line starts.
    pub start_timestamp: u64,
}

/// When an observed price is trusted enough to act on.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceConfig {
    /// Length of the TWAP window.
    pub twap_window_secs: u64,
    /// A longer silence means the feed was interrupted and the TWAP restarts.
    pub max_sample_gap_secs: u64,
    /// Minimum quote liquidity near spot.
    pub min_quote_depth: f64,
    /// Minimum quote volume traded inside the TWAP window.
    pub min_window_quote_volume: f64,
    /// Maximum relative gap between spot and TWAP.
    pub max_spot_twap_deviation: f64,
    /// Feed convention: `quote_depth` is the quote liquidity that moves the
    /// price by this fraction (0.02 = depth within a 2% move).
    pub depth_reference_move: f64,
}

/// Market regime thresholds.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegimeConfig {
    /// Lookback for trend, volatility and peak depth.
    pub window_secs: u64,
    /// Absolute log change over the window that counts as a trend.
    pub trend_threshold: f64,
    /// Daily realised volatility above which the market is "high volatility".
    pub high_daily_volatility: f64,
    /// Absolute single-step log return that trips protection.
    pub extreme_move: f64,
    /// Expected USD value of one quote-asset unit (1.0 for a USD stablecoin).
    pub quote_peg_usd: f64,
    /// Relative deviation from the peg that trips protection.
    pub depeg_threshold: f64,
    /// Share of the window's peak depth that may vanish before protection trips.
    pub max_depth_drop: f64,
}

/// Concentrated-liquidity range sizing.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiquidityConfig {
    /// Range half-width, in standard deviations of the expected move.
    pub range_sigmas: f64,
    /// Horizon, in days, the range should cover.
    pub range_horizon_days: f64,
    /// Narrowest relative half-width (0.02 = ±2%).
    pub min_half_width: f64,
    /// Widest relative half-width.
    pub max_half_width: f64,
    /// Relative width change that justifies a reposition on its own.
    pub width_tolerance: f64,
    /// Minimum time between two range changes.
    pub min_reposition_interval_secs: u64,
    /// Share of operational wallet value placed in the band; it scales with
    /// the capital actually available.
    pub band_capital_share: f64,
    /// Inventory gaps smaller than this share of the band budget are not
    /// worth a rebalancing swap.
    pub rebalance_tolerance: f64,
}

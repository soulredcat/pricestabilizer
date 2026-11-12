//! Asset units and the hard risk limits every order must respect.

use serde::Deserialize;

/// Base-unit precision of each asset, as the venue counts them.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetsConfig {
    pub token_decimals: u32,
    pub quote_decimals: u32,
}

/// Limits the risk gate and the loss monitor enforce. Required before any
/// live use; the example values are placeholders for simulation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiskConfig {
    /// Largest single swap, in quote.
    pub max_swap_quote: f64,
    /// Largest estimated price impact a swap may have.
    pub max_slippage: f64,
    /// Quote that must always stay free in the operational wallet.
    pub min_quote_reserve: f64,
    /// Largest value of token held across wallets and band, in quote.
    pub max_token_exposure_quote: f64,
    /// Largest equity loss within one UTC day, in quote.
    pub max_daily_loss_quote: f64,
    /// Largest equity drop from its peak, as a fraction.
    pub max_drawdown: f64,
    /// Failed venue orders in a row that trip protection.
    pub max_consecutive_failures: u32,
}

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
    /// A venue must refuse an order it has not executed within this time.
    pub order_ttl_secs: u64,
    /// A venue must refuse an order once its live price moved more than
    /// this, relative, from the price the order was planned at.
    pub max_price_drift: f64,
    /// A swap must return at least `1 - this` of its value at spot (fees and
    /// impact together); it is the swap's minimum out.
    pub max_swap_shortfall: f64,
    /// The most one order may pay in network costs, in quote.
    pub max_action_cost_quote: f64,
}

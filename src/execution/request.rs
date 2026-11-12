//! What the session asks the executor to do for one snapshot.

use crate::engine::liquidity::action::LiquidityAction;

#[derive(Debug, Clone, Copy)]
pub struct ExecutionRequest {
    pub action: LiquidityAction,
    /// Share of operational value a new band may use.
    pub capital_share: f64,
    /// An approved treasury purchase, in quote.
    pub treasury_buy_quote: Option<f64>,
    /// Price used to value the books: the TWAP, which one manipulated print
    /// cannot move.
    pub valuation_price: f64,
}

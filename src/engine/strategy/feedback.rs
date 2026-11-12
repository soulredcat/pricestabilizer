//! What happened downstream of the engine on the previous snapshot.

use crate::engine::regime::market_regime::EmergencyCause;

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionFeedback {
    /// A problem with the books or the venue that must trip protection.
    pub alert: Option<EmergencyCause>,
    /// Whether a band position is actually live at the venue.
    pub band_held: bool,
    /// The token's share of operational inventory value (wallet and band).
    pub token_share: Option<f64>,
}

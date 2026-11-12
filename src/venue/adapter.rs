//! The one interface between the engine and a market venue. Implementations
//! hide everything chain-specific: signing, gas, ticks, order books.

use serde_json::Value;

use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::holding::Holding;
use crate::venue::order::{Fill, OrderGuard, PositionView, VenueError, VenueOrder, Wallet};

pub trait Venue {
    /// Catches up with the market before any order (fee accrual, chain sync).
    fn observe(&mut self, market: &MarketSnapshot) -> Result<(), VenueError>;

    /// Executes `order` exactly, or refuses it when `guard` cannot be honoured.
    fn execute(
        &mut self,
        order: &VenueOrder,
        guard: &OrderGuard,
        market: &MarketSnapshot,
    ) -> Result<Fill, VenueError>;

    /// Wallet balances as the venue (or chain) reports them.
    fn balances(&mut self, wallet: Wallet) -> Result<Holding, VenueError>;

    fn position(&mut self, market: &MarketSnapshot) -> Result<Option<PositionView>, VenueError>;

    /// What the venue itself must carry across a restart. Venues whose state
    /// lives on a chain keep nothing here.
    fn saved_state(&self) -> Option<Value> {
        None
    }

    fn restore_state(&mut self, _state: Value) -> Result<(), VenueError> {
        Ok(())
    }
}

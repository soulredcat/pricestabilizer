//! What the executor reports back for one snapshot.

use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::execution::risk_gate::RiskBreach;
use crate::portfolio::pnl_tracker::BooksSnapshot;
use crate::venue::order::VenueError;

#[derive(Debug, Clone, Copy)]
pub enum OrderResult {
    Filled(&'static str),
    Blocked(RiskBreach),
    Failed(VenueError),
}

impl OrderResult {
    pub fn label(self) -> &'static str {
        match self {
            Self::Filled(order) => order,
            Self::Blocked(breach) => breach.label(),
            Self::Failed(error) => error.label(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ExecutionOutcome {
    pub orders: Vec<OrderResult>,
    /// `None` when the venue could not be read this snapshot.
    pub books: Option<BooksSnapshot>,
    /// Fed into the engine on the next snapshot.
    pub feedback: ExecutionFeedback,
    /// Quote an approved treasury purchase actually spent, when it filled.
    pub treasury_spent: Option<f64>,
}

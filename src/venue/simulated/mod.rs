//! Paper venue: fills orders against each snapshot and keeps its own
//! wallets, standing in for a chain during backtests and shadow runs.

pub mod guard_check;
pub mod paper_orders;
pub mod paper_venue;
pub mod position;

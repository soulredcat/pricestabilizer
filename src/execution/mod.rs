//! Execution: turns liquidity intents into venue orders behind a risk gate,
//! books every fill and reconciles the books with the venue.

pub mod band_funding;
pub mod band_mover;
pub mod booking;
pub mod executor;
pub mod fill_check;
pub mod guard_rules;
pub mod inspection;
pub mod order_runner;
pub mod outcome;
pub mod request;
pub mod risk_gate;

#[cfg(test)]
mod executor_tests;

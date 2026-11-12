//! Execution: turns liquidity intents into venue orders behind a risk gate,
//! books every fill and reconciles the books with the venue.

pub mod band_funding;
pub mod booking;
pub mod executor;
pub mod outcome;
pub mod risk_gate;

#[cfg(test)]
mod executor_tests;

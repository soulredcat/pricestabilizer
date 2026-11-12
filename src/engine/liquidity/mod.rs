//! Liquidity engine: where to provide liquidity, expressed as a venue-neutral
//! price band. A venue adapter later maps a band to a concentrated-liquidity
//! position or to a ladder of resting orders.

pub mod action;
pub mod band_keeper;
pub mod price_band;

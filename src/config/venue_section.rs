//! Which venue executes orders.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VenueConfig {
    /// Paper venue: fills against each snapshot, holds simulated wallets.
    Simulated(SimulatedVenueConfig),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedVenueConfig {
    /// Fee earned by liquidity providers on traded volume.
    pub fee_rate: f64,
    /// Fee paid on our own swaps.
    pub swap_fee_rate: f64,
    /// Network cost of one order, in quote.
    pub action_cost_quote: f64,
    /// Starting wallets, in human amounts.
    pub operational_token: f64,
    pub operational_quote: f64,
    pub treasury_token: f64,
    pub treasury_quote: f64,
}

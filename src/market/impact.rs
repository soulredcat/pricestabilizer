//! Price-impact estimate shared by the risk gate and the simulated venue.
//!
//! Feed convention: `quote_depth` is the quote liquidity that moves the price
//! by `depth_reference_move`. Trading that much moves price by the full
//! reference move, so its average fill lands about half-way.

pub fn estimated_impact(notional_quote: f64, quote_depth: f64, depth_reference_move: f64) -> f64 {
    if quote_depth > 0.0 {
        0.5 * depth_reference_move * notional_quote / quote_depth
    } else {
        f64::INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::estimated_impact;

    #[test]
    fn trading_the_whole_depth_costs_half_the_reference_move() {
        assert!((estimated_impact(1_000.0, 1_000.0, 0.02) - 0.01).abs() < 1e-12);
        assert!(estimated_impact(1.0, 0.0, 0.02).is_infinite());
    }
}

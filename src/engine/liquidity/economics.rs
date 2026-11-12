//! Whether providing liquidity pays: fees earned against what arbitrage
//! takes from the band (LVR, loss-versus-rebalancing).
//!
//! While the price is inside a band of liquidity `L`, arbitrage takes
//! `σ² · L · √p / 4` per day (σ the daily volatility), and the band earns
//! about `fee · volume · (its depth / the pool's depth)`. Its depth within the
//! feed's reference move `m` is `L · √p · (1 − 1/√(1+m))`, so while the band
//! is small next to the pool the ratio of the two does not depend on the
//! band's width or size at all:
//!
//! ```text
//! fees / LVR = 4 · fee · volume · (1 − 1/√(1+m)) / (depth · σ²)
//! ```
//!
//! Below 1, arbitrageurs take more from the band than traders pay into it.

pub fn fee_to_lvr(
    pool_fee_rate: f64,
    daily_volume: f64,
    quote_depth: f64,
    depth_reference_move: f64,
    daily_volatility: f64,
) -> Option<f64> {
    let variance = daily_volatility * daily_volatility;
    if quote_depth <= 0.0 || variance <= 0.0 {
        return None;
    }
    let depth_per_liquidity = 1.0 - 1.0 / (1.0 + depth_reference_move).sqrt();
    Some(4.0 * pool_fee_rate * daily_volume * depth_per_liquidity / (quote_depth * variance))
}

#[cfg(test)]
mod tests {
    use super::fee_to_lvr;

    #[test]
    fn more_volume_pays_and_more_volatility_costs() {
        let base = fee_to_lvr(0.003, 50_000.0, 50_000.0, 0.02, 0.05).expect("defined");
        // 0.3% fee, volume equal to depth, 5% daily volatility.
        assert!((base - 4.0 * 0.003 * (1.0 - 1.0 / 1.02_f64.sqrt()) / 0.0025).abs() < 1e-12);
        let busier = fee_to_lvr(0.003, 100_000.0, 50_000.0, 0.02, 0.05).expect("defined");
        let wilder = fee_to_lvr(0.003, 50_000.0, 50_000.0, 0.02, 0.10).expect("defined");
        assert!((busier / base - 2.0).abs() < 1e-12);
        assert!((wilder / base - 0.25).abs() < 1e-12);
    }

    #[test]
    fn undefined_without_depth_or_volatility() {
        assert_eq!(fee_to_lvr(0.003, 1.0, 0.0, 0.02, 0.05), None);
        assert_eq!(fee_to_lvr(0.003, 1.0, 1.0, 0.02, 0.0), None);
    }
}

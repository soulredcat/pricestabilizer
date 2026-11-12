//! The liquidity curve over a price band (the concentrated-liquidity model).
//! On an order book the same curve describes an evenly spread ladder of
//! resting orders, so this math stays venue-neutral.

use crate::engine::liquidity::price_band::PriceBand;

/// Token and quote held by one unit of liquidity over `band` at `price`.
/// Below the band it is all token; above it, all quote.
pub fn amounts_per_liquidity(band: PriceBand, price: f64) -> (f64, f64) {
    let sqrt_lower = band.lower.sqrt();
    let sqrt_upper = band.upper.sqrt();
    let sqrt_price = price.sqrt().clamp(sqrt_lower, sqrt_upper);
    (1.0 / sqrt_price - 1.0 / sqrt_upper, sqrt_price - sqrt_lower)
}

/// Quote value of one unit of liquidity at `price`.
pub fn value_per_liquidity(band: PriceBand, price: f64) -> f64 {
    let (token, quote) = amounts_per_liquidity(band, price);
    token * price + quote
}

/// Largest liquidity that `token` and `quote` can fund together.
pub fn liquidity_for(band: PriceBand, price: f64, token: f64, quote: f64) -> f64 {
    let (token_per, quote_per) = amounts_per_liquidity(band, price);
    let by_token = if token_per > 0.0 {
        token / token_per
    } else {
        f64::INFINITY
    };
    let by_quote = if quote_per > 0.0 {
        quote / quote_per
    } else {
        f64::INFINITY
    };
    by_token.min(by_quote)
}

/// Quote absorbed by `liquidity` while price falls by `relative_move`.
pub fn quote_depth(band: PriceBand, liquidity: f64, price: f64, relative_move: f64) -> f64 {
    let top = price.sqrt().min(band.upper.sqrt());
    let bottom = (price / (1.0 + relative_move))
        .sqrt()
        .max(band.lower.sqrt());
    liquidity * (top - bottom).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::{amounts_per_liquidity, liquidity_for, quote_depth, value_per_liquidity};
    use crate::engine::liquidity::price_band::PriceBand;

    fn band() -> PriceBand {
        PriceBand {
            lower: 0.81,
            upper: 1.21,
        }
    }

    #[test]
    fn centred_price_holds_both_assets() {
        let (token, quote) = amounts_per_liquidity(band(), 1.0);
        assert!((token - (1.0 - 1.0 / 1.1)).abs() < 1e-12);
        assert!((quote - 0.1).abs() < 1e-12);
        assert!((value_per_liquidity(band(), 1.0) - (token + quote)).abs() < 1e-12);
    }

    #[test]
    fn outside_the_band_holds_one_asset() {
        assert_eq!(amounts_per_liquidity(band(), 0.5).1, 0.0);
        assert_eq!(amounts_per_liquidity(band(), 2.0).0, 0.0);
    }

    #[test]
    fn exact_amounts_fund_their_liquidity() {
        let (token, quote) = amounts_per_liquidity(band(), 1.0);
        let liquidity = liquidity_for(band(), 1.0, token * 7.0, quote * 9.0);
        assert!((liquidity - 7.0).abs() < 1e-9);
    }

    #[test]
    fn depth_is_zero_above_and_positive_inside() {
        assert!(quote_depth(band(), 1.0, 1.0, 0.02) > 0.0);
        assert_eq!(quote_depth(band(), 1.0, 0.5, 0.02), 0.0);
    }
}

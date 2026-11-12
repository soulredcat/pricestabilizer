//! The one market shape the engine understands.

use serde::Deserialize;

/// One observation of a TOKEN/QUOTE market on any venue: an AMM pool, a
/// concentrated-liquidity pool or an order book. Connectors produce these; the
/// engine never sees addresses, ticks, decimals or SDK types. Unknown extra
/// fields are ignored so connectors may attach their own metadata.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MarketSnapshot {
    /// Unix seconds (UTC).
    pub timestamp: u64,
    /// Token price in quote units (pool price or order-book mid).
    pub spot_price: f64,
    /// Quote-asset liquidity available near the spot price.
    pub quote_depth: f64,
    /// Quote-asset volume traded since the previous snapshot.
    pub quote_volume: f64,
    /// USD value of one quote-asset unit.
    pub quote_usd_price: f64,
}

impl MarketSnapshot {
    /// Rejects values the engine must never reason about.
    pub fn validate(&self, previous_timestamp: Option<u64>) -> Result<(), String> {
        if previous_timestamp.is_some_and(|previous| self.timestamp <= previous) {
            return Err(format!(
                "timestamp {} is not after the previous snapshot",
                self.timestamp
            ));
        }
        positive("spot_price", self.spot_price)?;
        positive("quote_usd_price", self.quote_usd_price)?;
        non_negative("quote_depth", self.quote_depth)?;
        non_negative("quote_volume", self.quote_volume)
    }
}

fn positive(field: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be positive and finite, got {value}"))
    }
}

fn non_negative(field: &str, value: f64) -> Result<(), String> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(format!("{field} must be finite and >= 0, got {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::MarketSnapshot;

    fn snapshot(timestamp: u64, spot_price: f64) -> MarketSnapshot {
        MarketSnapshot {
            timestamp,
            spot_price,
            quote_depth: 1_000.0,
            quote_volume: 10.0,
            quote_usd_price: 1.0,
        }
    }

    #[test]
    fn accepts_a_sane_snapshot() {
        assert!(snapshot(10, 1.5).validate(Some(9)).is_ok());
    }

    #[test]
    fn rejects_time_going_backwards() {
        assert!(snapshot(10, 1.5).validate(Some(10)).is_err());
    }

    #[test]
    fn rejects_non_positive_price() {
        assert!(snapshot(10, 0.0).validate(None).is_err());
        assert!(snapshot(10, f64::NAN).validate(None).is_err());
    }
}

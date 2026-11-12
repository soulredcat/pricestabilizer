//! The one market shape the engine understands.

use std::fmt;

use serde::{Deserialize, Serialize};

/// One observation of a TOKEN/QUOTE market on any venue: an AMM pool, a
/// concentrated-liquidity pool or an order book. Connectors produce these; the
/// engine never sees addresses, ticks, decimals or SDK types. Unknown extra
/// fields are ignored so connectors may attach their own metadata.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

/// Why a snapshot was refused. Refused snapshots are skipped, never acted on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotDefect {
    /// Not strictly after the previous accepted snapshot.
    OutOfOrder,
    /// This field is negative, not finite, or zero where it must be positive.
    InvalidValue(&'static str),
    /// Too far from the wall clock for a live feed: stale data, or a unit
    /// mix-up such as milliseconds instead of seconds.
    OutsideClockWindow,
}

impl SnapshotDefect {
    pub fn label(self) -> &'static str {
        match self {
            Self::OutOfOrder => "out_of_order",
            Self::InvalidValue(_) => "invalid_value",
            Self::OutsideClockWindow => "outside_clock_window",
        }
    }
}

impl fmt::Display for SnapshotDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfOrder => write!(f, "timestamp is not after the previous snapshot"),
            Self::InvalidValue(field) => write!(f, "{field} is out of range"),
            Self::OutsideClockWindow => write!(f, "timestamp is too far from the wall clock"),
        }
    }
}

impl MarketSnapshot {
    /// Rejects values the engine must never reason about.
    pub fn validate(&self, previous_timestamp: Option<u64>) -> Result<(), SnapshotDefect> {
        if previous_timestamp.is_some_and(|previous| self.timestamp <= previous) {
            return Err(SnapshotDefect::OutOfOrder);
        }
        let positive = |value: f64| value.is_finite() && value > 0.0;
        let non_negative = |value: f64| value.is_finite() && value >= 0.0;
        [
            ("spot_price", positive(self.spot_price)),
            ("quote_usd_price", positive(self.quote_usd_price)),
            ("quote_depth", non_negative(self.quote_depth)),
            ("quote_volume", non_negative(self.quote_volume)),
        ]
        .into_iter()
        .find(|(_, valid)| !valid)
        .map_or(Ok(()), |(field, _)| {
            Err(SnapshotDefect::InvalidValue(field))
        })
    }

    /// Live feeds only: the snapshot must lie within `max_age_secs` of `now`,
    /// in either direction.
    pub fn check_clock(&self, now: u64, max_age_secs: u64) -> Result<(), SnapshotDefect> {
        if now.abs_diff(self.timestamp) <= max_age_secs {
            Ok(())
        } else {
            Err(SnapshotDefect::OutsideClockWindow)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MarketSnapshot, SnapshotDefect};

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
        assert_eq!(
            snapshot(10, 1.5).validate(Some(10)),
            Err(SnapshotDefect::OutOfOrder)
        );
    }

    #[test]
    fn rejects_non_positive_price() {
        let refused = Err(SnapshotDefect::InvalidValue("spot_price"));
        assert_eq!(snapshot(10, 0.0).validate(None), refused);
        assert_eq!(snapshot(10, f64::NAN).validate(None), refused);
    }

    #[test]
    fn live_snapshots_must_sit_near_the_wall_clock() {
        assert!(snapshot(1_000, 1.0).check_clock(1_100, 120).is_ok());
        assert!(snapshot(1_000, 1.0).check_clock(1_200, 120).is_err());
        // Milliseconds sent as seconds land far in the future.
        assert!(snapshot(1_000_000, 1.0).check_clock(1_000, 120).is_err());
    }
}

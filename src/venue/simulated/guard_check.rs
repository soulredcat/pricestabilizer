//! The paper venue honours order guards exactly as a live connector must.

use crate::market::snapshot::MarketSnapshot;
use crate::venue::order::{OrderGuard, VenueError};

/// Refuses an order whose guard cannot be honoured at this snapshot.
pub fn check_guard(
    guard: &OrderGuard,
    market: &MarketSnapshot,
    action_cost: u128,
) -> Result<(), VenueError> {
    if market.timestamp > guard.deadline {
        return Err(VenueError::Expired);
    }
    let drift = (market.spot_price / guard.reference_price - 1.0).abs();
    if drift.is_nan() || drift > guard.max_price_drift {
        return Err(VenueError::PriceMoved);
    }
    if action_cost > guard.max_action_cost {
        return Err(VenueError::CostTooHigh);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_guard;
    use crate::market::snapshot::MarketSnapshot;
    use crate::venue::order::{OrderGuard, VenueError};

    fn guard() -> OrderGuard {
        OrderGuard {
            order_id: 1,
            deadline: 100,
            reference_price: 2.0,
            max_price_drift: 0.01,
            max_action_cost: 50,
        }
    }

    fn market(timestamp: u64, spot_price: f64) -> MarketSnapshot {
        MarketSnapshot {
            timestamp,
            spot_price,
            quote_depth: 1_000.0,
            quote_volume: 1.0,
            quote_usd_price: 1.0,
        }
    }

    #[test]
    fn refuses_late_moved_or_costly_orders() {
        assert_eq!(check_guard(&guard(), &market(100, 2.019), 50), Ok(()));
        assert_eq!(
            check_guard(&guard(), &market(101, 2.0), 0),
            Err(VenueError::Expired)
        );
        assert_eq!(
            check_guard(&guard(), &market(90, 2.05), 0),
            Err(VenueError::PriceMoved)
        );
        assert_eq!(
            check_guard(&guard(), &market(90, 2.0), 51),
            Err(VenueError::CostTooHigh)
        );
    }
}

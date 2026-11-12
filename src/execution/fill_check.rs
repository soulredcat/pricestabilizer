//! A fill must match the order it answers. A venue reporting anything else
//! is misbehaving, and the engine stops trusting it until an operator looks.

use crate::portfolio::asset::Asset;
use crate::portfolio::holding::Holding;
use crate::venue::order::{Fill, OrderGuard, VenueOrder};

pub fn consistent(order: &VenueOrder, guard: &OrderGuard, fill: &Fill) -> bool {
    // The order cost is part of the quote spent and within the guard.
    let Some(principal_quote) = fill.spent.quote.checked_sub(fill.action_cost) else {
        return false;
    };
    if fill.action_cost > guard.max_action_cost {
        return false;
    }
    match *order {
        VenueOrder::Swap {
            sell,
            units,
            min_received,
            ..
        } => {
            let (sold, other_spent) = match sell {
                Asset::Token => (fill.spent.token, principal_quote),
                Asset::Quote => (principal_quote, fill.spent.token),
            };
            sold <= units
                && other_spent == 0
                && fill.received.get(sell) == 0
                && fill.received.get(sell.counterpart()) >= min_received
        }
        VenueOrder::AddBand {
            token_units,
            quote_units,
            ..
        } => {
            fill.spent.token <= token_units
                && principal_quote <= quote_units
                && fill.received == Holding::default()
        }
        VenueOrder::RemoveBand => {
            fill.spent.token == 0
                && principal_quote == 0
                && fill.received.quote >= fill.fees_claimed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::consistent;
    use crate::engine::liquidity::price_band::PriceBand;
    use crate::portfolio::asset::Asset;
    use crate::portfolio::holding::Holding;
    use crate::venue::order::{Fill, OrderGuard, VenueOrder, Wallet};

    const GUARD: OrderGuard = OrderGuard {
        order_id: 1,
        deadline: 10,
        reference_price: 1.0,
        max_price_drift: 0.01,
        max_action_cost: 5,
    };

    fn sell_token() -> VenueOrder {
        VenueOrder::Swap {
            wallet: Wallet::Operational,
            sell: Asset::Token,
            units: 100,
            min_received: 95,
        }
    }

    fn swap_fill(received_quote: u128) -> Fill {
        Fill {
            received: Holding::of(Asset::Quote, received_quote),
            spent: Holding {
                token: 100,
                quote: 2,
            },
            action_cost: 2,
            ..Fill::default()
        }
    }

    #[test]
    fn accepts_a_fill_that_matches_its_order() {
        assert!(consistent(&sell_token(), &GUARD, &swap_fill(97)));
    }

    #[test]
    fn flags_under_delivery_and_overspending() {
        assert!(!consistent(&sell_token(), &GUARD, &swap_fill(90)));
        let add = VenueOrder::AddBand {
            band: PriceBand::around(1.0, 0.1),
            token_units: 10,
            quote_units: 10,
        };
        let overspent = Fill {
            spent: Holding {
                token: 11,
                quote: 1,
            },
            action_cost: 1,
            ..Fill::default()
        };
        assert!(!consistent(&add, &GUARD, &overspent));
        let hidden_cost = Fill {
            spent: Holding { token: 0, quote: 1 },
            action_cost: 3,
            ..Fill::default()
        };
        assert!(!consistent(&VenueOrder::RemoveBand, &GUARD, &hidden_cost));
    }
}

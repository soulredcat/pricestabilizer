//! Executor behaviour against the paper venue and a venue that always fails.

use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::price_band::PriceBand;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::execution::executor::Executor;
use crate::execution::test_support::{BrokenVenue, config, market, request};
use crate::venue::venue_factory::open_venue;

#[test]
fn opening_and_moving_a_band_keeps_the_books_reconciled() {
    let config = config();
    let mut executor =
        Executor::open(&config, open_venue(&config).expect("venue"), None).expect("opens");
    let open = executor.on_decision(
        &market(1, 1.0),
        &request(
            LiquidityAction::Open(PriceBand::around(1.0, 0.05)),
            None,
            1.0,
        ),
    );
    assert!(open.feedback.band_held);
    assert_eq!(open.feedback.alert, None);

    let moved = executor.on_decision(
        &market(2, 1.08),
        &request(
            LiquidityAction::Reposition(PriceBand::around(1.08, 0.05)),
            None,
            1.08,
        ),
    );
    let labels: Vec<_> = moved.orders.iter().map(|order| order.label()).collect();
    assert_eq!(labels.first(), Some(&"remove_band"));
    assert_eq!(labels.last(), Some(&"add_band"));
    assert_eq!(moved.feedback.alert, None, "ledger and venue agree");
}

#[test]
fn holding_places_no_orders() {
    let config = config();
    let mut executor =
        Executor::open(&config, open_venue(&config).expect("venue"), None).expect("opens");
    let outcome = executor.on_decision(
        &market(1, 1.0),
        &request(LiquidityAction::Hold(HoldReason::InBand), None, 1.0),
    );
    assert!(outcome.orders.is_empty());
    assert!(outcome.books.is_some());
}

#[test]
fn approved_treasury_buy_spends_treasury_quote_only() {
    let config = config();
    let mut executor =
        Executor::open(&config, open_venue(&config).expect("venue"), None).expect("opens");
    let outcome = executor.on_decision(
        &market(1, 1.0),
        &request(LiquidityAction::Hold(HoldReason::InBand), Some(250.0), 1.0),
    );
    assert_eq!(outcome.treasury_spent, Some(250.0));
    assert_eq!(
        outcome.feedback.alert, None,
        "treasury booked and reconciled"
    );
    let treasury = executor
        .ledger()
        .holding(crate::portfolio::ledger::Account::Treasury);
    // 250 spent plus the 0.25 order cost, both from the treasury wallet.
    assert_eq!(treasury.quote, 749_750_000);
    assert!(treasury.token > 0);
}

#[test]
fn repeated_venue_failures_raise_an_alert() {
    let config = config();
    let mut executor = Executor::open(&config, Box::new(BrokenVenue), None).expect("opens");
    let open = LiquidityAction::Open(PriceBand::around(1.0, 0.05));
    let alerts: Vec<_> = (0..3)
        .map(|tick| {
            executor
                .on_decision(&market(tick, 1.0), &request(open, None, 1.0))
                .feedback
                .alert
        })
        .collect();
    assert_eq!(alerts[0], None);
    assert_eq!(alerts[2], Some(EmergencyCause::VenueFailures));
}

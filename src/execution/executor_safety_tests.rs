//! Executor safety: wallet-bounded units, the kill switch, fill anomalies
//! and the daily order limit.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::config::engine_config::EngineConfig;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::price_band::PriceBand;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::execution::executor::Executor;
use crate::execution::test_support::{
    EXAMPLE, RecordingVenue, config, dry_run_config, market, request, token_short_wallet,
};
use crate::portfolio::holding::Holding;
use crate::venue::order::VenueOrder;
use crate::venue::venue_factory::open_venue;

#[test]
fn orders_never_ask_for_more_units_than_the_wallet_holds() {
    // Seen on recorded data: an 18-decimal wallet built by earlier fills does
    // not survive an f64 round trip, and a token-limited deposit asked for
    // about a million units more than the wallet held.
    let text = EXAMPLE.replace("token_decimals = 9", "token_decimals = 18");
    let config = EngineConfig::parse(&text, Path::new("eighteen")).expect("valid config");
    let wallet = Holding {
        token: 13_184_104_334_791_636_156_416,
        quote: 9_550_501_143,
    };
    let orders = Rc::new(RefCell::new(Vec::new()));
    let venue = RecordingVenue {
        wallet,
        orders: Rc::clone(&orders),
    };
    let mut executor = Executor::open(&config, Box::new(venue), None).expect("opens");
    let band = PriceBand {
        lower: 0.037_735_865_780_957_32,
        upper: 0.056_065_437_887_358_93,
    };
    executor.on_decision(
        &market(1, 0.045_775_623_848_488_815),
        &request(LiquidityAction::Open(band), None, 0.045_775_623_848_488_815),
    );
    let recorded = orders.borrow();
    assert!(
        recorded
            .iter()
            .any(|order| matches!(order, VenueOrder::AddBand { .. }))
    );
    for order in recorded.iter() {
        let within = match *order {
            VenueOrder::Swap { sell, units, .. } => units <= wallet.get(sell),
            VenueOrder::AddBand {
                token_units,
                quote_units,
                ..
            } => token_units <= wallet.token && quote_units <= wallet.quote,
            VenueOrder::RemoveBand => true,
        };
        assert!(within, "{order:?} exceeds {wallet:?}");
    }
}

#[test]
fn kill_switch_withholds_every_order() {
    let orders = Rc::new(RefCell::new(Vec::new()));
    let venue = RecordingVenue {
        wallet: token_short_wallet(),
        orders: Rc::clone(&orders),
    };
    let mut executor = Executor::open(&dry_run_config(), Box::new(venue), None).expect("opens");
    let open = LiquidityAction::Open(PriceBand::around(1.0, 0.05));
    let outcome = executor.on_decision(&market(1, 1.0), &request(open, Some(100.0), 1.0));
    let labels: Vec<_> = outcome.orders.iter().map(|order| order.label()).collect();
    assert_eq!(labels, vec!["withheld_swap", "withheld_swap"]);
    assert!(orders.borrow().is_empty(), "the venue never saw an order");
    assert_eq!(outcome.treasury_spent, None);
}

#[test]
fn a_fill_that_does_not_match_its_order_trips_protection() {
    // The recording venue answers every order with an empty fill, so a swap
    // "returns" nothing although it promised a minimum out.
    let venue = RecordingVenue {
        wallet: token_short_wallet(),
        orders: Rc::new(RefCell::new(Vec::new())),
    };
    let mut executor = Executor::open(&config(), Box::new(venue), None).expect("opens");
    let open = LiquidityAction::Open(PriceBand::around(1.0, 0.05));
    let outcome = executor.on_decision(&market(1, 1.0), &request(open, None, 1.0));
    assert_eq!(
        outcome.feedback.alert,
        Some(EmergencyCause::ExecutionAnomaly)
    );
    executor.acknowledge_losses();
    let calm = LiquidityAction::Hold(HoldReason::InBand);
    let after = executor.on_decision(&market(2, 1.0), &request(calm, None, 1.0));
    assert_eq!(after.feedback.alert, None, "an operator resume clears it");
}

#[test]
fn a_burst_of_orders_trips_protection_but_withdrawals_still_go_through() {
    let text = EXAMPLE.replace("max_orders_per_day = 48", "max_orders_per_day = 2");
    let config = EngineConfig::parse(&text, Path::new("burst")).expect("valid config");
    let venue = open_venue(&config).expect("venue");
    let mut executor = Executor::open(&config, venue, None).expect("opens");
    let open = LiquidityAction::Open(PriceBand::around(1.0, 0.05));
    executor.on_decision(&market(1, 1.0), &request(open, None, 1.0));
    let moved = LiquidityAction::Reposition(PriceBand::around(1.08, 0.05));
    let outcome = executor.on_decision(&market(2, 1.08), &request(moved, None, 1.08));
    let labels: Vec<_> = outcome.orders.iter().map(|order| order.label()).collect();
    assert_eq!(labels, vec!["remove_band", "blocked_daily_order_limit"]);
    assert_eq!(outcome.feedback.alert, Some(EmergencyCause::OrderBurst));
}

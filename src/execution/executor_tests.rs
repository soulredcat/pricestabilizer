//! Executor behaviour against a paper venue and a venue that always fails.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::config::engine_config::EngineConfig;
use crate::engine::liquidity::action::{HoldReason, LiquidityAction};
use crate::engine::liquidity::price_band::PriceBand;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::execution::executor::Executor;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::order::{Fill, PositionView, VenueError, VenueOrder, Wallet};
use crate::venue::venue_factory::open_venue;

const EXAMPLE: &str = include_str!("../../config/simulation.example.toml");

fn config() -> EngineConfig {
    EngineConfig::parse(EXAMPLE, Path::new("example")).expect("valid example")
}

fn market(timestamp: u64, spot_price: f64) -> MarketSnapshot {
    MarketSnapshot {
        timestamp,
        spot_price,
        quote_depth: 50_000.0,
        quote_volume: 400.0,
        quote_usd_price: 1.0,
    }
}

/// A venue whose wallets exist but whose orders never go through.
struct BrokenVenue;

impl Venue for BrokenVenue {
    fn observe(&mut self, _: &MarketSnapshot) -> Result<(), VenueError> {
        Ok(())
    }

    fn execute(&mut self, _: &VenueOrder, _: &MarketSnapshot) -> Result<Fill, VenueError> {
        Err(VenueError::NoLiquidity)
    }

    fn balances(&mut self, _: Wallet) -> Result<Holding, VenueError> {
        Ok(Holding {
            token: 1_000_000_000_000,
            quote: 1_000_000_000,
        })
    }

    fn position(&mut self, _: &MarketSnapshot) -> Result<Option<PositionView>, VenueError> {
        Ok(None)
    }
}

#[test]
fn opening_and_moving_a_band_keeps_the_books_reconciled() {
    let config = config();
    let mut executor = Executor::open(&config, open_venue(&config).expect("venue")).expect("opens");
    let open = executor.on_decision(
        &market(1, 1.0),
        LiquidityAction::Open(PriceBand::around(1.0, 0.05)),
        None,
    );
    assert!(open.feedback.band_held);
    assert_eq!(open.feedback.alert, None);

    let moved = executor.on_decision(
        &market(2, 1.08),
        LiquidityAction::Reposition(PriceBand::around(1.08, 0.05)),
        None,
    );
    let labels: Vec<_> = moved.orders.iter().map(|order| order.label()).collect();
    assert_eq!(labels.first(), Some(&"remove_band"));
    assert_eq!(labels.last(), Some(&"add_band"));
    assert_eq!(moved.feedback.alert, None, "ledger and venue agree");
}

/// Fixed wallets; every order is recorded and filled as a no-op.
struct RecordingVenue {
    wallet: Holding,
    orders: Rc<RefCell<Vec<VenueOrder>>>,
}

impl Venue for RecordingVenue {
    fn observe(&mut self, _: &MarketSnapshot) -> Result<(), VenueError> {
        Ok(())
    }

    fn execute(&mut self, order: &VenueOrder, _: &MarketSnapshot) -> Result<Fill, VenueError> {
        self.orders.borrow_mut().push(*order);
        Ok(Fill::default())
    }

    fn balances(&mut self, _: Wallet) -> Result<Holding, VenueError> {
        Ok(self.wallet)
    }

    fn position(&mut self, _: &MarketSnapshot) -> Result<Option<PositionView>, VenueError> {
        Ok(None)
    }
}

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
    let mut executor = Executor::open(&config, Box::new(venue)).expect("opens");
    let band = PriceBand {
        lower: 0.037_735_865_780_957_32,
        upper: 0.056_065_437_887_358_93,
    };
    executor.on_decision(
        &market(1, 0.045_775_623_848_488_815),
        LiquidityAction::Open(band),
        None,
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
fn holding_places_no_orders() {
    let config = config();
    let mut executor = Executor::open(&config, open_venue(&config).expect("venue")).expect("opens");
    let outcome = executor.on_decision(
        &market(1, 1.0),
        LiquidityAction::Hold(HoldReason::InBand),
        None,
    );
    assert!(outcome.orders.is_empty());
    assert!(outcome.books.is_some());
}

#[test]
fn approved_treasury_buy_spends_treasury_quote_only() {
    let config = config();
    let mut executor = Executor::open(&config, open_venue(&config).expect("venue")).expect("opens");
    let outcome = executor.on_decision(
        &market(1, 1.0),
        LiquidityAction::Hold(HoldReason::InBand),
        Some(250.0),
    );
    assert!(outcome.treasury_filled);
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
    let mut executor = Executor::open(&config, Box::new(BrokenVenue)).expect("opens");
    let open = LiquidityAction::Open(PriceBand::around(1.0, 0.05));
    let alerts: Vec<_> = (0..3)
        .map(|tick| {
            executor
                .on_decision(&market(tick, 1.0), open, None)
                .feedback
                .alert
        })
        .collect();
    assert_eq!(alerts[0], None);
    assert_eq!(alerts[2], Some(EmergencyCause::VenueFailures));
}

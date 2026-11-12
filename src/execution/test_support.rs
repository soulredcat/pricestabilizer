//! Shared fixtures for the executor tests: the example config, snapshots,
//! requests, and venues that fail or record.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::config::engine_config::EngineConfig;
use crate::engine::liquidity::action::LiquidityAction;
use crate::execution::request::ExecutionRequest;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::order::{Fill, OrderGuard, PositionView, VenueError, VenueOrder, Wallet};

pub const EXAMPLE: &str = include_str!("../../config/simulation.example.toml");

pub fn config() -> EngineConfig {
    EngineConfig::parse(EXAMPLE, Path::new("example")).expect("valid example")
}

pub fn market(timestamp: u64, spot_price: f64) -> MarketSnapshot {
    MarketSnapshot {
        timestamp,
        spot_price,
        quote_depth: 50_000.0,
        quote_volume: 400.0,
        quote_usd_price: 1.0,
    }
}

pub fn request(
    action: LiquidityAction,
    treasury_buy_quote: Option<f64>,
    price: f64,
) -> ExecutionRequest {
    ExecutionRequest {
        action,
        treasury_buy_quote,
        valuation_price: price,
    }
}

/// A venue whose wallets exist but whose orders never go through.
pub struct BrokenVenue;

impl Venue for BrokenVenue {
    fn observe(&mut self, _: &MarketSnapshot) -> Result<(), VenueError> {
        Ok(())
    }

    fn execute(
        &mut self,
        _: &VenueOrder,
        _: &OrderGuard,
        _: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
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

/// Fixed wallets; every order is recorded and filled as a no-op.
pub struct RecordingVenue {
    pub wallet: Holding,
    pub orders: Rc<RefCell<Vec<VenueOrder>>>,
}

impl Venue for RecordingVenue {
    fn observe(&mut self, _: &MarketSnapshot) -> Result<(), VenueError> {
        Ok(())
    }

    fn execute(
        &mut self,
        order: &VenueOrder,
        _: &OrderGuard,
        _: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
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

/// The example config with its venue swapped for a dry-run external one.
pub fn dry_run_config() -> EngineConfig {
    let start = EXAMPLE.find("[venue]").expect("venue section");
    let end = EXAMPLE.find("[treasury]").expect("treasury section");
    let venue = "[venue]\nkind = \"external\"\ncommand = [\"connector\"]\ntimeout_secs = 5\nsend_orders = false\n\n";
    let text = format!("{}{venue}{}", &EXAMPLE[..start], &EXAMPLE[end..]);
    EngineConfig::parse(&text, Path::new("dry-run")).expect("valid config")
}

pub fn token_short_wallet() -> Holding {
    Holding {
        token: 100_000_000_000,
        quote: 9_000_000_000,
    }
}

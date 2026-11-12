//! Accumulates per-feed statistics from decisions and their execution.

use std::collections::BTreeMap;

use crate::app::session::{TickEvent, TickReport};
use crate::engine::liquidity::action::LiquidityAction;
use crate::engine::liquidity::price_band::PriceBand;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::pnl_tracker::BooksSnapshot;
use crate::treasury::proposal::TreasuryEvent;

pub type Counter = BTreeMap<&'static str, u64>;

pub struct ProtectionTrip {
    pub after_secs: u64,
    pub cause: &'static str,
}

#[derive(Default)]
pub struct RunSummary {
    pub feed_name: String,
    pub ticks: u64,
    first_timestamp: u64,
    pub elapsed_secs: u64,
    pub first_spot: f64,
    pub last_spot: f64,
    pub last_target: f64,
    /// Lowest and highest `spot / target − 1` seen.
    pub lowest_target_gap: f64,
    pub highest_target_gap: f64,
    pub regimes: Counter,
    pub modes: Counter,
    pub price_issues: Counter,
    pub liquidity: Counter,
    pub ticks_with_band: u64,
    pub ticks_spot_in_band: u64,
    pub latest_band: Option<PriceBand>,
    pub protection: Option<ProtectionTrip>,
    /// Order results by label: filled order kinds, risk blocks, failures.
    pub orders: Counter,
    /// Books as last marked.
    pub books: Option<BooksSnapshot>,
    /// Operator and treasury events by label.
    pub events: Counter,
    /// Refused input by reason; none of it was acted on.
    pub skipped: Counter,
    /// Ticks where fee/LVR was defined, its sum, and ticks where it was >= 1.
    pub fee_to_lvr_ticks: u64,
    pub fee_to_lvr_sum: f64,
    pub fee_to_lvr_covered: u64,
    pub treasury_spent: f64,
}

impl RunSummary {
    pub fn new(feed_name: String) -> Self {
        Self {
            feed_name,
            ..Self::default()
        }
    }

    pub fn skip(&mut self, reason: &'static str) {
        bump(&mut self.skipped, reason);
    }

    pub fn record(&mut self, snapshot: &MarketSnapshot, report: &TickReport) {
        let decision = &report.decision;
        let outcome = &report.outcome;
        if self.ticks == 0 {
            self.first_timestamp = snapshot.timestamp;
            self.first_spot = snapshot.spot_price;
            self.lowest_target_gap = f64::INFINITY;
            self.highest_target_gap = f64::NEG_INFINITY;
        }
        self.ticks += 1;
        self.elapsed_secs = snapshot.timestamp.saturating_sub(self.first_timestamp);
        self.last_spot = snapshot.spot_price;
        self.last_target = decision.target_price;

        let target_gap = snapshot.spot_price / decision.target_price - 1.0;
        self.lowest_target_gap = self.lowest_target_gap.min(target_gap);
        self.highest_target_gap = self.highest_target_gap.max(target_gap);

        bump(&mut self.regimes, decision.regime.regime.label());
        bump(&mut self.modes, decision.mode.label());
        for issue in &decision.price.issues {
            bump(&mut self.price_issues, issue.label());
        }
        bump(&mut self.liquidity, decision.liquidity.label());

        if let LiquidityAction::Open(band) | LiquidityAction::Reposition(band) = decision.liquidity
        {
            self.latest_band = Some(band);
        }
        if let Some(in_band) = decision.spot_in_band {
            self.ticks_with_band += 1;
            self.ticks_spot_in_band += u64::from(in_band);
        }
        if let (None, OperatingMode::Protection(cause)) = (&self.protection, decision.mode) {
            self.protection = Some(ProtectionTrip {
                after_secs: self.elapsed_secs,
                cause: cause.label(),
            });
        }
        if let Some(ratio) = decision.fee_to_lvr {
            self.fee_to_lvr_ticks += 1;
            self.fee_to_lvr_sum += ratio;
            self.fee_to_lvr_covered += u64::from(ratio >= 1.0);
        }
        for order in &outcome.orders {
            bump(&mut self.orders, order.label());
        }
        self.books = outcome.books.or(self.books);
        for event in &report.events {
            bump(&mut self.events, event.label());
            if let TickEvent::Treasury(TreasuryEvent::Executed { quote, .. }) = event {
                self.treasury_spent += quote;
            }
        }
    }
}

fn bump(counter: &mut Counter, label: &'static str) {
    *counter.entry(label).or_insert(0) += 1;
}

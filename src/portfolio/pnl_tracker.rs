//! Marks equity every snapshot and watches the loss limits. Fee and cost
//! totals come from the order runner, which sees every fill. Unclaimed fees
//! are reported apart from cash, never mixed in.

use crate::config::execution_sections::RiskConfig;

const SECONDS_PER_DAY: u64 = 86_400;

/// Books at one moment, all in quote.
#[derive(Debug, Clone, Copy, Default)]
pub struct BooksSnapshot {
    /// Wallets plus band position plus unclaimed fees, at the valuation price.
    pub equity: f64,
    /// The starting holdings, untouched, valued at the same price.
    pub hold_equity: f64,
    pub unclaimed_fees: f64,
    pub fees_claimed: f64,
    pub action_costs: f64,
    /// Swap fees and price impact paid, versus spot.
    pub swap_costs: f64,
    pub worst_drawdown: f64,
}

/// Running totals over every fill, in quote.
#[derive(Debug, Clone, Copy, Default)]
pub struct FillTotals {
    pub fees_claimed: f64,
    pub action_costs: f64,
    pub swap_costs: f64,
}

impl FillTotals {
    pub fn add(&mut self, fees_claimed: f64, action_cost: f64, swap_cost: f64) {
        self.fees_claimed += fees_claimed;
        self.action_costs += action_cost;
        self.swap_costs += swap_cost;
    }
}

pub struct PnlTracker {
    max_daily_loss_quote: f64,
    max_drawdown: f64,
    peak_equity: f64,
    day_open: Option<(u64, f64)>,
    worst_drawdown: f64,
}

impl PnlTracker {
    pub fn new(risk: &RiskConfig) -> Self {
        Self {
            max_daily_loss_quote: risk.max_daily_loss_quote,
            max_drawdown: risk.max_drawdown,
            peak_equity: 0.0,
            day_open: None,
            worst_drawdown: 0.0,
        }
    }

    /// Marks equity at `timestamp`; `true` when a loss limit is breached.
    pub fn mark(&mut self, timestamp: u64, equity: f64) -> bool {
        let day = timestamp / SECONDS_PER_DAY;
        let day_open_equity = match self.day_open {
            Some((open_day, open_equity)) if open_day == day => open_equity,
            _ => {
                self.day_open = Some((day, equity));
                equity
            }
        };
        self.peak_equity = self.peak_equity.max(equity);
        let drawdown = if self.peak_equity > 0.0 {
            1.0 - equity / self.peak_equity
        } else {
            0.0
        };
        self.worst_drawdown = self.worst_drawdown.max(drawdown);
        drawdown > self.max_drawdown || day_open_equity - equity > self.max_daily_loss_quote
    }

    /// After an operator resumes, losses so far are acknowledged: the peak
    /// and the day's opening equity restart from the next mark.
    pub fn reset_baseline(&mut self) {
        self.peak_equity = 0.0;
        self.day_open = None;
    }

    pub fn snapshot(
        &self,
        equity: f64,
        hold_equity: f64,
        unclaimed_fees: f64,
        totals: FillTotals,
    ) -> BooksSnapshot {
        BooksSnapshot {
            equity,
            hold_equity,
            unclaimed_fees,
            fees_claimed: totals.fees_claimed,
            action_costs: totals.action_costs,
            swap_costs: totals.swap_costs,
            worst_drawdown: self.worst_drawdown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FillTotals, PnlTracker};
    use crate::config::execution_sections::RiskConfig;

    fn tracker() -> PnlTracker {
        PnlTracker::new(&RiskConfig {
            max_swap_quote: 1.0,
            max_slippage: 0.01,
            min_quote_reserve: 0.0,
            max_token_exposure_quote: 1.0,
            max_daily_loss_quote: 100.0,
            max_drawdown: 0.2,
            max_consecutive_failures: 3,
            order_ttl_secs: 60,
            max_price_drift: 0.01,
            max_swap_shortfall: 0.02,
            max_action_cost_quote: 1.0,
        })
    }

    #[test]
    fn daily_loss_resets_each_utc_day() {
        let mut tracker = tracker();
        assert!(!tracker.mark(0, 1_000.0));
        assert!(!tracker.mark(3_600, 950.0));
        assert!(tracker.mark(7_200, 890.0));
        assert!(!tracker.mark(86_400, 890.0));
    }

    #[test]
    fn drawdown_is_measured_from_the_peak() {
        let mut tracker = tracker();
        tracker.mark(0, 1_000.0);
        tracker.mark(86_400, 1_500.0);
        assert!(!tracker.mark(2 * 86_400, 1_250.0));
        assert!(tracker.mark(3 * 86_400, 1_190.0));
        assert!(
            (tracker
                .snapshot(0.0, 0.0, 0.0, FillTotals::default())
                .worst_drawdown
                - (1.0 - 1_190.0 / 1_500.0))
                .abs()
                < 1e-12
        );
    }
}

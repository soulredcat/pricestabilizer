//! How an approved proposal is bought: in slices, never faster than the
//! slice interval, never more in a UTC day than a share of the market's
//! volume, and, when configured, only while the price is outside the
//! project's own band, so the treasury never trades against the project's
//! own liquidity.

use serde::{Deserialize, Serialize};

use crate::config::treasury_section::TreasuryConfig;

const SECONDS_PER_DAY: u64 = 86_400;

/// Market facts a slice decision needs.
#[derive(Debug, Clone, Copy)]
pub struct SliceConditions {
    pub now: u64,
    /// Estimated quote volume per day.
    pub daily_volume: f64,
    /// Spot sits inside the project's own operational band.
    pub spot_in_own_band: bool,
}

/// Slices bought so far; survives a restart with the treasury state.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SliceTally {
    pub last_slice_at: Option<u64>,
    pub day: u64,
    pub bought_today: f64,
}

impl SliceTally {
    /// The next slice, at most `remaining`; `None` while the rules say wait.
    pub fn next_slice(
        &self,
        config: &TreasuryConfig,
        remaining: f64,
        conditions: &SliceConditions,
    ) -> Option<f64> {
        if config.only_outside_own_band && conditions.spot_in_own_band {
            return None;
        }
        let rested = self.last_slice_at.is_none_or(|last| {
            conditions.now.saturating_sub(last) >= config.min_slice_interval_secs
        });
        if !rested {
            return None;
        }
        let bought_today = if self.day == conditions.now / SECONDS_PER_DAY {
            self.bought_today
        } else {
            0.0
        };
        let daily_room = config.max_daily_volume_share * conditions.daily_volume - bought_today;
        let quote = remaining.min(config.slice_quote).min(daily_room);
        (quote > 0.0).then_some(quote)
    }

    pub fn record(&mut self, now: u64, quote: f64) {
        let today = now / SECONDS_PER_DAY;
        if self.day != today {
            self.day = today;
            self.bought_today = 0.0;
        }
        self.bought_today += quote;
        self.last_slice_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::{SliceConditions, SliceTally};
    use crate::config::treasury_section::TreasuryConfig;

    fn config() -> TreasuryConfig {
        TreasuryConfig {
            budget_quote: 1_000.0,
            proposal_quote: 500.0,
            min_proposal_interval_secs: 0,
            proposal_ttl_secs: 100,
            slice_quote: 100.0,
            min_slice_interval_secs: 600,
            max_daily_volume_share: 0.25,
            only_outside_own_band: true,
        }
    }

    fn conditions(now: u64, daily_volume: f64, spot_in_own_band: bool) -> SliceConditions {
        SliceConditions {
            now,
            daily_volume,
            spot_in_own_band,
        }
    }

    #[test]
    fn slices_are_capped_by_size_interval_and_daily_volume() {
        let mut tally = SliceTally::default();
        let config = config();
        assert_eq!(
            tally.next_slice(&config, 500.0, &conditions(0, 10_000.0, false)),
            Some(100.0)
        );
        tally.record(0, 100.0);
        assert_eq!(
            tally.next_slice(&config, 400.0, &conditions(300, 10_000.0, false)),
            None
        );
        // 25% of 600 quote of daily volume leaves 50 after the first 100... none.
        assert_eq!(
            tally.next_slice(&config, 400.0, &conditions(600, 400.0, false)),
            None
        );
        assert_eq!(
            tally.next_slice(&config, 400.0, &conditions(600, 800.0, false)),
            Some(100.0)
        );
        // A new UTC day starts a new daily allowance.
        tally.record(600, 100.0);
        assert_eq!(
            tally.next_slice(&config, 30.0, &conditions(86_400, 800.0, false)),
            Some(30.0)
        );
    }

    #[test]
    fn never_buys_against_the_projects_own_band() {
        let tally = SliceTally::default();
        assert_eq!(
            tally.next_slice(&config(), 500.0, &conditions(0, 1e9, true)),
            None
        );
    }
}

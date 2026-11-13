//! Proposes accumulation while the engine is in accumulation review, waits
//! for an operator's approval, then buys the approved amount in slices while
//! it is still safe: never in protection, never without a trusted price, and
//! only as the slicing rules allow.

use serde::{Deserialize, Serialize};

use crate::config::treasury_section::TreasuryConfig;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::treasury::proposal::{Proposal, TreasuryEvent};
use crate::treasury::slicing::{SliceConditions, SliceTally};

/// A proposal counts as bought once less than this share of it remains:
/// unit rounding leaves crumbs that are not worth an order.
const COMPLETE_SHARE: f64 = 0.001;

/// One purchase the executor should make now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    pub proposal_id: u64,
    pub quote: f64,
}

pub struct TreasuryDesk {
    config: TreasuryConfig,
    spent: f64,
    next_id: u64,
    last_proposal_at: Option<u64>,
    pending: Option<Proposal>,
    /// When the pending proposal was approved or last bought a slice.
    approved_progress_at: Option<u64>,
    filled: f64,
    tally: SliceTally,
}

/// The budget spent, proposal numbering and slicing survive a restart: a
/// restart never refills the budget, and an old approval never matches a
/// new proposal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TreasuryState {
    pub spent: f64,
    pub next_id: u64,
    pub last_proposal_at: Option<u64>,
    pub pending: Option<Proposal>,
    pub approved_progress_at: Option<u64>,
    pub filled: f64,
    pub tally: SliceTally,
}

impl TreasuryDesk {
    pub fn new(config: &TreasuryConfig) -> Self {
        Self {
            config: config.clone(),
            spent: 0.0,
            next_id: 1,
            last_proposal_at: None,
            pending: None,
            approved_progress_at: None,
            filled: 0.0,
            tally: SliceTally::default(),
        }
    }

    pub fn state(&self) -> TreasuryState {
        TreasuryState {
            spent: self.spent,
            next_id: self.next_id,
            last_proposal_at: self.last_proposal_at,
            pending: self.pending,
            approved_progress_at: self.approved_progress_at,
            filled: self.filled,
            tally: self.tally,
        }
    }

    pub fn restore(&mut self, state: TreasuryState) {
        self.spent = state.spent;
        self.next_id = state.next_id;
        self.last_proposal_at = state.last_proposal_at;
        self.pending = state.pending;
        self.approved_progress_at = state.approved_progress_at;
        self.filled = state.filled;
        self.tally = state.tally;
    }

    pub fn approve(&mut self, id: u64, now: u64) -> Option<TreasuryEvent> {
        if self.pending_id() == Some(id) {
            self.approved_progress_at = Some(now);
            None
        } else {
            Some(TreasuryEvent::UnknownProposal(id))
        }
    }

    /// Cancels a pending proposal, including what an approval left unbought.
    pub fn reject(&mut self, id: u64) -> TreasuryEvent {
        if self.pending_id() == Some(id) {
            self.clear();
            TreasuryEvent::Rejected(id)
        } else {
            TreasuryEvent::UnknownProposal(id)
        }
    }

    /// Expires idle proposals, opens new ones, and returns the next slice of
    /// an approved proposal when it is safe and the slicing rules allow it.
    pub fn on_tick(
        &mut self,
        mode: OperatingMode,
        price_trusted: bool,
        conditions: &SliceConditions,
        events: &mut Vec<TreasuryEvent>,
    ) -> Option<Slice> {
        let now = conditions.now;
        if let Some(pending) = self.pending {
            let idle_since = self.approved_progress_at.unwrap_or(pending.created_at);
            if now.saturating_sub(idle_since) >= self.config.proposal_ttl_secs {
                self.clear();
                events.push(TreasuryEvent::Expired(pending.id));
            }
        }
        if let (Some(pending), Some(_)) = (self.pending, self.approved_progress_at) {
            let safe = price_trusted && !matches!(mode, OperatingMode::Protection(_));
            if !safe {
                return None;
            }
            let remaining = pending.quote - self.filled;
            return self
                .tally
                .next_slice(&self.config, remaining, conditions)
                .map(|quote| Slice {
                    proposal_id: pending.id,
                    quote,
                });
        }
        if self.pending.is_none()
            && mode == OperatingMode::AccumulationReview
            && self.may_propose(now)
        {
            let proposal = Proposal {
                id: self.next_id,
                created_at: now,
                quote: self.config.proposal_quote.min(self.remaining()),
            };
            self.next_id += 1;
            self.last_proposal_at = Some(now);
            self.pending = Some(proposal);
            events.push(TreasuryEvent::Proposed(proposal.id));
        }
        None
    }

    /// Records a slice. The quote actually spent counts against the budget;
    /// the proposal closes once bought in full, or when a slice fails.
    pub fn settle(
        &mut self,
        slice: Slice,
        spent: Option<f64>,
        now: u64,
        events: &mut Vec<TreasuryEvent>,
    ) {
        let Some(quote) = spent else {
            self.clear();
            events.push(TreasuryEvent::NotFilled(slice.proposal_id));
            return;
        };
        self.spent += quote;
        self.filled += quote;
        self.tally.record(now, quote);
        self.approved_progress_at = Some(now);
        events.push(TreasuryEvent::Bought {
            id: slice.proposal_id,
            quote,
        });
        if let Some(pending) = self.pending
            && pending.quote - self.filled <= pending.quote * COMPLETE_SHARE
        {
            self.clear();
            events.push(TreasuryEvent::Completed(pending.id));
        }
    }

    fn may_propose(&self, now: u64) -> bool {
        let rested = self
            .last_proposal_at
            .is_none_or(|last| now.saturating_sub(last) >= self.config.min_proposal_interval_secs);
        rested && self.remaining() > 0.0
    }

    fn remaining(&self) -> f64 {
        (self.config.budget_quote - self.spent).max(0.0)
    }

    fn pending_id(&self) -> Option<u64> {
        self.pending.map(|proposal| proposal.id)
    }

    fn clear(&mut self) {
        self.pending = None;
        self.approved_progress_at = None;
        self.filled = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::{Slice, TreasuryDesk};
    use crate::config::treasury_section::TreasuryConfig;
    use crate::engine::mode::operating_mode::OperatingMode;
    use crate::engine::regime::market_regime::EmergencyCause;
    use crate::treasury::proposal::TreasuryEvent;
    use crate::treasury::slicing::SliceConditions;

    fn desk() -> TreasuryDesk {
        TreasuryDesk::new(&TreasuryConfig {
            budget_quote: 300.0,
            proposal_quote: 200.0,
            min_proposal_interval_secs: 100,
            proposal_ttl_secs: 50,
            slice_quote: 100.0,
            min_slice_interval_secs: 10,
            max_daily_volume_share: 0.5,
            only_outside_own_band: true,
        })
    }

    fn at(now: u64) -> SliceConditions {
        SliceConditions {
            now,
            daily_volume: 1e9,
            spot_in_own_band: false,
        }
    }

    const REVIEW: OperatingMode = OperatingMode::AccumulationReview;

    #[test]
    fn proposes_but_never_buys_without_approval() {
        let mut desk = desk();
        let mut events = Vec::new();
        for now in [0, 10, 60] {
            assert_eq!(desk.on_tick(REVIEW, true, &at(now), &mut events), None);
        }
        assert_eq!(
            events,
            vec![TreasuryEvent::Proposed(1), TreasuryEvent::Expired(1)]
        );
    }

    #[test]
    fn an_approved_proposal_is_bought_in_slices_while_safe() {
        let mut desk = desk();
        let mut events = Vec::new();
        desk.on_tick(REVIEW, true, &at(0), &mut events);
        assert_eq!(desk.approve(1, 0), None);
        let protection = OperatingMode::Protection(EmergencyCause::OperatorPause);
        assert_eq!(desk.on_tick(protection, true, &at(5), &mut events), None);
        let first = desk.on_tick(REVIEW, true, &at(10), &mut events);
        assert_eq!(
            first,
            Some(Slice {
                proposal_id: 1,
                quote: 100.0
            })
        );
        desk.settle(first.expect("slice"), Some(100.0), 10, &mut events);
        assert_eq!(
            desk.on_tick(REVIEW, true, &at(15), &mut events),
            None,
            "interval"
        );
        let second = desk
            .on_tick(REVIEW, true, &at(20), &mut events)
            .expect("slice");
        desk.settle(second, Some(100.0), 20, &mut events);
        assert_eq!(events.last(), Some(&TreasuryEvent::Completed(1)));
        desk.on_tick(REVIEW, true, &at(200), &mut events);
        assert_eq!(events.last(), Some(&TreasuryEvent::Proposed(2)));
        assert_eq!(desk.reject(2), TreasuryEvent::Rejected(2));
        assert_eq!(desk.reject(2), TreasuryEvent::UnknownProposal(2));
    }

    #[test]
    fn an_approval_that_buys_nothing_expires() {
        let mut desk = desk();
        let mut events = Vec::new();
        desk.on_tick(REVIEW, true, &at(0), &mut events);
        desk.approve(1, 0);
        let inside_own_band = super::SliceConditions {
            spot_in_own_band: true,
            ..at(30)
        };
        assert_eq!(
            desk.on_tick(REVIEW, true, &inside_own_band, &mut events),
            None
        );
        desk.on_tick(REVIEW, true, &at(50), &mut events);
        assert!(events.contains(&TreasuryEvent::Expired(1)));
    }
}

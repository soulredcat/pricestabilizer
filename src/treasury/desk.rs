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

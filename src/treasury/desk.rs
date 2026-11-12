//! Proposes accumulation while the engine is in accumulation review, waits
//! for an operator's approval, and executes only while it is still safe:
//! never in protection and never without a trusted price.

use crate::config::treasury_section::TreasuryConfig;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::treasury::proposal::{Proposal, TreasuryEvent};

pub struct TreasuryDesk {
    config: TreasuryConfig,
    spent: f64,
    next_id: u64,
    last_proposal_at: Option<u64>,
    pending: Option<Proposal>,
    approved: bool,
}

impl TreasuryDesk {
    pub fn new(config: &TreasuryConfig) -> Self {
        Self {
            config: config.clone(),
            spent: 0.0,
            next_id: 1,
            last_proposal_at: None,
            pending: None,
            approved: false,
        }
    }

    pub fn approve(&mut self, id: u64) -> Option<TreasuryEvent> {
        if self.pending_id() == Some(id) {
            self.approved = true;
            None
        } else {
            Some(TreasuryEvent::UnknownProposal(id))
        }
    }

    pub fn reject(&mut self, id: u64) -> TreasuryEvent {
        if self.pending_id() == Some(id) {
            self.clear();
            TreasuryEvent::Rejected(id)
        } else {
            TreasuryEvent::UnknownProposal(id)
        }
    }

    /// Expires stale proposals, opens new ones, and returns an approved
    /// proposal when it is safe to execute right now.
    pub fn on_tick(
        &mut self,
        now: u64,
        mode: OperatingMode,
        price_trusted: bool,
        events: &mut Vec<TreasuryEvent>,
    ) -> Option<Proposal> {
        if let Some(pending) = self.pending
            && now.saturating_sub(pending.created_at) >= self.config.proposal_ttl_secs
        {
            self.clear();
            events.push(TreasuryEvent::Expired(pending.id));
        }
        let safe = price_trusted && !matches!(mode, OperatingMode::Protection(_));
        if self.approved && safe {
            return self.pending;
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

    /// Records how an executed proposal went; it is closed either way.
    pub fn settle(&mut self, proposal: Proposal, filled: bool, events: &mut Vec<TreasuryEvent>) {
        self.clear();
        if filled {
            self.spent += proposal.quote;
            events.push(TreasuryEvent::Executed {
                id: proposal.id,
                quote: proposal.quote,
            });
        } else {
            events.push(TreasuryEvent::NotFilled(proposal.id));
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
        self.approved = false;
    }
}

#[cfg(test)]
mod tests {
    use super::TreasuryDesk;
    use crate::config::treasury_section::TreasuryConfig;
    use crate::engine::mode::operating_mode::OperatingMode;
    use crate::engine::regime::market_regime::EmergencyCause;
    use crate::treasury::proposal::TreasuryEvent;

    fn desk() -> TreasuryDesk {
        TreasuryDesk::new(&TreasuryConfig {
            budget_quote: 300.0,
            proposal_quote: 200.0,
            min_proposal_interval_secs: 100,
            proposal_ttl_secs: 50,
        })
    }

    const REVIEW: OperatingMode = OperatingMode::AccumulationReview;

    #[test]
    fn proposes_but_never_buys_without_approval() {
        let mut desk = desk();
        let mut events = Vec::new();
        assert_eq!(desk.on_tick(0, REVIEW, true, &mut events), None);
        assert_eq!(desk.on_tick(10, REVIEW, true, &mut events), None);
        assert_eq!(desk.on_tick(60, REVIEW, true, &mut events), None);
        assert_eq!(
            events,
            vec![TreasuryEvent::Proposed(1), TreasuryEvent::Expired(1)]
        );
    }

    #[test]
    fn approved_proposal_waits_for_safety_then_spends_the_budget() {
        let mut desk = desk();
        let mut events = Vec::new();
        desk.on_tick(0, REVIEW, true, &mut events);
        assert_eq!(desk.approve(1), None);
        let protection = OperatingMode::Protection(EmergencyCause::OperatorPause);
        assert_eq!(desk.on_tick(10, protection, true, &mut events), None);
        let proposal = desk
            .on_tick(20, REVIEW, true, &mut events)
            .expect("executable");
        desk.settle(proposal, true, &mut events);
        desk.on_tick(200, REVIEW, true, &mut events);
        assert_eq!(events.last(), Some(&TreasuryEvent::Proposed(2)));
        let second = desk.on_tick(201, REVIEW, true, &mut Vec::new());
        assert_eq!(second, None, "proposal 2 is not approved");
        assert_eq!(desk.reject(2), TreasuryEvent::Rejected(2));
        assert_eq!(desk.reject(2), TreasuryEvent::UnknownProposal(2));
    }
}

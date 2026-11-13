//! Treasury desk: proposals, approval, slices and expiry.

use crate::config::treasury_section::TreasuryConfig;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::treasury::desk::{Slice, TreasuryDesk};
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
    let inside_own_band = SliceConditions {
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

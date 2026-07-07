//! Tests for presence-conditioned reward accrual.
//!
//! Deterministic. Real `did:mini` identity roots stand in for the delegators named in
//! verdicts; the verdicts themselves are constructed directly (public fields).

use did_mini::{Controller, Did};
use mini_presence::PresenceVerdict;
use mini_reward::{accrue, ledger, RewardParams};

fn human(a: u8, b: u8) -> Did {
    Controller::incept_single_from_seeds(&[a; 32], &[b; 32])
        .unwrap()
        .did()
}

fn verdict(x: &Did, y: &Did, at_ms: u64) -> PresenceVerdict {
    PresenceVerdict {
        initiator_root: x.clone(),
        responder_root: y.clone(),
        at_ms,
    }
}

/// Base profile: no rate cap, no maturation — isolates the accrual arithmetic.
fn simple() -> RewardParams {
    RewardParams {
        base_points: 1_000,
        max_repeats_per_counterparty: 5,
        window_ms: 0,
        max_points_per_window: 0,
        maturation_ms: 0,
    }
}

#[test]
fn single_copresence_accrues_base_for_both() {
    let (a, b) = (human(1, 2), human(3, 4));
    let vs = vec![verdict(&a, &b, 1_000)];
    let acct = accrue(&a, &vs, &simple(), 2_000);
    assert_eq!(acct.accrued_points, 1_000);
    assert_eq!(acct.vested_points, 1_000); // maturation 0 -> vested now
    assert_eq!(acct.distinct_counterparties, 1);
    assert_eq!(acct.event_count, 1);
    // The other party accrues symmetrically.
    assert_eq!(accrue(&b, &vs, &simple(), 2_000).accrued_points, 1_000);
}

#[test]
fn diversity_beats_repetition() {
    let (a, b, c, d) = (human(1, 2), human(3, 4), human(5, 6), human(7, 8));

    // Three encounters with the SAME counterparty: 1000 + 500 + 250.
    let repeats = vec![
        verdict(&a, &b, 1_000),
        verdict(&a, &b, 2_000),
        verdict(&a, &b, 3_000),
    ];
    let r = accrue(&a, &repeats, &simple(), 9_999);
    assert_eq!(r.accrued_points, 1_750);
    assert_eq!(r.distinct_counterparties, 1);

    // Three encounters with DISTINCT counterparties: 1000 * 3.
    let diverse = vec![
        verdict(&a, &b, 1_000),
        verdict(&a, &c, 2_000),
        verdict(&a, &d, 3_000),
    ];
    let dv = accrue(&a, &diverse, &simple(), 9_999);
    assert_eq!(dv.accrued_points, 3_000);
    assert_eq!(dv.distinct_counterparties, 3);

    assert!(dv.accrued_points > r.accrued_points);
}

#[test]
fn repeats_stop_counting_after_the_cap() {
    let (a, b) = (human(1, 2), human(3, 4));
    let params = RewardParams {
        max_repeats_per_counterparty: 2,
        ..simple()
    };
    let vs = vec![
        verdict(&a, &b, 1_000),
        verdict(&a, &b, 2_000),
        verdict(&a, &b, 3_000), // beyond the cap -> 0
    ];
    // 1000 + 500 only.
    assert_eq!(accrue(&a, &vs, &params, 9_999).accrued_points, 1_500);
}

#[test]
fn per_window_rate_cap_slows_accrual() {
    let (a, b, c, d) = (human(1, 2), human(3, 4), human(5, 6), human(7, 8));
    let params = RewardParams {
        base_points: 1_000,
        max_repeats_per_counterparty: 5,
        window_ms: 1_000,
        max_points_per_window: 1_200,
        maturation_ms: 0,
    };
    // Window 0 (t=100,200): 1000 + min(1000, 200) = 1200 (capped).
    // Window 1 (t=1500):    1000.
    let vs = vec![
        verdict(&a, &b, 100),
        verdict(&a, &c, 200),
        verdict(&a, &d, 1_500),
    ];
    assert_eq!(accrue(&a, &vs, &params, 9_999).accrued_points, 2_200);
}

#[test]
fn contributions_vest_only_after_maturation() {
    let (a, b) = (human(1, 2), human(3, 4));
    let params = RewardParams {
        maturation_ms: 1_000,
        window_ms: 0,
        max_points_per_window: 0,
        ..simple()
    };
    let vs = vec![verdict(&a, &b, 500)];
    // now=1000: event matures at 1500 -> not vested yet, but accrued.
    let early = accrue(&a, &vs, &params, 1_000);
    assert_eq!(early.accrued_points, 1_000);
    assert_eq!(early.vested_points, 0);
    // now=2000: matured.
    assert_eq!(accrue(&a, &vs, &params, 2_000).vested_points, 1_000);
}

#[test]
fn uninvolved_human_accrues_nothing() {
    let (a, b, e) = (human(1, 2), human(3, 4), human(9, 10));
    let vs = vec![verdict(&a, &b, 1_000)];
    let acct = accrue(&e, &vs, &simple(), 2_000);
    assert_eq!(acct.accrued_points, 0);
    assert_eq!(acct.event_count, 0);
}

#[test]
fn self_pairing_is_ignored() {
    let a = human(1, 2);
    let vs = vec![verdict(&a, &a, 1_000)];
    let acct = accrue(&a, &vs, &simple(), 2_000);
    assert_eq!(acct.accrued_points, 0);
    assert_eq!(acct.event_count, 0);
}

#[test]
fn ledger_is_complete_sorted_and_order_independent() {
    let (a, b, c) = (human(1, 2), human(3, 4), human(5, 6));
    let vs = vec![verdict(&a, &b, 1_000), verdict(&b, &c, 2_000)];
    let l = ledger(&vs, &simple(), 9_999);

    // All three identity roots present.
    assert_eq!(l.len(), 3);
    // Sorted by identifier.
    let ids: Vec<&str> = l.iter().map(|acct| acct.identity_root.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    // b met both a and c.
    let b_acct = l
        .iter()
        .find(|acct| acct.identity_root.as_str() == b.as_str())
        .unwrap();
    assert_eq!(b_acct.distinct_counterparties, 2);

    // Reordering the input does not change the ledger.
    let shuffled = vec![verdict(&b, &c, 2_000), verdict(&a, &b, 1_000)];
    assert_eq!(ledger(&shuffled, &simple(), 9_999), l);
}

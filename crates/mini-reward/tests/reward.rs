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

// ---- storage-commitment accrual (founder decision, 2026-07-07) ----

use mini_crypto::{encoding, HashAlgorithm, Multihash};
use mini_objects::ObjectId;
use mini_reward::{accrue_storage, storage_ledger, StorageRewardParams};
use mini_storage::ServeVerdict;

fn simple_storage() -> StorageRewardParams {
    StorageRewardParams {
        points_per_gib: 100,
        max_repeats_per_witness: 5,
        window_ms: 0,
        max_points_per_window: 0,
        maturation_ms: 0,
    }
}

/// A cheap, structurally-valid but otherwise meaningless content id — no
/// object-signing needed, `accrue_storage` never inspects it.
fn fake_content_id(tag: u64) -> ObjectId {
    let mh = Multihash::of(HashAlgorithm::Blake3, &tag.to_be_bytes());
    let s = encoding::encode(encoding::BASE58BTC, &mh.to_bytes()).unwrap();
    ObjectId::parse(&s).unwrap()
}

fn witnessed(host: &Did, witness: &Did, gib: u64, at_ms: u64) -> ServeVerdict {
    ServeVerdict {
        host_root: host.clone(),
        witness_root: witness.clone(),
        content_id: fake_content_id(at_ms),
        bytes: gib * (1u64 << 30),
        at_ms,
    }
}

#[test]
fn a_fresh_witness_accrues_bytes_scaled_points() {
    let (host, witness) = (human(1, 2), human(3, 4));
    let ws = vec![witnessed(&host, &witness, 5, 1_000)];
    let acct = accrue_storage(&host, &ws, &simple_storage(), 2_000);
    assert_eq!(acct.accrued_points, 500); // 5 GiB * 100 pts/GiB
    assert_eq!(acct.vested_points, 500); // maturation 0 -> vested now
    assert_eq!(acct.distinct_counterparties, 1);
    assert_eq!(acct.event_count, 1);
}

#[test]
fn only_the_host_accrues_not_the_witness() {
    let (host, witness) = (human(1, 2), human(3, 4));
    let ws = vec![witnessed(&host, &witness, 5, 1_000)];
    let witness_acct = accrue_storage(&witness, &ws, &simple_storage(), 2_000);
    assert_eq!(witness_acct.accrued_points, 0);
    assert_eq!(witness_acct.event_count, 0);
}

#[test]
fn a_host_cannot_witness_and_pay_itself() {
    let host = human(1, 2);
    let ws = vec![witnessed(&host, &host, 5, 1_000)];
    let acct = accrue_storage(&host, &ws, &simple_storage(), 2_000);
    assert_eq!(acct.accrued_points, 0);
    assert_eq!(acct.event_count, 0);
}

#[test]
fn diversity_of_witnesses_beats_repeated_witnessing() {
    let (host, a, b, c) = (human(1, 2), human(3, 4), human(5, 6), human(7, 8));

    // Three commitments witnessed by the SAME peer: 500 + 250 + 125.
    let repeats = vec![
        witnessed(&host, &a, 5, 1_000),
        witnessed(&host, &a, 5, 2_000),
        witnessed(&host, &a, 5, 3_000),
    ];
    let r = accrue_storage(&host, &repeats, &simple_storage(), 9_999);
    assert_eq!(r.accrued_points, 875);
    assert_eq!(r.distinct_counterparties, 1);

    // Three commitments witnessed by DISTINCT peers: 500 * 3.
    let diverse = vec![
        witnessed(&host, &a, 5, 1_000),
        witnessed(&host, &b, 5, 2_000),
        witnessed(&host, &c, 5, 3_000),
    ];
    let dv = accrue_storage(&host, &diverse, &simple_storage(), 9_999);
    assert_eq!(dv.accrued_points, 1_500);
    assert_eq!(dv.distinct_counterparties, 3);

    assert!(dv.accrued_points > r.accrued_points);
}

#[test]
fn repeated_witness_commitments_stop_counting_after_the_cap() {
    let (host, witness) = (human(1, 2), human(3, 4));
    let params = StorageRewardParams {
        max_repeats_per_witness: 2,
        ..simple_storage()
    };
    let ws = vec![
        witnessed(&host, &witness, 5, 1_000),
        witnessed(&host, &witness, 5, 2_000),
        witnessed(&host, &witness, 5, 3_000), // beyond the cap -> 0
    ];
    // 500 + 250 only.
    assert_eq!(
        accrue_storage(&host, &ws, &params, 9_999).accrued_points,
        750
    );
}

#[test]
fn storage_accrual_respects_the_per_window_rate_cap() {
    let (host, a, b, c) = (human(1, 2), human(3, 4), human(5, 6), human(7, 8));
    let params = StorageRewardParams {
        points_per_gib: 100,
        max_repeats_per_witness: 5,
        window_ms: 1_000,
        max_points_per_window: 600,
        maturation_ms: 0,
    };
    // Window 0 (t=100,200): 500 + min(500, 100) = 600 (capped).
    // Window 1 (t=1500):    500.
    let ws = vec![
        witnessed(&host, &a, 5, 100),
        witnessed(&host, &b, 5, 200),
        witnessed(&host, &c, 5, 1_500),
    ];
    assert_eq!(
        accrue_storage(&host, &ws, &params, 9_999).accrued_points,
        1_100
    );
}

#[test]
fn storage_commitments_vest_only_after_maturation() {
    let (host, witness) = (human(1, 2), human(3, 4));
    let params = StorageRewardParams {
        maturation_ms: 1_000,
        ..simple_storage()
    };
    let ws = vec![witnessed(&host, &witness, 5, 500)];
    // now=1000: event matures at 1500 -> not vested yet, but accrued.
    let early = accrue_storage(&host, &ws, &params, 1_000);
    assert_eq!(early.accrued_points, 500);
    assert_eq!(early.vested_points, 0);
    // now=2000: matured.
    assert_eq!(
        accrue_storage(&host, &ws, &params, 2_000).vested_points,
        500
    );
}

#[test]
fn sub_gib_commitments_round_down_to_zero_points() {
    // Bytes-scaled accrual never gives points for a fractional GiB — this
    // keeps the accrual math from being gamed with many tiny "commitments".
    let (host, witness) = (human(1, 2), human(3, 4));
    let ws = vec![ServeVerdict {
        host_root: host.clone(),
        witness_root: witness.clone(),
        content_id: fake_content_id(1),
        bytes: (1u64 << 30) - 1, // just under 1 GiB
        at_ms: 1_000,
    }];
    let acct = accrue_storage(&host, &ws, &simple_storage(), 2_000);
    assert_eq!(acct.accrued_points, 0);
    assert_eq!(acct.event_count, 0);
}

#[test]
fn storage_ledger_is_complete_sorted_and_order_independent() {
    let (a, b, c) = (human(1, 2), human(3, 4), human(5, 6));
    let ws = vec![witnessed(&a, &b, 5, 1_000), witnessed(&b, &c, 5, 2_000)];
    let l = storage_ledger(&ws, &simple_storage(), 9_999);

    // Only host roots appear (a and b hosted; c never hosted anything).
    assert_eq!(l.len(), 2);
    let ids: Vec<&str> = l.iter().map(|acct| acct.identity_root.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);

    // Reordering the input does not change the ledger.
    let shuffled = vec![witnessed(&b, &c, 5, 2_000), witnessed(&a, &b, 5, 1_000)];
    assert_eq!(storage_ledger(&shuffled, &simple_storage(), 9_999), l);
}

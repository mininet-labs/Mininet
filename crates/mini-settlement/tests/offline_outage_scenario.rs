//! End-to-end offline-outage scenario (roadmap #41's own framing): a payer
//! goes offline, signs conflicting claims to two different merchants (or
//! simply loses connectivity mid-transaction and a stale local copy gets
//! replayed), and reconnection must resolve to exactly one canonical
//! winner — never a merge, never both, never neither once resolved.

use mini_crypto::SigningKey;
use mini_settlement::{
    evaluate_local_acceptance, reconcile, InMemoryClaimWatcher, InMemoryLedgerView,
    LocalAcceptancePolicy, SettlementState,
};

fn payer() -> SigningKey {
    SigningKey::from_seed(&[0x77; 32])
}

/// The full narrative: a payer signs two conflicting claims while
/// partitioned from the network (double-spend attempt, or an honest
/// mistake — the protocol can't and shouldn't try to tell the difference).
/// Two merchants, each in their own partition, locally accept low-risk
/// claims. On reconnection, the canonical ledger resolves the conflict:
/// exactly one merchant's claim is honored.
#[test]
fn a_double_spend_across_two_partitions_resolves_to_exactly_one_winner() {
    let payer = payer();

    // Two conflicting claims at the same sequence -- the payer's outage-time
    // "signed promises," per Directive 5, made to two different merchants.
    let claim_to_merchant_a = mini_settlement::sign_claim(
        &payer,
        b"merchant-a-address",
        400,
        0,
        10_000,
        b"chain-head-42",
        0,
    )
    .unwrap();
    let claim_to_merchant_b = mini_settlement::sign_claim(
        &payer,
        b"merchant-b-address",
        400,
        0,
        10_000,
        b"chain-head-42",
        0,
    )
    .unwrap();

    // Each merchant is in their own partition -- they never see each
    // other's claim locally, so their own ClaimWatcher sees no conflict.
    let policy = LocalAcceptancePolicy {
        max_amount_micro_without_finality: 1_000,
    };
    let mut watcher_a = InMemoryClaimWatcher::new();
    let mut watcher_b = InMemoryClaimWatcher::new();

    let local_state_a =
        evaluate_local_acceptance(&claim_to_merchant_a, &policy, 0, &mut watcher_a).unwrap();
    let local_state_b =
        evaluate_local_acceptance(&claim_to_merchant_b, &policy, 0, &mut watcher_b).unwrap();

    // Both merchants locally accepted -- and both know, by the type they
    // hold, that this is a risk decision, not a fact.
    assert_eq!(local_state_a, SettlementState::AcceptedLocal);
    assert_eq!(local_state_b, SettlementState::AcceptedLocal);
    assert!(!local_state_a.is_final());
    assert!(!local_state_b.is_final());

    // Reconnection: the network heals, and the canonical chain (built by
    // roadmap #36-#45, represented here by the trait this crate defines)
    // finalizes exactly one of the two conflicting claims.
    let mut ledger = InMemoryLedgerView::new();
    ledger.finalize(
        &claim_to_merchant_a.payer,
        0,
        mini_settlement::claim_digest(&claim_to_merchant_a),
    );

    let final_state_a = reconcile(&claim_to_merchant_a, &ledger, 100).unwrap();
    let final_state_b = reconcile(&claim_to_merchant_b, &ledger, 100).unwrap();

    assert_eq!(final_state_a, SettlementState::Finalized);
    assert_eq!(final_state_b, SettlementState::RejectedConflict);

    // The exact property M1/M3 exist to guarantee: never both, never
    // neither, once the canonical ledger has spoken.
    assert!(final_state_a.is_final());
    assert!(!final_state_b.is_final());
}

/// A payer who never sends a second conflicting claim sees the ordinary,
/// unremarkable path: local accept (a risk decision) followed by real
/// finality (a fact) for the very same claim.
#[test]
fn the_honest_path_reaches_finality_for_the_same_claim_that_was_locally_accepted() {
    let payer = payer();
    let claim = mini_settlement::sign_claim(&payer, b"merchant", 50, 0, 10_000, b"chain-head-1", 0)
        .unwrap();

    let policy = LocalAcceptancePolicy {
        max_amount_micro_without_finality: 1_000,
    };
    let mut watcher = InMemoryClaimWatcher::new();
    assert_eq!(
        evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap(),
        SettlementState::AcceptedLocal
    );

    let mut ledger = InMemoryLedgerView::new();
    ledger.finalize(&claim.payer, 0, mini_settlement::claim_digest(&claim));
    assert_eq!(
        reconcile(&claim, &ledger, 100).unwrap(),
        SettlementState::Finalized
    );
}

/// A high-value claim above the merchant's risk threshold is never
/// AcceptedLocal, no matter how confident the merchant might otherwise
/// feel -- the policy threshold is the only lever, and it must be an
/// explicit, deliberate choice (see `LocalAcceptancePolicy::never_accept_early`).
#[test]
fn high_value_claims_always_wait_for_canonical_finality() {
    let payer = payer();
    let claim = mini_settlement::sign_claim(
        &payer,
        b"merchant",
        1_000_000,
        0,
        10_000,
        b"chain-head-1",
        0,
    )
    .unwrap();
    let policy = LocalAcceptancePolicy {
        max_amount_micro_without_finality: 1_000,
    };
    let mut watcher = InMemoryClaimWatcher::new();
    assert_eq!(
        evaluate_local_acceptance(&claim, &policy, 0, &mut watcher).unwrap(),
        SettlementState::PendingCanonical
    );
}

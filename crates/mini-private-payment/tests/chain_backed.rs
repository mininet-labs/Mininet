//! Reconciling a shielded payment against real canonical finality
//! (roadmap R5).
//!
//! Until this existed, `PrivateLedgerView` had exactly one implementation —
//! `InMemoryPrivateLedger`, which finalizes whatever it is told to — so no
//! private payment could reach `Finalized` on the strength of anything but
//! a test double's say-so. This is the private analogue of the gap D-0061
//! closed for the transparent path.
//!
//! ## The seam, and why it is a seam rather than a function call
//!
//! The canonical state lives in `mini-execution`, which depends on
//! `mini-chain`. This crate depends on `mini-value`. A dependency edge
//! between them — in either direction — would be the first path in this
//! tree from a value crate to the crate that counts votes, which P1 and
//! Directive 16 forbid. There is no such path today and this is exactly the
//! change that would have created one.
//!
//! So the halves meet through `(Vec<u8>, [u8; 32])`: a key image and a claim
//! digest. Nothing is shared but standard-library types, and neither side
//! can drift from a format that does not exist.
//!
//! What keeps them honest about *semantics* is a pair of tests rather than a
//! compiler. `a_chain_shaped_ledger_resolves_a_shielded_conflict` below
//! asserts the exact map that
//! `the_finalized_map_is_the_one_the_shielded_side_expects` in
//! `mini-execution/tests/shielded_ordering.rs` produces from a block body.
//! Change what either side means by "finalized" and one of the two fails.

mod support;

use std::collections::BTreeMap;

use mini_private_payment::{
    reconcile, verify, ChainBackedPrivateLedger, KeyImageSet, SpendOutcome,
};
use mini_settlement::{CanonicalRejection, SettlementState};
use support::{pay, payment_to, recipient, request_for, Ledger, NETWORK};

/// A stand-in for `mini_execution::LedgerState`'s shielded map: key image →
/// the digest of the claim that first spent it. Deliberately the same shape
/// and nothing more, because that is the entire surface the two layers
/// share.
#[derive(Default)]
struct FinalizedNullifiers {
    spent: BTreeMap<Vec<u8>, [u8; 32]>,
}

impl FinalizedNullifiers {
    /// Finalize every input of one claim, the way `apply_block` does for a
    /// group that wins.
    fn finalize(&mut self, key_images: &[Vec<u8>], claim_digest: [u8; 32]) {
        for key_image in key_images {
            self.spent.insert(key_image.clone(), claim_digest);
        }
    }

    fn lookup(&self) -> impl Fn(&[u8]) -> Option<[u8; 32]> + '_ {
        move |key_image| self.spent.get(key_image).copied()
    }
}

#[test]
fn a_payment_the_chain_has_not_seen_is_pending_not_final() {
    // M2: a verified claim is a signed promise, not ownership. An empty
    // canonical state must never render as final.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1_000, b"pending");
    let verified = verify(&claim, &NETWORK).unwrap();

    let chain = FinalizedNullifiers::default();
    let ledger = ChainBackedPrivateLedger::new(chain.lookup());

    let state = reconcile(&verified, &ledger, 0).unwrap();
    assert_eq!(state, SettlementState::PendingCanonical);
    assert!(!state.is_final());
}

#[test]
fn a_payment_the_chain_finalized_is_final() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1_000, b"final");
    let verified = verify(&claim, &NETWORK).unwrap();

    let mut chain = FinalizedNullifiers::default();
    let images: Vec<Vec<u8>> = verified.key_images().map(|i| i.to_vec()).collect();
    chain.finalize(&images, *verified.transcript_digest());

    let ledger = ChainBackedPrivateLedger::new(chain.lookup());
    let state = reconcile(&verified, &ledger, 0).unwrap();
    assert_eq!(state, SettlementState::Finalized);
    assert!(state.is_final());
}

#[test]
fn a_finalized_payment_stays_final_after_its_deadline() {
    // Value that moved cannot un-move because a device clock advanced.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"clock");
    let verified = verify(&claim, &NETWORK).unwrap();

    let mut chain = FinalizedNullifiers::default();
    let images: Vec<Vec<u8>> = verified.key_images().map(|i| i.to_vec()).collect();
    chain.finalize(&images, *verified.transcript_digest());

    let ledger = ChainBackedPrivateLedger::new(chain.lookup());
    assert_eq!(
        reconcile(&verified, &ledger, u64::MAX).unwrap(),
        SettlementState::Finalized
    );
}

#[test]
fn a_payment_the_chain_never_saw_expires() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"expiry");
    let verified = verify(&claim, &NETWORK).unwrap();

    let chain = FinalizedNullifiers::default();
    let ledger = ChainBackedPrivateLedger::new(chain.lookup());
    assert_eq!(
        reconcile(&verified, &ledger, u64::MAX).unwrap(),
        SettlementState::Expired
    );
}

#[test]
fn a_chain_shaped_ledger_resolves_a_shielded_conflict() {
    // **The seam test.** The map built below is exactly what
    // `the_finalized_map_is_the_one_the_shielded_side_expects` in
    // mini-execution/tests/shielded_ordering.rs proves `apply_block`
    // produces from a block body: claim A spends {0x11, 0x22} and wins both;
    // claim B spends {0x33, 0x22}, collides on 0x22, and takes nothing.
    //
    // The two crates cannot be linked (P1, Directive 16), so this pair of
    // tests is what keeps them honest about each other. Change what either
    // means by "finalized" and one of them fails.
    const CLAIM_A: [u8; 32] = [0xa1; 32];

    let mut chain = FinalizedNullifiers::default();
    chain.finalize(&[vec![0x11; 32], vec![0x22; 32]], CLAIM_A);

    let ledger = ChainBackedPrivateLedger::new(chain.lookup());

    // Reading the chain's answer back, key image for key image.
    assert_eq!(chain.spent.get(&vec![0x11; 32]).copied(), Some(CLAIM_A));
    assert_eq!(chain.spent.get(&vec![0x22; 32]).copied(), Some(CLAIM_A));
    assert_eq!(chain.spent.get(&vec![0x33; 32]), None);
    assert_eq!(chain.spent.len(), 2);

    // And the adapter reports the same through the trait the shielded side
    // actually calls.
    use mini_private_payment::PrivateLedgerView;
    assert_eq!(ledger.finalized_claim(&[0x11; 32]), Some(CLAIM_A));
    assert_eq!(ledger.finalized_claim(&[0x33; 32]), None);
}

#[test]
fn the_loser_of_a_real_double_spend_is_rejected_by_the_chains_answer() {
    // End to end with real claims rather than literal bytes: two claims
    // spending the same output, the chain finalizes one, and the other is
    // RejectedConflict -- never merged, netted, or preferred for arriving
    // later.
    let to = recipient();
    let (ledger_fixture, spend) = Ledger::with_funds(500);

    let make = |purpose: &[u8]| {
        let mut request = request_for(vec![spend.clone()], vec![pay(&to, 500, purpose)], 0);
        request.decoy_entropy = [0x71; 32];
        let (claim, _) = mini_private_payment::build(&request, &ledger_fixture).unwrap();
        verify(&claim, &NETWORK).unwrap()
    };
    let winner = make(b"winner");
    let loser = make(b"loser");
    assert_ne!(winner.transcript_digest(), loser.transcript_digest());

    let mut chain = FinalizedNullifiers::default();
    let images: Vec<Vec<u8>> = winner.key_images().map(|i| i.to_vec()).collect();
    chain.finalize(&images, *winner.transcript_digest());

    let view = ChainBackedPrivateLedger::new(chain.lookup());
    assert_eq!(
        reconcile(&winner, &view, 0).unwrap(),
        SettlementState::Finalized
    );
    assert_eq!(
        reconcile(&loser, &view, 0).unwrap(),
        SettlementState::RejectedConflict
    );
    assert!(!reconcile(&loser, &view, 0).unwrap().is_final());

    // Exactly one finalized. Nothing was combined.
    assert_eq!(
        [&winner, &loser]
            .iter()
            .filter(|claim| reconcile(claim, &view, 0).unwrap().is_final())
            .count(),
        1
    );
}

#[test]
fn a_partial_overlap_still_loses_against_the_chain() {
    // The multi-input case, read from canonical state rather than from a
    // local nullifier set: claim A spends {X, Y}, claim B spends {Z, Y}.
    // Once the chain has finalized A, B must be RejectedConflict even
    // though B's *first* input is untouched.
    let to = recipient();
    let mut fixture = Ledger::new();
    fixture.fill(mini_private_payment::MIN_RING_SIZE * 4);
    let x = fixture.mint(300);
    let y = fixture.mint(700);
    let z = fixture.mint(400);

    let first = mini_private_payment::build(
        &request_for(vec![x, y.clone()], vec![pay(&to, 1_000, b"a")], 0),
        &fixture,
    )
    .unwrap()
    .0;
    let second = mini_private_payment::build(
        &request_for(vec![z, y], vec![pay(&to, 1_100, b"b")], 0),
        &fixture,
    )
    .unwrap()
    .0;
    let first = verify(&first, &NETWORK).unwrap();
    let second = verify(&second, &NETWORK).unwrap();

    let mut chain = FinalizedNullifiers::default();
    let images: Vec<Vec<u8>> = first.key_images().map(|i| i.to_vec()).collect();
    chain.finalize(&images, *first.transcript_digest());

    let view = ChainBackedPrivateLedger::new(chain.lookup());
    assert_eq!(
        reconcile(&first, &view, 0).unwrap(),
        SettlementState::Finalized
    );
    assert_eq!(
        reconcile(&second, &view, 0).unwrap(),
        SettlementState::RejectedConflict
    );

    // The local nullifier set reaches the same verdict without the chain,
    // which is what lets an offline device refuse a double spend it can
    // already see -- it just cannot call it final (M2).
    let mut local = KeyImageSet::new();
    assert_eq!(local.observe(&first), SpendOutcome::Accepted);
    assert!(matches!(
        local.observe(&second),
        SpendOutcome::Conflict { .. }
    ));
}

#[test]
fn a_canonical_rejection_is_reported_when_the_chain_records_one() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"rejected");
    let verified = verify(&claim, &NETWORK).unwrap();
    let digest = *verified.transcript_digest();

    let chain = FinalizedNullifiers::default();
    let view =
        ChainBackedPrivateLedger::with_rejections(chain.lookup(), move |asked: &[u8; 32]| {
            (*asked == digest).then_some(CanonicalRejection::WrongNetwork)
        });

    assert_eq!(
        reconcile(&verified, &view, 0).unwrap(),
        SettlementState::RejectedCanonical(CanonicalRejection::WrongNetwork)
    );
}

#[test]
fn a_chain_with_no_rejection_source_reports_none_rather_than_inventing_one() {
    // Honest rather than lossy: a chain that records no rejection reasons
    // has none to report, and a fabricated reason would be worse than its
    // absence.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"norej");
    let verified = verify(&claim, &NETWORK).unwrap();

    let chain = FinalizedNullifiers::default();
    let view = ChainBackedPrivateLedger::new(chain.lookup());
    assert_eq!(
        reconcile(&verified, &view, 0).unwrap(),
        SettlementState::PendingCanonical
    );
}

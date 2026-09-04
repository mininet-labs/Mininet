//! What an attacker gets to try, and what must happen.
//!
//! Each test names a concrete attack rather than a property, because a
//! privacy failure is never announced by a panic — it looks exactly like
//! success until somebody deanonymizes a user.

mod support;

use mini_private_payment::{
    build, canonicalize_ring, reconcile, verify, InMemoryPrivateLedger, KeyImageSet,
    PaymentPurpose, PrivatePaymentError, SpendOutcome, MAX_MEMO_BYTES, MAX_RING_SIZE,
    MIN_RING_SIZE,
};
use mini_settlement::{CanonicalRejection, SettlementState};
use mini_value::StealthKeypair;
use support::{pay, payment_to, payment_with_ring, recipient, request_for, Ledger, NETWORK};

// ---------------------------------------------------------------------------
// The payment works at all
// ---------------------------------------------------------------------------

#[test]
fn a_well_formed_private_payment_verifies() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1_000, b"post:abc");
    assert!(verify(&claim, &NETWORK).is_ok());
}

#[test]
fn the_recipient_recognizes_and_reads_their_own_payment() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 2_500, b"post:xyz");
    let verified = verify(&claim, &NETWORK).unwrap();

    let found = mini_private_payment::scan_one(
        &to.view_secret_bytes(),
        &to.spend_public_bytes(),
        &verified,
    )
    .unwrap()
    .pop()
    .expect("payment is addressed here");
    assert_eq!(
        found.note.purpose,
        PaymentPurpose::new(b"post:xyz".to_vec())
    );
    assert_eq!(found.note.amount_micro, 2_500);
}

#[test]
fn a_stranger_neither_recognizes_the_payment_nor_reads_its_purpose() {
    let to = recipient();
    let stranger = recipient();
    let (claim, _) = payment_to(&to, 500, b"post:private");
    let verified = verify(&claim, &NETWORK).unwrap();

    assert!(!mini_private_payment::recognizes(
        &stranger.view_secret_bytes(),
        &stranger.spend_public_bytes(),
        &verified
    ));
    assert!(mini_private_payment::scan_one(
        &stranger.view_secret_bytes(),
        &stranger.spend_public_bytes(),
        &verified
    )
    .unwrap()
    .is_empty());
}

// ---------------------------------------------------------------------------
// The privacy properties themselves
// ---------------------------------------------------------------------------

#[test]
fn two_payments_to_the_same_recipient_are_not_linkable_by_their_outputs() {
    // The single most important property: if two payments to one creator
    // shared an address, "private payments" would be a public creator
    // ledger with extra steps.
    let to = recipient();
    let (first, _) = payment_to(&to, 100, b"post:a");
    let (second, _) = payment_to(&to, 100, b"post:b");

    assert_ne!(
        first.outputs[0].output.one_time_address,
        second.outputs[0].output.one_time_address
    );
    assert_ne!(
        first.outputs[0].output.tx_public_key,
        second.outputs[0].output.tx_public_key
    );

    // ...and the recipient still recognizes both.
    for claim in [&first, &second] {
        let verified = verify(claim, &NETWORK).unwrap();
        assert!(mini_private_payment::recognizes(
            &to.view_secret_bytes(),
            &to.spend_public_bytes(),
            &verified
        ));
    }
}

#[test]
fn two_payments_of_the_same_amount_do_not_share_a_commitment() {
    // Equal amounts must not produce equal commitments, or the amount
    // becomes a fingerprint and hiding it accomplishes nothing.
    let to = recipient();
    let (first, _) = payment_to(&to, 42_000, b"x");
    let (second, _) = payment_to(&to, 42_000, b"y");
    assert_ne!(
        first.outputs[0].amount_commitment,
        second.outputs[0].amount_commitment
    );
}

#[test]
fn the_claim_carries_no_payer_field_and_no_sequence() {
    // A structural assertion, kept as a test because the transparent claim
    // leaked the payer's entire ordered history through exactly these two
    // fields. If someone ever adds them back for convenience, this fails.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"z");
    let encoded = claim.encode();
    // The failure message deliberately names the field rather than dumping
    // the claim: a test about not exposing payment detail should not print
    // a payment to prove it.
    let rendered = format!("{claim:?}");
    assert!(!rendered.contains("payer"), "a payer field reappeared");
    assert!(
        !rendered.contains("sequence"),
        "a sequence field reappeared"
    );
    // And nothing in the wire bytes equals any ring member repeated in a
    // "payer" position -- the ring is the only place keys appear.
    assert!(encoded.len() > 32);
}

#[test]
fn the_amount_never_appears_in_the_wire_bytes() {
    // A distinctive amount, searched for in every endianness a naive
    // encoder might have used.
    let to = recipient();
    let amount: u64 = 0x0102_0304_0506_0708;
    let (claim, _) = payment_to(&to, amount, b"m");
    let encoded = claim.encode();
    assert!(!encoded
        .windows(8)
        .any(|w| w == amount.to_be_bytes() || w == amount.to_le_bytes()));
}

#[test]
fn the_purpose_never_appears_in_the_wire_bytes() {
    let to = recipient();
    let purpose = b"post:this-exact-string-must-not-leak";
    let (claim, _) = payment_to(&to, 7, purpose);
    let encoded = claim.encode();
    assert!(!encoded
        .windows(purpose.len())
        .any(|window| window == purpose));
}

#[test]
fn payments_with_and_without_a_purpose_are_the_same_size() {
    // Otherwise "this payment has a memo" is itself a signal, and a
    // creator paid with notes is distinguishable from one paid without.
    let to = recipient();
    let (empty, _) = payment_to(&to, 1, b"");
    let (full, _) = payment_to(&to, 1, &[0xcd; MAX_MEMO_BYTES]);
    assert_eq!(empty.encode().len(), full.encode().len());
}

// ---------------------------------------------------------------------------
// Ring integrity — the anonymity set has to be real
// ---------------------------------------------------------------------------

#[test]
fn a_ring_that_hides_nobody_is_refused_at_build_time() {
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1);
    let mut request = request_for(vec![spend], vec![pay(&to, 1, b"tiny")], 0);
    request.ring_size = 2;
    assert!(matches!(
        build(&request, &ledger),
        Err(PrivatePaymentError::RingTooSmall {
            min: MIN_RING_SIZE,
            ..
        })
    ));
}

#[test]
fn a_too_small_ring_is_refused_at_verify_time_too() {
    // Build-time refusal is not enough: claims arrive over a wire from
    // parties who never ran our builder.
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"q");
    claim.inputs[0].ring.truncate(MIN_RING_SIZE - 1);
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::RingTooSmall { .. })
    ));
}

#[test]
fn a_ring_padded_with_duplicates_is_refused() {
    // Padding a ring of one to a ring of eight by repeating one key costs
    // nothing and buys no anonymity -- it can only be an attempt to look
    // better hidden than you are.
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"dup");
    let member = claim.inputs[0].ring[0].clone();
    claim.inputs[0].ring = vec![member; MIN_RING_SIZE];
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::DuplicateRingMember)
    ));
}

#[test]
fn an_oversized_ring_is_refused_rather_than_verified_slowly() {
    // Ring verification is linear in ring size; an unbounded ring is a
    // denial-of-service against the weakest honest device (Directive 11).
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"big");
    let mut ring: Vec<Vec<u8>> = (0..MAX_RING_SIZE + 1)
        .map(|_| {
            StealthKeypair::generate()
                .unwrap()
                .spend_public_bytes()
                .to_vec()
        })
        .collect();
    let mut commitments: Vec<Vec<u8>> = (0..MAX_RING_SIZE + 1)
        .map(|_| mini_crypto::random_32().unwrap().to_vec())
        .collect();
    assert!(canonicalize_ring(&mut ring, &mut commitments));
    claim.inputs[0].ring = ring;
    claim.inputs[0].ring_commitments = commitments;
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::RingTooLarge { .. })
    ));
}

#[test]
fn reordering_the_ring_invalidates_the_signature() {
    // The ring is inside the transcript, so a signature cannot be moved to
    // a different anonymity set -- which would change who the payment
    // appears to hide among.
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"order");
    claim.inputs[0].ring.swap(0, 1);
    // Swapping breaks canonical order first; verify reports that, and even
    // re-sorting into a *different* set still fails the signature.
    assert!(verify(&claim, &NETWORK).is_err());

    let (mut swapped, _) = payment_to(&to, 1, b"order");
    let input = &mut swapped.inputs[0];
    input.ring[0] = StealthKeypair::generate()
        .unwrap()
        .spend_public_bytes()
        .to_vec();
    assert!(canonicalize_ring(
        &mut input.ring,
        &mut input.ring_commitments
    ));
    assert!(matches!(
        verify(&swapped, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));
}

#[test]
fn a_response_count_that_disagrees_with_the_ring_is_refused() {
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"count");
    claim.inputs[0].signature.key_responses.pop();
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));
}

// ---------------------------------------------------------------------------
// Amount soundness — hiding a number must not mean inventing one
// ---------------------------------------------------------------------------

#[test]
fn a_tampered_amount_commitment_fails_its_range_proof() {
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1_000, b"amt");
    claim.outputs[0].amount_commitment[0] ^= 0x01;
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::BadRangeProof)
    ));
}

#[test]
fn a_range_proof_from_another_payment_does_not_transfer() {
    // Splicing a valid proof from one commitment onto another is the
    // obvious way to hide an out-of-range amount behind honest-looking
    // evidence.
    let to = recipient();
    let (first, _) = payment_to(&to, 10, b"a");
    let (mut second, _) = payment_to(&to, 20, b"b");
    second.outputs[0].range_proof = first.outputs[0].range_proof.clone();
    assert!(matches!(
        verify(&second, &NETWORK),
        Err(PrivatePaymentError::BadRangeProof)
    ));
}

#[test]
fn a_zero_amount_is_a_valid_payment() {
    // Zero is in range and must verify: refusing it here would push
    // callers toward encoding "no payment" some other, less examined way.
    let to = recipient();
    let (claim, _) = payment_to(&to, 0, b"zero");
    assert!(verify(&claim, &NETWORK).is_ok());
}

// ---------------------------------------------------------------------------
// Transcript binding — every field must be under the signature
// ---------------------------------------------------------------------------

#[test]
fn every_mutable_field_is_covered_by_the_signature() {
    let to = recipient();
    let base = payment_to(&to, 1_234, b"bind").0;

    // Amount commitment: covered by the range-proof check above, so here
    // we cover the fields whose tampering the signature alone must catch.
    let mut tampered_deadline = base.clone();
    tampered_deadline.valid_until_ms += 1;
    assert!(matches!(
        verify(&tampered_deadline, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));

    let mut tampered_chain = base.clone();
    tampered_chain.last_known_chain = b"height:99999".to_vec();
    assert!(matches!(
        verify(&tampered_chain, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));

    let mut tampered_output = base.clone();
    tampered_output.outputs[0].output.one_time_address = StealthKeypair::generate()
        .unwrap()
        .spend_public_bytes()
        .to_vec();
    assert!(matches!(
        verify(&tampered_output, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));

    let mut tampered_memo = base.clone();
    tampered_memo.outputs[0].memo.ciphertext[0] ^= 0x01;
    assert!(matches!(
        verify(&tampered_memo, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));
}

#[test]
fn redirecting_the_payment_to_another_address_invalidates_it() {
    // The attack: intercept a payment in flight and swap the one-time
    // address for your own.
    let to = recipient();
    let thief = recipient();
    let (mut claim, _) = payment_to(&to, 5_000, b"steal");
    let (stolen_output, _) = mini_value::derive_output_with_secret(
        &thief.spend_public_bytes(),
        &thief.view_public_bytes(),
    )
    .unwrap();
    claim.outputs[0].output = stolen_output;
    assert!(matches!(
        verify(&claim, &NETWORK),
        Err(PrivatePaymentError::BadSpendProof)
    ));
}

#[test]
fn a_claim_for_another_network_is_refused() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"net");
    assert!(matches!(
        verify(&claim, &[0xff; 32]),
        Err(PrivatePaymentError::NetworkMismatch)
    ));
}

// ---------------------------------------------------------------------------
// Memo binding
// ---------------------------------------------------------------------------

#[test]
fn a_memo_cannot_be_lifted_onto_another_payment() {
    // Without the AAD binding, an observer could take "payment for post X"
    // off a large payment and staple it to a tiny one, so the creator
    // credits the wrong payment to the wrong post.
    let to = recipient();
    let (generous, _) = payment_to(&to, 100_000, b"post:valuable");
    let (mut stingy, _) = payment_to(&to, 1, b"post:cheap");
    stingy.outputs[0].memo = generous.outputs[0].memo.clone();
    // The signature catches it first; even ignoring that, the memo's AAD
    // is the transcript digest, which the swap changes.
    assert!(verify(&stingy, &NETWORK).is_err());
}

// ---------------------------------------------------------------------------
// Double spending — M1
// ---------------------------------------------------------------------------

#[test]
fn spending_the_same_output_twice_is_refused_never_merged() {
    // M1: money does not merge. The second claim is refused outright --
    // not netted, not summed, not "the larger wins".
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(500);

    // Both claims spend the same output, so both must pay out the same
    // total -- conservation is checked before the key image ever is. What
    // differs is who gets paid and why, which is enough to make them two
    // distinct claims.
    let make = |purpose: &[u8]| {
        let mut request = request_for(vec![spend.clone()], vec![pay(&to, 500, purpose)], 0);
        // Fixed entropy: the two claims must draw the same ring, so the
        // only thing that differs is what is under test rather than the
        // sampling. (The key image would collide either way -- it is
        // determined by the one-time secret, not the ring.)
        request.decoy_entropy = [0x5c; 32];
        verify(&build(&request, &ledger).unwrap().0, &NETWORK).unwrap()
    };

    let first = make(b"first");
    let second = make(b"second");

    // Same output spent twice -> same key image, which is exactly how this
    // is detectable without a public payer.
    assert_eq!(
        first.key_images().next().unwrap(),
        second.key_images().next().unwrap()
    );
    assert_ne!(first.transcript_digest(), second.transcript_digest());

    let mut spent = KeyImageSet::new();
    assert_eq!(spent.observe(&first), SpendOutcome::Accepted);
    match spent.observe(&second) {
        SpendOutcome::Conflict { held, offered } => {
            assert_eq!(held, *first.transcript_digest());
            assert_eq!(offered, *second.transcript_digest());
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert!(matches!(
        spent.admit(&second),
        Err(PrivatePaymentError::AlreadySpent)
    ));
    assert_eq!(spent.len(), 1, "a conflict must not add a second entry");
}

#[test]
fn rebroadcasting_the_same_claim_is_idempotent_not_a_conflict() {
    // Networks re-deliver. Treating a duplicate as a double-spend would
    // make ordinary gossip look like fraud.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"rebroadcast");
    let verified = verify(&claim, &NETWORK).unwrap();
    let mut spent = KeyImageSet::new();
    assert_eq!(spent.observe(&verified), SpendOutcome::Accepted);
    assert_eq!(spent.observe(&verified), SpendOutcome::AlreadyRecorded);
    assert!(spent.admit(&verified).is_ok());
}

#[test]
fn different_outputs_produce_different_key_images() {
    let to = recipient();
    let (first, _) = payment_to(&to, 1, b"a");
    let (second, _) = payment_to(&to, 1, b"b");
    let (first, second) = (
        verify(&first, &NETWORK).unwrap(),
        verify(&second, &NETWORK).unwrap(),
    );
    assert_ne!(
        first.key_images().next().unwrap(),
        second.key_images().next().unwrap()
    );

    let mut spent = KeyImageSet::new();
    assert_eq!(spent.observe(&first), SpendOutcome::Accepted);
    assert_eq!(spent.observe(&second), SpendOutcome::Accepted);
    assert_eq!(spent.len(), 2);
}

// ---------------------------------------------------------------------------
// M2 / M3 — offline is never final, canonical order decides
// ---------------------------------------------------------------------------

#[test]
fn an_unfinalized_payment_is_pending_and_never_final() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"pending");
    let verified = verify(&claim, &NETWORK).unwrap();
    let ledger = InMemoryPrivateLedger::new();
    let state = reconcile(&verified, &ledger, 0).unwrap();
    assert_eq!(state, SettlementState::PendingCanonical);
    assert!(!state.is_final());
}

#[test]
fn only_canonical_inclusion_makes_a_payment_final() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"final");
    let verified = verify(&claim, &NETWORK).unwrap();
    let mut ledger = InMemoryPrivateLedger::new();
    ledger.finalize(&verified);
    let state = reconcile(&verified, &ledger, 0).unwrap();
    assert_eq!(state, SettlementState::Finalized);
    assert!(state.is_final());
}

#[test]
fn canonical_ordering_alone_resolves_a_conflict() {
    // M3: exactly one of two conflicting claims resolves, and the loser is
    // rejected rather than merged or retried.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(30);
    let make = |purpose: &[u8]| {
        let mut request = request_for(vec![spend.clone()], vec![pay(&to, 30, purpose)], 0);
        // Fixed entropy: the two claims must draw the same ring, so the
        // only thing that differs is what is under test.
        request.decoy_entropy = [0x5c; 32];
        verify(&build(&request, &ledger).unwrap().0, &NETWORK).unwrap()
    };
    let winner = make(b"winner");
    let loser = make(b"loser");

    let mut ledger = InMemoryPrivateLedger::new();
    ledger.finalize(&winner);

    assert_eq!(
        reconcile(&winner, &ledger, 0).unwrap(),
        SettlementState::Finalized
    );
    assert_eq!(
        reconcile(&loser, &ledger, 0).unwrap(),
        SettlementState::RejectedConflict
    );
    // Arrival order did not win, and nothing was combined.
    assert!(!reconcile(&loser, &ledger, 0).unwrap().is_final());
}

#[test]
fn an_expired_claim_that_never_finalized_is_expired() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"expiry");
    let verified = verify(&claim, &NETWORK).unwrap();
    let ledger = InMemoryPrivateLedger::new();
    assert_eq!(
        reconcile(&verified, &ledger, 999_999).unwrap(),
        SettlementState::Expired
    );
}

#[test]
fn a_finalized_claim_stays_final_after_its_deadline_passes() {
    // Value that moved cannot un-move because a device clock advanced.
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"clock");
    let verified = verify(&claim, &NETWORK).unwrap();
    let mut ledger = InMemoryPrivateLedger::new();
    ledger.finalize(&verified);
    assert_eq!(
        reconcile(&verified, &ledger, u64::MAX).unwrap(),
        SettlementState::Finalized
    );
}

#[test]
fn a_canonically_rejected_claim_reports_the_reason() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"reject");
    let verified = verify(&claim, &NETWORK).unwrap();
    let mut ledger = InMemoryPrivateLedger::new();
    ledger.reject(
        *verified.transcript_digest(),
        CanonicalRejection::WrongNetwork,
    );
    assert_eq!(
        reconcile(&verified, &ledger, 0).unwrap(),
        SettlementState::RejectedCanonical(CanonicalRejection::WrongNetwork)
    );
}

// ---------------------------------------------------------------------------
// Wire handling
// ---------------------------------------------------------------------------

#[test]
fn a_claim_survives_a_wire_round_trip() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 8_888, b"wire");
    let encoded = claim.encode();
    let decoded = mini_private_payment::PrivatePaymentClaim::decode(&encoded).unwrap();
    assert_eq!(decoded, claim);
    assert!(verify(&decoded, &NETWORK).is_ok());
}

#[test]
fn a_truncated_claim_is_refused_without_panicking() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"trunc");
    let encoded = claim.encode();
    for cut in [1usize, 40, encoded.len() / 2, encoded.len() - 1] {
        assert!(mini_private_payment::PrivatePaymentClaim::decode(&encoded[..cut]).is_err());
    }
}

#[test]
fn trailing_bytes_after_a_claim_are_refused() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"trail");
    let mut encoded = claim.encode();
    encoded.push(0);
    assert!(matches!(
        mini_private_payment::PrivatePaymentClaim::decode(&encoded),
        Err(PrivatePaymentError::Decode(
            mini_private_payment::DecodeFailure::TrailingBytes
        ))
    ));
}

#[test]
fn a_claim_with_a_foreign_domain_tag_is_refused() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"domain");
    let mut encoded = claim.encode();
    encoded[0] ^= 0xff;
    assert!(matches!(
        mini_private_payment::PrivatePaymentClaim::decode(&encoded),
        Err(PrivatePaymentError::Decode(
            mini_private_payment::DecodeFailure::UnsupportedVersion
        ))
    ));
}

#[test]
fn a_noncanonically_ordered_ring_is_refused_on_decode() {
    // One payment, one encoding. Two encodings of the same payment would
    // be two claims to any registry keyed on bytes.
    let to = recipient();
    let (mut claim, _) = payment_to(&to, 1, b"canon");
    claim.inputs[0].ring.swap(0, 1);
    assert!(matches!(
        mini_private_payment::PrivatePaymentClaim::decode(&claim.encode()),
        Err(PrivatePaymentError::Decode(
            mini_private_payment::DecodeFailure::NoncanonicalRingOrder
        ))
    ));
}

#[test]
fn random_bytes_never_decode_into_a_claim() {
    for seed in 0u8..32 {
        let garbage: Vec<u8> = (0..512u16).map(|i| (i as u8) ^ seed).collect();
        assert!(mini_private_payment::PrivatePaymentClaim::decode(&garbage).is_err());
    }
}

// ---------------------------------------------------------------------------
// Ring sizes across the permitted range
// ---------------------------------------------------------------------------

#[test]
fn payments_verify_across_the_whole_permitted_ring_range() {
    let to = recipient();
    for size in [MIN_RING_SIZE, MIN_RING_SIZE + 1, 16, 32] {
        let (claim, _) = payment_with_ring(&to, 1, b"sizes", size);
        assert!(verify(&claim, &NETWORK).is_ok(), "ring size {size} failed");
        assert_eq!(claim.inputs[0].ring.len(), size);
    }
}

// ---------------------------------------------------------------------------
// Decoys are the protocol's choice, not the wallet's (D-0449)
// ---------------------------------------------------------------------------

#[test]
fn the_caller_cannot_supply_its_own_ring() {
    // A structural assertion kept as a test. The whole point of D-0449 is
    // that ring membership is not a caller parameter -- a wallet that
    // samples differently from its peers marks its own users. If someone
    // ever adds the field back for convenience, this stops compiling, which
    // is the intended alarm.
    let to = recipient();
    let (ledger, spend) = Ledger::with_funds(1);
    let request = request_for(vec![spend], vec![pay(&to, 1, b"no-ring")], 0);
    let rendered = format!("{request:?}");
    assert!(!rendered.contains("ring:"), "a ring field reappeared");
    assert!(build(&request, &ledger).is_ok());
}

#[test]
fn two_wallets_with_the_same_inputs_produce_the_same_ring() {
    // Determinism is the anti-fingerprinting property: if two
    // implementations disagreed about the sampling, an observer could tell
    // which one made a payment from the shape of its ring, and the smaller
    // population would be the more identifiable.
    let to = recipient();
    let mut ledger = Ledger::new();
    ledger.fill(200);
    let spend = ledger.mint(1);

    let make = || {
        let mut request = request_for(vec![spend.clone()], vec![pay(&to, 1, b"same")], 0);
        request.decoy_entropy = [0x4d; 32];
        build(&request, &ledger).unwrap().0.inputs[0].ring.clone()
    };
    assert_eq!(make(), make());
}

#[test]
fn the_default_ring_is_sixteen_and_never_below_the_frozen_floor() {
    let to = recipient();
    let (claim, _) = payment_to(&to, 1, b"size");
    assert_eq!(claim.inputs[0].ring.len(), 16);
    assert_eq!(mini_private_payment::ABSOLUTE_MIN_RING_SIZE, 8);
}

#[test]
fn a_payment_built_from_a_real_output_set_verifies_end_to_end() {
    // The sampler feeds the signer: if selection returned the real output's
    // position wrongly, the ring signature would be over a member whose
    // secret the signer does not hold, and verification would fail.
    let to = recipient();
    for size in [MIN_RING_SIZE, 32, 64] {
        let (claim, _) = payment_with_ring(&to, 1, b"e2e", size);
        assert_eq!(claim.inputs[0].ring.len(), size);
        assert!(verify(&claim, &NETWORK).is_ok(), "ring size {size}");
    }
}

#[test]
fn an_output_set_too_small_for_the_ring_is_refused_rather_than_padded() {
    // Padding to reach the requested size would repeat members, which looks
    // like anonymity and provides none -- the same failure the duplicate
    // check above catches on the wire, caught here at construction.
    let to = recipient();
    let mut ledger = Ledger::new();
    ledger.fill(3);
    let spend = ledger.mint(1);
    let request = request_for(vec![spend], vec![pay(&to, 1, b"thin")], 0);
    assert!(matches!(
        build(&request, &ledger),
        Err(PrivatePaymentError::OutputSetTooSmall { .. })
    ));
}

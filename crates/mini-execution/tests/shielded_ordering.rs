//! The chain half of the shielded settlement path (roadmap R5).
//!
//! What is under test is **ordering**, not cryptography — the chain
//! deliberately cannot see a private claim's contents, so it cannot check
//! that one produced the key image it is finalizing. See
//! `mini_execution::nullifier`'s module docs for why that boundary exists
//! and what it leaves open.
//!
//! ## The seam these tests pin
//!
//! `mini-private-payment` cannot be linked from here: it reaches
//! `mini-value`, this crate reaches `mini-chain`, and a dependency edge
//! between them would be the first value-to-voice path in the tree (P1,
//! Directive 16). So the two halves are pinned from both sides against the
//! same literal bytes. The map this file's
//! `the_finalized_map_is_the_one_the_shielded_side_expects` produces is
//! asserted, key image for key image, by
//! `a_chain_shaped_ledger_resolves_a_shielded_conflict` in
//! `mini-private-payment/tests/chain_backed.rs`. If either side changes
//! what it means by "finalized", one of the two fails.

use mini_execution::{
    apply_block, LedgerState, NullifierRecord, SettlementBlockBody, MAX_KEY_IMAGE_BYTES,
    MAX_NULLIFIERS_PER_BLOCK,
};

const CLAIM_A: [u8; 32] = [0xa1; 32];
const CLAIM_B: [u8; 32] = [0xb2; 32];
const CLAIM_C: [u8; 32] = [0xc3; 32];

/// Key images are 32 bytes of Ristretto point, opaque here.
fn image(tag: u8) -> Vec<u8> {
    vec![tag; 32]
}

fn body(records: Vec<NullifierRecord>) -> SettlementBlockBody {
    SettlementBlockBody::new(Vec::new()).with_nullifiers(records)
}

#[test]
fn a_shielded_spend_is_finalized_and_readable_by_its_key_image() {
    let state = apply_block(
        &LedgerState::new(),
        &body(vec![NullifierRecord::new(image(1), CLAIM_A)]),
    )
    .unwrap();

    assert_eq!(state.finalized_nullifier(&image(1)), Some(CLAIM_A));
    assert_eq!(state.finalized_nullifier(&image(2)), None);
    assert_eq!(state.nullifier_count(), 1);
}

#[test]
fn the_first_claim_to_take_a_key_image_keeps_it_permanently() {
    // M1/M3, on the shielded side: body order decides, and the loser is
    // dropped rather than merged, netted, or preferred for being later.
    let state = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(1), CLAIM_B),
        ]),
    )
    .unwrap();

    assert_eq!(state.finalized_nullifier(&image(1)), Some(CLAIM_A));
    assert_eq!(state.nullifier_count(), 1, "the loser added nothing");

    // ...and across blocks, not only within one.
    let later = apply_block(&state, &body(vec![NullifierRecord::new(image(1), CLAIM_C)])).unwrap();
    assert_eq!(later.finalized_nullifier(&image(1)), Some(CLAIM_A));
}

#[test]
fn re_including_a_claim_already_finalized_changes_nothing() {
    // Networks re-deliver. A duplicate is not a double-spend, and treating
    // it as one would make ordinary gossip look like fraud.
    let first = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(2), CLAIM_A),
        ]),
    )
    .unwrap();
    let again = apply_block(
        &first,
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(2), CLAIM_A),
        ]),
    )
    .unwrap();

    assert_eq!(first.commitment(), again.commitment());
}

#[test]
fn a_claim_that_overlaps_on_one_input_takes_none_of_them() {
    // The bug this grouping exists to prevent. Claim A spends {1, 2}; claim
    // B spends {3, 2}. They collide on image 2 only, and not at the same
    // position. Applied record-by-record, B would take image 3 and lose
    // image 2 -- half a double-spend, finalized as a success, with output 3
    // burned by a claim no verifier accepts.
    //
    // That is the merge M1 forbids, arriving through partial application.
    let state = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(2), CLAIM_A),
            NullifierRecord::new(image(3), CLAIM_B),
            NullifierRecord::new(image(2), CLAIM_B),
        ]),
    )
    .unwrap();

    assert_eq!(state.finalized_nullifier(&image(1)), Some(CLAIM_A));
    assert_eq!(state.finalized_nullifier(&image(2)), Some(CLAIM_A));
    assert_eq!(
        state.finalized_nullifier(&image(3)),
        None,
        "B lost image 2, so B must not have taken image 3 either"
    );
    assert_eq!(state.nullifier_count(), 2);
}

#[test]
fn the_same_holds_when_the_collision_arrives_in_a_later_block() {
    let first = apply_block(
        &LedgerState::new(),
        &body(vec![NullifierRecord::new(image(2), CLAIM_A)]),
    )
    .unwrap();
    let second = apply_block(
        &first,
        &body(vec![
            NullifierRecord::new(image(3), CLAIM_B),
            NullifierRecord::new(image(2), CLAIM_B),
        ]),
    )
    .unwrap();

    assert_eq!(second.finalized_nullifier(&image(3)), None);
    assert_eq!(second.nullifier_count(), 1);
}

#[test]
fn a_malformed_record_takes_its_whole_claim_down_with_it() {
    // A body that failed to name one of a claim's inputs storably cannot
    // finalize that claim: the record it dropped is an input whose double
    // spend would then go undetected.
    for broken in [Vec::new(), vec![9u8; MAX_KEY_IMAGE_BYTES + 1]] {
        let state = apply_block(
            &LedgerState::new(),
            &body(vec![
                NullifierRecord::new(image(1), CLAIM_A),
                NullifierRecord::new(broken, CLAIM_A),
            ]),
        )
        .unwrap();
        assert_eq!(state.nullifier_count(), 0, "the whole group is dropped");
    }
}

#[test]
fn a_dropped_group_leaves_its_key_images_free_for_a_later_claim() {
    // The consequence that makes the previous test safe rather than merely
    // strict: refusing a group must not burn its inputs.
    let first = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(Vec::new(), CLAIM_A),
        ]),
    )
    .unwrap();
    let second = apply_block(&first, &body(vec![NullifierRecord::new(image(1), CLAIM_B)])).unwrap();

    assert_eq!(second.finalized_nullifier(&image(1)), Some(CLAIM_B));
}

#[test]
fn shielded_spends_change_the_state_commitment() {
    // Otherwise a block header's state_root would not commit to them, and a
    // node could serve a state that had quietly forgotten a spend.
    let empty = LedgerState::new();
    let with_spend =
        apply_block(&empty, &body(vec![NullifierRecord::new(image(1), CLAIM_A)])).unwrap();
    assert_ne!(empty.commitment(), with_spend.commitment());

    // And the commitment is over content, not insertion order: two states
    // reaching the same set of spends by different routes agree.
    let forward = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(2), CLAIM_B),
        ]),
    )
    .unwrap();
    let reversed = apply_block(
        &apply_block(
            &LedgerState::new(),
            &body(vec![NullifierRecord::new(image(2), CLAIM_B)]),
        )
        .unwrap(),
        &body(vec![NullifierRecord::new(image(1), CLAIM_A)]),
    )
    .unwrap();
    assert_eq!(forward.commitment(), reversed.commitment());
}

#[test]
fn the_body_hash_covers_the_shielded_records() {
    let without = SettlementBlockBody::new(Vec::new());
    let with = body(vec![NullifierRecord::new(image(1), CLAIM_A)]);
    assert_ne!(without.hash(), with.hash());

    // Reordering the records is a different body: order is what decides
    // conflicts, so it must not be free to permute.
    let one_way = body(vec![
        NullifierRecord::new(image(1), CLAIM_A),
        NullifierRecord::new(image(1), CLAIM_B),
    ]);
    let other_way = body(vec![
        NullifierRecord::new(image(1), CLAIM_B),
        NullifierRecord::new(image(1), CLAIM_A),
    ]);
    assert_ne!(one_way.hash(), other_way.hash());
}

#[test]
fn an_oversized_shielded_list_is_refused_before_anything_is_applied() {
    let too_many: Vec<_> = (0..MAX_NULLIFIERS_PER_BLOCK + 1)
        .map(|i| NullifierRecord::new(vec![(i % 251) as u8; 32], CLAIM_A))
        .collect();
    assert!(apply_block(&LedgerState::new(), &body(too_many)).is_err());
}

#[test]
fn a_snapshot_round_trip_preserves_every_shielded_spend() {
    // A restored state that forgot its nullifiers would treat every output
    // it had already finalized as unspent -- a replay of every private
    // payment the chain had ever seen, arriving through state sync.
    let state = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(image(1), CLAIM_A),
            NullifierRecord::new(image(2), CLAIM_A),
            NullifierRecord::new(image(3), CLAIM_B),
        ]),
    )
    .unwrap();

    let bytes = state.to_snapshot_bytes().unwrap();
    let restored = LedgerState::from_snapshot_bytes(&bytes).unwrap();

    assert_eq!(restored, state);
    assert_eq!(restored.commitment(), state.commitment());
    assert_eq!(restored.finalized_nullifier(&image(1)), Some(CLAIM_A));
    assert_eq!(restored.finalized_nullifier(&image(3)), Some(CLAIM_B));
}

#[test]
fn the_finalized_map_is_the_one_the_shielded_side_expects() {
    // The seam, pinned from this side. The literal pairs below are asserted
    // again -- as the map a shielded wallet reconciles against -- by
    // `a_chain_shaped_ledger_resolves_a_shielded_conflict` in
    // mini-private-payment/tests/chain_backed.rs. The two crates cannot be
    // linked (P1), so this pair of tests is what keeps them honest about
    // each other.
    //
    // Two claims, one shared input: A spends {0x11, 0x22}, B spends
    // {0x33, 0x22}. A is first in body order, so A takes both of its
    // inputs and B takes nothing.
    let state = apply_block(
        &LedgerState::new(),
        &body(vec![
            NullifierRecord::new(vec![0x11; 32], CLAIM_A),
            NullifierRecord::new(vec![0x22; 32], CLAIM_A),
            NullifierRecord::new(vec![0x33; 32], CLAIM_B),
            NullifierRecord::new(vec![0x22; 32], CLAIM_B),
        ]),
    )
    .unwrap();

    assert_eq!(state.finalized_nullifier(&[0x11; 32]), Some(CLAIM_A));
    assert_eq!(state.finalized_nullifier(&[0x22; 32]), Some(CLAIM_A));
    assert_eq!(state.finalized_nullifier(&[0x33; 32]), None);
    assert_eq!(state.nullifier_count(), 2);
}

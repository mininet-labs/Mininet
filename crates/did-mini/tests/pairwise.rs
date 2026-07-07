//! Pairwise pseudonym derivation (SPEC-01 §10, founder decision 2026-07-07):
//! one human, many independent, deterministically-recoverable, unlinkable-
//! by-default pseudonym roots — one function call per context, not N
//! hand-managed random seeds.

use did_mini::{Controller, IdentityError};
use mini_crypto::SigningKey;

fn root(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap()
}

#[test]
fn same_root_same_context_is_deterministic() {
    let r = root(1);
    let a = r.incept_pairwise_pseudonym(b"context-a").unwrap();
    let b = r.incept_pairwise_pseudonym(b"context-a").unwrap();
    assert_eq!(a.did().as_str(), b.did().as_str());
}

#[test]
fn different_contexts_yield_independent_looking_roots() {
    let r = root(1);
    let a = r.incept_pairwise_pseudonym(b"context-a").unwrap();
    let b = r.incept_pairwise_pseudonym(b"context-b").unwrap();
    assert_ne!(a.did().as_str(), b.did().as_str());
}

#[test]
fn different_master_roots_yield_different_pseudonyms_for_the_same_context() {
    let r1 = root(1);
    let r2 = root(50);
    let a = r1.incept_pairwise_pseudonym(b"same-context").unwrap();
    let b = r2.incept_pairwise_pseudonym(b"same-context").unwrap();
    assert_ne!(a.did().as_str(), b.did().as_str());
}

#[test]
fn the_pseudonym_is_an_ordinary_independent_root() {
    let r = root(1);
    let p = r.incept_pairwise_pseudonym(b"forum:xyz").unwrap();

    // It verifies fully offline, exactly like any other inception.
    let state = p.kel().verify().unwrap();
    assert_eq!(state.sn, 0);
    // It is not a delegated device of the master root (it is its own root).
    assert!(p.kel().delegator().is_none());
    // Its identifier differs from the master root's.
    assert_ne!(p.did().as_str(), r.did().as_str());
    // Its SCID self-certifies independently, with no reference to the
    // master root anywhere in its inception event.
    assert_eq!(p.kel().scid(), p.scid());
}

#[test]
fn pseudonym_roots_never_reveal_the_master_root_in_their_wire_bytes() {
    let r = root(1);
    let p = r.incept_pairwise_pseudonym(b"forum:xyz").unwrap();
    let wire = p.kel().to_bytes();
    // The derived root's wire bytes never contain the master root's SCID as
    // a substring — they are structurally unrelated encodings.
    let haystack = String::from_utf8_lossy(&wire);
    assert!(!haystack.contains(r.scid()));
}

#[test]
fn multi_key_roots_cannot_derive_a_pairwise_pseudonym() {
    let current = vec![
        SigningKey::from_seed(&[1u8; 32]),
        SigningKey::from_seed(&[2u8; 32]),
    ];
    let next = vec![
        SigningKey::from_seed(&[3u8; 32]),
        SigningKey::from_seed(&[4u8; 32]),
    ];
    let r = Controller::incept(current, 2, next, 2).unwrap();
    assert_eq!(
        r.incept_pairwise_pseudonym(b"context").unwrap_err(),
        IdentityError::PairwiseRequiresSingleKey
    );
}

#[test]
fn empty_context_still_derives_deterministically() {
    let r = root(1);
    let a = r.incept_pairwise_pseudonym(b"").unwrap();
    let b = r.incept_pairwise_pseudonym(b"").unwrap();
    assert_eq!(a.did().as_str(), b.did().as_str());
    // But it's still a different identity than any non-empty context.
    let c = r.incept_pairwise_pseudonym(b"x").unwrap();
    assert_ne!(a.did().as_str(), c.did().as_str());
}

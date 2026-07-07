//! Integration tests for `did-mini`.
//!
//! Fully deterministic (fixed seeds), so they reproduce identically anywhere and
//! double as executable proof of the SPEC-01 invariants this crate enforces:
//! self-certifying identifiers, an unbroken signed KEL, and pre-rotation.

use did_mini::{Controller, Did, Kel};
use mini_crypto::SigningKey;

const CUR_A: [u8; 32] = [1u8; 32];
const NEXT_A: [u8; 32] = [2u8; 32];
const CUR_B: [u8; 32] = [3u8; 32];
const NEXT_B: [u8; 32] = [4u8; 32];
const ROT_A: [u8; 32] = [5u8; 32];

// ---- inception + offline verification (EPIC 1.1 acceptance) ----

#[test]
fn incept_and_verify_offline() {
    let ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let state = ctrl.kel().verify().unwrap();
    assert_eq!(state.sn, 0);
    assert_eq!(state.threshold, 1);
    assert_eq!(state.keys.len(), 1);
    // The resolved key is exactly the inception (current) key.
    let expected = SigningKey::from_seed(&CUR_A).verifying_key();
    assert_eq!(state.keys, vec![expected]);
}

#[test]
fn scid_is_deterministic_and_self_certifying() {
    // Same inception material -> same identifier, on every run and platform.
    let a1 = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let a2 = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    assert_eq!(a1.scid(), a2.scid());

    // Different material -> different identifier.
    let b = Controller::incept_single_from_seeds(&CUR_B, &NEXT_B).unwrap();
    assert_ne!(a1.scid(), b.scid());

    // The identifier is multibase base58btc (leading 'z').
    assert!(a1.did().as_str().starts_with("did:mini:z"));
}

#[test]
fn wire_roundtrip_verifies_on_a_second_device() {
    // Device 1 builds an identity and serialises the public KEL.
    let device1 = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let blob = device1.kel().to_bytes();

    // Device 2 has only the bytes — no shared state — and verifies authenticity.
    let kel = Kel::from_bytes(&blob).unwrap();
    let state = kel.verify().unwrap();
    assert_eq!(kel.scid(), device1.scid());
    assert_eq!(state, device1.key_state());
}

// ---- did:mini identifier parsing ----

#[test]
fn did_parse_roundtrip() {
    let ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let s = ctrl.did().as_str().to_string();
    let parsed = Did::parse(&s).unwrap();
    assert_eq!(parsed.as_str(), s);
    assert_eq!(parsed.scid(), ctrl.scid());
}

#[test]
fn did_from_scid_rejects_invalid_scid() {
    assert!(Did::from_scid("not-a-multibase-multihash").is_err());
}

#[test]
fn did_parse_rejects_non_mini() {
    assert!(Did::parse("did:web:example.com").is_err());
    assert!(Did::parse("did:mini:").is_err()); // empty scid
    assert!(Did::parse("not-a-did").is_err());
}

// ---- rotation + pre-rotation (SPEC-01 §5) ----

#[test]
fn rotation_reveals_precommitted_keys_and_verifies() {
    let mut ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let scid_before = ctrl.scid().to_string();

    ctrl.rotate_with_next(vec![SigningKey::from_seed(&ROT_A)])
        .unwrap();

    let kel = ctrl.kel();
    let state = kel.verify().unwrap();
    assert_eq!(state.sn, 1);
    // The identifier persists across rotation (the whole point of SPEC-01 G2).
    assert_eq!(ctrl.scid(), scid_before);
    // The current key is now the previously pre-committed "next" key (NEXT_A).
    let revealed = SigningKey::from_seed(&NEXT_A).verifying_key();
    assert_eq!(state.keys, vec![revealed]);
}

#[test]
fn interaction_event_anchors_and_verifies() {
    let mut ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    ctrl.interact(vec![[9u8; 32]]).unwrap();
    let state = ctrl.kel().verify().unwrap();
    assert_eq!(state.sn, 1);
    // Control is unchanged by an interaction event.
    let unchanged = SigningKey::from_seed(&CUR_A).verifying_key();
    assert_eq!(state.keys, vec![unchanged]);
}

// ---- tamper rejection (the chain is verified, not trusted) ----

/// A corrupted blob must never verify: either decoding rejects it, or the
/// verification walk does.
fn never_verifies(blob: &[u8]) -> bool {
    Kel::from_bytes(blob)
        .ok()
        .and_then(|k| k.verify().ok())
        .is_none()
}

#[test]
fn tampered_signature_is_rejected() {
    let ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let mut blob = ctrl.kel().to_bytes();
    let last = blob.len() - 1; // final byte is the last signature byte
    blob[last] ^= 0xff;
    assert!(never_verifies(&blob));
}

#[test]
fn tampered_identifier_is_rejected() {
    let ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let mut blob = ctrl.kel().to_bytes();
    // Offset 10 falls inside the log's scid string; corrupting it breaks the
    // self-certification (or the UTF-8), so the identity is not authentic.
    blob[10] ^= 0xff;
    assert!(never_verifies(&blob));
}

#[test]
fn trailing_bytes_are_rejected() {
    let ctrl = Controller::incept_single_from_seeds(&CUR_A, &NEXT_A).unwrap();
    let mut blob = ctrl.kel().to_bytes();
    blob.push(0x00);
    assert!(Kel::from_bytes(&blob).is_err());
}

#[test]
fn did_parse_rejects_non_canonical_scid() {
    assert!(Did::parse("did:mini:not-a-multibase-multihash").is_err());
}

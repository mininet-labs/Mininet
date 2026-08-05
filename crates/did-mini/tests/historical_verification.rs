//! Historical key-state verification: durable signed objects must survive
//! their signer's ordinary key rotation.
//!
//! `Kel::verify_message` answers "is this signer authorized *now*", which is
//! right for live payloads and wrong for evidence meant to outlive a rotation.
//! These tests pin the behaviour long-lived objects (storage claims, audit
//! attestations, receipts) depend on, and pin the honest limits too: history
//! is not a timestamp, and a broken tail invalidates the whole log including
//! its past.

use did_mini::{Controller, IdentityError, Kel};

fn rotating_signer(seed: u8) -> Controller {
    Controller::incept_single_from_seeds(&[seed; 32], &[seed.wrapping_add(1); 32]).unwrap()
}

#[test]
fn a_message_signed_before_a_rotation_still_verifies_at_its_own_sequence() {
    let mut signer = rotating_signer(11);
    let signed_at = signer.kel().verify().unwrap().sn;
    let message = b"durable evidence signed before any rotation";
    let signature = signer.sign_message(message);

    // Live verification succeeds before the rotation.
    assert!(signer.kel().verify_message(message, &signature).is_ok());

    signer.rotate().unwrap();
    signer.rotate().unwrap();
    let kel = signer.kel();

    // Live verification now fails: those keys are no longer authoritative.
    assert!(matches!(
        kel.verify_message(message, &signature),
        Err(IdentityError::SignatureThresholdNotMet { .. })
    ));

    // Historical verification against the state that was authoritative when
    // the message was signed still succeeds -- which is the whole point.
    assert!(kel
        .verify_message_at(signed_at, message, &signature)
        .is_ok());
}

#[test]
fn historical_verification_does_not_accept_the_wrong_era() {
    let mut signer = rotating_signer(12);
    let message = b"signed at sn 0";
    let signature = signer.sign_message(message);
    signer.rotate().unwrap();
    let kel = signer.kel();

    // Verified at the sequence it was actually signed under: fine.
    assert!(kel.verify_message_at(0, message, &signature).is_ok());
    // Claimed to be from after the rotation: rejected, because the keys that
    // signed it were not authoritative then.
    assert!(matches!(
        kel.verify_message_at(1, message, &signature),
        Err(IdentityError::SignatureThresholdNotMet { .. })
    ));
}

#[test]
fn a_sequence_beyond_the_head_is_rejected_rather_than_clamped() {
    let signer = rotating_signer(13);
    let kel = signer.kel();
    assert_eq!(
        kel.key_state_at(1),
        Err(IdentityError::UnknownSequence { sn: 1, head: 0 })
    );
    assert_eq!(
        kel.event_digest_at(7),
        Err(IdentityError::UnknownSequence { sn: 7, head: 0 })
    );
}

#[test]
fn key_state_at_returns_the_state_that_event_established() {
    let mut signer = rotating_signer(14);
    let at_zero = signer.kel().verify().unwrap();
    signer.rotate().unwrap();
    let kel = signer.kel();

    let historical = kel.key_state_at(0).unwrap();
    assert_eq!(historical.sn, 0);
    assert_eq!(historical.keys, at_zero.keys);

    let current = kel.key_state_at(1).unwrap();
    assert_eq!(current.sn, 1);
    assert_ne!(current.keys, at_zero.keys);
    assert_eq!(current, kel.verify().unwrap());
}

#[test]
fn a_tampered_tail_invalidates_historical_lookups_too() {
    // A historical state is only meaningful if it comes from a log that is
    // consistent all the way to its head: otherwise an attacker could truncate
    // or corrupt everything after a compromised key and still "prove" state
    // from before it.
    let mut signer = rotating_signer(15);
    let message = b"signed at sn 0";
    let signature = signer.sign_message(message);
    signer.rotate().unwrap();

    // Corrupt the chain link itself: event 1's `prior` field is a copy of
    // event 0's digest, so flipping a byte of that copy inside the serialized
    // log breaks the chain without touching anything else.
    let kel = signer.kel();
    let digest = kel.event_digest_at(0).unwrap();
    let mut bytes = kel.to_bytes();
    let link = bytes
        .windows(digest.len())
        .rposition(|window| window == digest.as_slice())
        .expect("event 1 carries event 0's digest as its prior link");
    bytes[link] ^= 0xFF;
    let broken = Kel::from_bytes(&bytes).unwrap();

    assert!(matches!(
        broken.key_state_at(0),
        Err(IdentityError::BrokenChain { sn: 1 })
    ));
    assert!(broken.verify_message_at(0, message, &signature).is_err());
}

#[test]
fn event_digests_pin_a_specific_point_of_history() {
    let mut signer = rotating_signer(16);
    let kel_at_zero = signer.kel();
    let digest_at_zero = kel_at_zero.event_digest_at(0).unwrap();
    assert_eq!(kel_at_zero.head_digest().unwrap(), digest_at_zero);

    signer.rotate().unwrap();
    let kel = signer.kel();

    // Appending history never changes an already-cited digest.
    assert_eq!(kel.event_digest_at(0).unwrap(), digest_at_zero);
    // The head has moved on, and the two are distinguishable.
    assert_ne!(kel.head_digest().unwrap(), digest_at_zero);
    assert_eq!(kel.head_digest().unwrap(), kel.event_digest_at(1).unwrap());
}

//! `Controller::restore` — resuming a live session exactly where it left
//! off (e.g. after an app process restart), as opposed to
//! `Controller::recover_from_kel`'s deliberate lost/stolen/compromised-device
//! rotation (see `tests/recovery.rs`).
//!
//! Restore must reconstruct a controller that is functionally identical to
//! the one that was saved -- same DID, same sequence number, same ability to
//! sign and to rotate normally afterward -- and it must never silently
//! accept secret material that doesn't actually belong to the persisted KEL.

use did_mini::{Controller, IdentityError, Kel};
use mini_crypto::SigningKey;

const CUR: [u8; 32] = [0xB1; 32];
const NXT: [u8; 32] = [0xB2; 32];
const DELEGATOR_CUR: [u8; 32] = [0xD1; 32];
const DELEGATOR_NXT: [u8; 32] = [0xD2; 32];
const DEVICE_CUR: [u8; 32] = [0xD3; 32];
const DEVICE_NXT: [u8; 32] = [0xD4; 32];
const WRONG: [u8; 32] = [0xFF; 32];

/// The happy path: save the KEL bytes and the seed bytes, drop the original
/// controller entirely (simulating process death), and restore from just
/// those bytes -- the same "public KEL + secret seeds" shape a persistence
/// layer would actually store.
#[test]
fn restoring_reconstructs_a_fully_functional_controller() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let did_before = original.did();
    let sn_before = original.kel().len();
    let kel_bytes = original.kel().to_bytes();
    drop(original);

    let kel = Kel::from_bytes(&kel_bytes).unwrap();
    let current = vec![SigningKey::from_seed(&CUR)];
    let next = vec![SigningKey::from_seed(&NXT)];
    let mut restored = Controller::restore(&kel, current, next).unwrap();

    assert_eq!(restored.did(), did_before);
    assert_eq!(restored.kel().len(), sn_before);

    // Fully functional, not read-only: a normal rotation works afterward.
    restored.rotate().unwrap();
    assert_eq!(restored.kel().len(), sn_before + 1);
    restored.kel().verify().unwrap();
}

/// Restore performs no rotation and appends no event -- the KEL is
/// byte-for-byte identical before and after, unlike `recover_from_kel`
/// which always appends a rotation.
#[test]
fn restore_appends_no_event() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let kel_bytes_before = original.kel().to_bytes();
    drop(original);

    let kel = Kel::from_bytes(&kel_bytes_before).unwrap();
    let current = vec![SigningKey::from_seed(&CUR)];
    let next = vec![SigningKey::from_seed(&NXT)];
    let restored = Controller::restore(&kel, current, next).unwrap();

    assert_eq!(restored.kel().to_bytes(), kel_bytes_before);
}

/// A device (delegated) identity restores with its delegator intact.
#[test]
fn a_delegated_device_restores_with_its_delegator() {
    let root = Controller::incept_single_from_seeds(&DELEGATOR_CUR, &DELEGATOR_NXT).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &DEVICE_CUR, &DEVICE_NXT).unwrap();
    let device_did_before = device.did();
    let kel_bytes = device.kel().to_bytes();
    drop(device);

    let kel = Kel::from_bytes(&kel_bytes).unwrap();
    let current = vec![SigningKey::from_seed(&DEVICE_CUR)];
    let next = vec![SigningKey::from_seed(&DEVICE_NXT)];
    let restored = Controller::restore(&kel, current, next).unwrap();

    assert_eq!(restored.did(), device_did_before);
    assert_eq!(restored.kel().delegator(), Some(root.did()));
}

/// Wrong current-key seed bytes are rejected rather than silently producing
/// a controller that can't actually sign as this identity.
#[test]
fn restore_rejects_a_current_key_that_does_not_match_the_kel() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let kel = Kel::from_bytes(&original.kel().to_bytes()).unwrap();
    drop(original);

    let wrong_current = vec![SigningKey::from_seed(&WRONG)];
    let next = vec![SigningKey::from_seed(&NXT)];
    assert_eq!(
        Controller::restore(&kel, wrong_current, next).unwrap_err(),
        IdentityError::RestoreKeysMismatch
    );
}

/// Wrong next-key seed bytes (don't hash to the standing pre-rotation
/// commitment) are rejected the same way.
#[test]
fn restore_rejects_a_next_key_that_does_not_match_the_commitment() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let kel = Kel::from_bytes(&original.kel().to_bytes()).unwrap();
    drop(original);

    let current = vec![SigningKey::from_seed(&CUR)];
    let wrong_next = vec![SigningKey::from_seed(&WRONG)];
    assert_eq!(
        Controller::restore(&kel, current, wrong_next).unwrap_err(),
        IdentityError::RestoreKeysMismatch
    );
}

/// Empty key sets are rejected outright, same as every other constructor.
#[test]
fn restore_rejects_empty_key_sets() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let kel = Kel::from_bytes(&original.kel().to_bytes()).unwrap();
    drop(original);

    assert_eq!(
        Controller::restore(&kel, vec![], vec![SigningKey::from_seed(&NXT)]).unwrap_err(),
        IdentityError::EmptyKeySet
    );
    assert_eq!(
        Controller::restore(&kel, vec![SigningKey::from_seed(&CUR)], vec![]).unwrap_err(),
        IdentityError::EmptyKeySet
    );
}

/// A tampered KEL (broken chain) fails at the `kel.verify()` step inside
/// `restore`, before any key comparison even happens.
#[test]
fn restore_rejects_a_tampered_kel() {
    let mut original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    original.interact(vec![[1u8; 32]]).unwrap();
    let mut kel_bytes = original.kel().to_bytes();
    drop(original);
    // Flip a byte well inside the encoded log to break the chain digest.
    let flip_at = kel_bytes.len() / 2;
    kel_bytes[flip_at] ^= 0xFF;

    // Either decoding itself fails, or it decodes but fails verify() inside
    // restore -- both are acceptable rejections, never a silent success.
    match Kel::from_bytes(&kel_bytes) {
        Err(_) => {}
        Ok(kel) => {
            let current = vec![SigningKey::from_seed(&CUR)];
            let next = vec![SigningKey::from_seed(&NXT)];
            assert!(Controller::restore(&kel, current, next).is_err());
        }
    }
}

/// `export_current_and_next_keys_for_storage` round-trips through
/// `restore`: a persistence layer that saves exactly the KEL bytes plus the
/// exported seeds, then rebuilds via `SigningKey::to_seed_bytes`, ends up
/// with a fully functional controller again -- this is the exact shape
/// `mini-ffi`'s `RootCore::persist_state`/`RootCore::restore` (issue #198)
/// use.
#[test]
fn exported_keys_round_trip_through_restore() {
    let original = Controller::incept_single_from_seeds(&CUR, &NXT).unwrap();
    let did_before = original.did();
    let kel_bytes = original.kel().to_bytes();
    let (current_keys, next_keys) = original.export_current_and_next_keys_for_storage();
    let current_seeds: Vec<[u8; 32]> = current_keys.iter().map(|k| k.to_seed_bytes()).collect();
    let next_seeds: Vec<[u8; 32]> = next_keys.iter().map(|k| k.to_seed_bytes()).collect();
    drop(original);

    let kel = Kel::from_bytes(&kel_bytes).unwrap();
    let current = current_seeds.iter().map(SigningKey::from_seed).collect();
    let next = next_seeds.iter().map(SigningKey::from_seed).collect();
    let mut restored = Controller::restore(&kel, current, next).unwrap();

    assert_eq!(restored.did(), did_before);
    restored.rotate().unwrap();
    restored.kel().verify().unwrap();
}

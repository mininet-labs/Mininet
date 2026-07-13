//! Identity recovery edge cases (roadmap issue #13).
//!
//! Pre-rotation used as designed: the *next* keys are committed but unrevealed,
//! so their seeds can be escrowed off-device (paper backup, safe, heir's
//! envelope). Losing the device loses the current keys — recovery is revealing
//! the escrowed next keys as the new current set via an ordinary rotation.
//!
//! These tests pin down what recovery can do, what it must refuse, and — just
//! as deliberately — what it cannot do (stale-KEL acceptance), so the known
//! freshness gap is a test-documented fact rather than folklore.

use did_mini::{verify_delegation, Capabilities, Controller, FreshnessPins, IdentityError, Kel};
use mini_crypto::SigningKey;

const CUR_A: [u8; 32] = [0xA1; 32];
const NXT_A: [u8; 32] = [0xA2; 32];
const NXT_B: [u8; 32] = [0xA3; 32];
const DEV_C: [u8; 32] = [0xC1; 32];
const DEV_N: [u8; 32] = [0xC2; 32];
const EVIL: [u8; 32] = [0xEE; 32];

/// The happy path: device lost, identity recovered from the public KEL plus
/// the escrowed next-key seed. The DID is unchanged, the recovered controller
/// holds control, and the whole log still verifies offline.
#[test]
fn lost_device_recovers_from_kel_and_escrowed_seed() {
    // The device holds current + next; the *seed* of next is also on paper.
    let mut alice = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    alice.interact(vec![[7u8; 32]]).unwrap();
    let did_before = alice.did();

    // The device is lost. All that survives: the public KEL (any peer has it)
    // and the paper seed for the committed next key.
    let public_kel = Kel::from_bytes(&alice.kel().to_bytes()).unwrap();
    drop(alice);

    let escrowed = vec![SigningKey::from_seed(&NXT_A)];
    let fresh_next = vec![SigningKey::from_seed(&NXT_B)];
    let recovered = Controller::recover_from_kel(&public_kel, escrowed, fresh_next, 1).unwrap();

    // Same identity, control regained, and the log verifies end-to-end.
    assert_eq!(recovered.did(), did_before);
    let state = recovered.kel().verify().unwrap();
    assert_eq!(state.sn, 2); // icp, ixn, recovery rot
    assert_eq!(state.keys, recovered.key_state().keys);
}

/// A recovered identity keeps working: it can rotate again, sign, and delegate
/// — recovery is a rotation, not a special half-alive state.
#[test]
fn recovered_identity_is_fully_operational() {
    let alice = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let public_kel = alice.kel();
    drop(alice);

    let mut recovered = Controller::recover_from_kel(
        &public_kel,
        vec![SigningKey::from_seed(&NXT_A)],
        vec![SigningKey::from_seed(&NXT_B)],
        1,
    )
    .unwrap();

    // Delegate a replacement device under the recovered root.
    let device =
        Controller::incept_device_single_from_seeds(&recovered.did(), &DEV_C, &DEV_N).unwrap();
    recovered
        .delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    let caps = verify_delegation(&recovered.kel(), &device.kel()).unwrap();
    assert!(caps.contains(Capabilities::SIGN));

    // And rotate onward normally.
    recovered.rotate().unwrap();
    recovered.kel().verify().unwrap();
}

/// Wrong escrow keys are rejected before any event is emitted: an attacker
/// who merely *claims* "I lost my device" cannot hijack the identity without
/// holding the committed next keys. This is the "recovery abuse" case.
#[test]
fn recovery_with_wrong_keys_is_rejected() {
    let alice = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let public_kel = alice.kel();

    let err = Controller::recover_from_kel(
        &public_kel,
        vec![SigningKey::from_seed(&EVIL)],
        vec![SigningKey::from_seed(&NXT_B)],
        1,
    )
    .unwrap_err();
    assert_eq!(err, IdentityError::RecoveryKeysMismatch);

    // Wrong cardinality is the same refusal, not a panic or a partial match.
    let err = Controller::recover_from_kel(
        &public_kel,
        vec![SigningKey::from_seed(&NXT_A), SigningKey::from_seed(&EVIL)],
        vec![SigningKey::from_seed(&NXT_B)],
        1,
    )
    .unwrap_err();
    assert_eq!(err, IdentityError::RecoveryKeysMismatch);
}

/// After recovery, the lost/stolen device's keys are dead: events signed by
/// the old current keys no longer extend the canonical KEL, because control
/// moved to the revealed set. (A thief with the old device can still *fork*
/// the pre-recovery log for anyone who never sees the recovery — that
/// freshness gap is documented by the stale-KEL test below and owned by the
/// witness batch, M3.)
#[test]
fn old_device_keys_are_dead_after_recovery() {
    let mut alice = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let public_kel = alice.kel();

    let recovered = Controller::recover_from_kel(
        &public_kel,
        vec![SigningKey::from_seed(&NXT_A)],
        vec![SigningKey::from_seed(&NXT_B)],
        1,
    )
    .unwrap();
    let canonical = recovered.kel();
    let head_state = canonical.verify().unwrap();

    // The thief's device continues signing with the *old* current key. Its
    // extended log is a fork: same sn-1 prefix, but its sn-1 event differs
    // from the canonical recovery rotation, so any verifier holding the
    // canonical log sees a chain break, and the thief's detached signatures
    // fail against the canonical (post-recovery) key state.
    alice.interact(vec![[9u8; 32]]).unwrap();
    let sigs = alice.sign_message(b"drain the accounts");
    let verdict = {
        // verify_message checks against the canonical current state.
        canonical.verify_message(b"drain the accounts", &sigs)
    };
    assert!(verdict.is_err());
    assert_eq!(head_state.threshold, 1);
}

/// REGRESSION (issue #12 finding): rotation used to hardcode the next-set
/// threshold to N-of-N, silently rewriting an identity's M-of-N policy on its
/// first rotation and bricking future rotations if any one next key was lost.
/// The policy must survive rotations unchanged unless explicitly changed.
#[test]
fn rotation_preserves_threshold_policy() {
    let current: Vec<SigningKey> = (0u8..3).map(|i| SigningKey::from_seed(&[i; 32])).collect();
    let next: Vec<SigningKey> = (10u8..13)
        .map(|i| SigningKey::from_seed(&[i; 32]))
        .collect();
    let mut root = Controller::incept(current, 2, next, 2).unwrap();

    // First rotation: reveals the 2-of-3 next set...
    root.rotate().unwrap();
    let state = root.key_state();
    assert_eq!(state.threshold, 2);
    // ...and the *standing commitment* must still be 2-of-3, not 3-of-3.
    assert_eq!(state.next_threshold, 2);
    assert_eq!(state.next_commitments.len(), 3);

    // Second rotation still works and still preserves the policy.
    root.rotate().unwrap();
    assert_eq!(root.key_state().threshold, 2);
    assert_eq!(root.key_state().next_threshold, 2);
    root.kel().verify().unwrap();

    // Changing the policy is possible, but only as a deliberate act.
    let new_next: Vec<SigningKey> = (20u8..22)
        .map(|i| SigningKey::from_seed(&[i; 32]))
        .collect();
    root.rotate_with_next_and_threshold(new_next, 1).unwrap();
    assert_eq!(root.key_state().next_threshold, 1);
    root.kel().verify().unwrap();
}

/// HARDENING (issue #12 finding): a delegated identity cannot act as a
/// delegator. Every "root" handed to verify_delegation must be a true root,
/// so no caller counting identity roots can be fed a device posing as one.
#[test]
fn delegated_identity_cannot_delegate_sub_devices() {
    let mut root = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let mut device =
        Controller::incept_device_single_from_seeds(&root.did(), &DEV_C, &DEV_N).unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();

    // The device now tries to act as a root for a sub-device.
    let sub = Controller::incept_device_single_from_seeds(&device.did(), &[0xD1; 32], &[0xD2; 32])
        .unwrap();
    device
        .delegate_device(&sub.did(), Capabilities::secondary())
        .unwrap();

    let err = verify_delegation(&device.kel(), &sub.kel()).unwrap_err();
    assert_eq!(err, IdentityError::RootIsDelegated);
}

/// DOCUMENTED LIMITATION (issue #13): revocation is only as strong as KEL
/// freshness. A verifier holding a stale copy of the root's KEL — from before
/// a revocation — still accepts the revoked device. This test pins the gap so
/// it is a stated fact with a named owner (witness receipts, SPEC-01 §7 / M3;
/// until then every caller must fetch the freshest root KEL it can and never
/// accept a lower sn than it has already seen for a SCID).
#[test]
fn stale_root_kel_still_accepts_revoked_device_the_known_freshness_gap() {
    let mut root = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let device = Controller::incept_device_single_from_seeds(&root.did(), &DEV_C, &DEV_N).unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();

    let stale_root_kel = root.kel(); // snapshot BEFORE revocation
    root.revoke_device(&device.did()).unwrap();
    let fresh_root_kel = root.kel();

    // Fresh KEL: revoked, correctly refused.
    assert!(verify_delegation(&fresh_root_kel, &device.kel()).is_err());
    // Stale KEL: still accepted by `verify_delegation` alone — THIS IS THE
    // GAP. If this assertion ever fails, the gap has been closed at this
    // layer and this test, the one below, and the audit doc must all be
    // updated to say so.
    assert!(verify_delegation(&stale_root_kel, &device.kel()).is_ok());
    // The defense available today: sn monotonicity. The fresh log is strictly
    // longer; a verifier that pins the highest sn seen per SCID refuses the
    // stale one. See `freshness_pins_close_this_exact_gap_for_a_verifier_that_has_seen_the_fresh_kel`
    // below for that defense actually exercised end to end.
    assert!(fresh_root_kel.verify().unwrap().sn > stale_root_kel.verify().unwrap().sn);
}

/// The mitigation the previous test's comment promises, exercised for real:
/// a verifier using `FreshnessPins` (not `verify_delegation` alone) on this
/// exact revoked-device scenario refuses the stale root KEL, closing the gap
/// -- for a verifier that has already seen the fresher log. A verifier who
/// has *never* seen the fresh KEL has nothing to pin against; that residual
/// case is exactly what real witness receipts (SPEC-01 §7, still unbuilt)
/// are for, not something this interim rule claims to solve.
#[test]
fn freshness_pins_close_this_exact_gap_for_a_verifier_that_has_seen_the_fresh_kel() {
    let mut root = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let device = Controller::incept_device_single_from_seeds(&root.did(), &DEV_C, &DEV_N).unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();

    let stale_root_kel = root.kel();
    root.revoke_device(&device.did()).unwrap();
    let fresh_root_kel = root.kel();

    let mut pins = FreshnessPins::new();
    // The verifier has seen the fresh (post-revocation) KEL at some point...
    pins.check_and_pin(&fresh_root_kel).unwrap();
    // ...so a later attempt to hand it the old, pre-revocation snapshot is
    // rejected before `verify_delegation` would ever get the chance to
    // (wrongly) accept it.
    assert!(pins.check_and_pin(&stale_root_kel).is_err());
}

/// Total loss is total: without the committed next keys, nothing recovers the
/// identity — by design, since any backdoor that could would also be a theft
/// path. (The "death without escrow" case: the identity is permanently
/// orphaned; heirs holding the escrowed seed are exactly the recovery path.)
#[test]
fn nothing_recovers_an_identity_without_the_committed_keys() {
    let alice = Controller::incept_single_from_seeds(&CUR_A, &NXT_A).unwrap();
    let public_kel = alice.kel();

    for candidate in [[0x00u8; 32], [0x42; 32], [0xFF; 32]] {
        assert_eq!(
            Controller::recover_from_kel(
                &public_kel,
                vec![SigningKey::from_seed(&candidate)],
                vec![SigningKey::from_seed(&NXT_B)],
                1,
            )
            .unwrap_err(),
            IdentityError::RecoveryKeysMismatch
        );
    }
}

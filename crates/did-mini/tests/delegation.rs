//! Integration tests for device delegation (SPEC-01 §6).
//!
//! Deterministic and offline. They prove the "many devices, one human" property:
//! a human-root authorizes device identifiers with capabilities, the link is
//! mutual (neither side can fake it), and revocation removes a device.

use did_mini::{verify_delegation, Capabilities, Controller, Did};

// Distinct seeds for root + two devices (current/next per identity).
const ROOT_C: [u8; 32] = [10u8; 32];
const ROOT_N: [u8; 32] = [11u8; 32];
const A_C: [u8; 32] = [20u8; 32];
const A_N: [u8; 32] = [21u8; 32];
const B_C: [u8; 32] = [30u8; 32];
const B_N: [u8; 32] = [31u8; 32];
const X_C: [u8; 32] = [40u8; 32];
const X_N: [u8; 32] = [41u8; 32];

fn root() -> Controller {
    Controller::incept_single_from_seeds(&ROOT_C, &ROOT_N).unwrap()
}

fn device(delegator: &Did, c: &[u8; 32], n: &[u8; 32]) -> Controller {
    Controller::incept_device_single_from_seeds(delegator, c, n).unwrap()
}

#[test]
fn device_inception_records_its_delegator() {
    let root = root();
    let dev = device(&root.did(), &A_C, &A_N);
    // The device KEL self-certifies and names its delegator.
    let kel = dev.kel();
    assert!(kel.verify().is_ok());
    assert_eq!(kel.delegator().unwrap().as_str(), root.did().as_str());
    // A non-delegated identity has no delegator.
    assert!(root.kel().delegator().is_none());
}

#[test]
fn two_devices_one_human_with_capabilities() {
    let mut root = root();
    let phone = device(&root.did(), &A_C, &A_N);
    let laptop = device(&root.did(), &B_C, &B_N);

    root.delegate_device(&phone.did(), Capabilities::primary()).unwrap();
    root.delegate_device(&laptop.did(), Capabilities::secondary()).unwrap();

    // Both devices verify as delegated, with the granted capabilities.
    let phone_caps = verify_delegation(&root.kel(), &phone.kel()).unwrap();
    let laptop_caps = verify_delegation(&root.kel(), &laptop.kel()).unwrap();
    assert_eq!(phone_caps, Capabilities::primary());
    assert_eq!(laptop_caps, Capabilities::secondary());

    // The primary may vote (cast the human's single vote); the secondary may not.
    assert!(phone_caps.contains(Capabilities::VOTE));
    assert!(!laptop_caps.contains(Capabilities::VOTE));

    // The root lists exactly these two devices.
    let devices = root.kel().delegated_devices();
    assert_eq!(devices.len(), 2);
}

#[test]
fn revocation_removes_a_device() {
    let mut root = root();
    let phone = device(&root.did(), &A_C, &A_N);
    root.delegate_device(&phone.did(), Capabilities::primary()).unwrap();
    assert!(verify_delegation(&root.kel(), &phone.kel()).is_ok());

    root.revoke_device(&phone.did()).unwrap();
    assert!(verify_delegation(&root.kel(), &phone.kel()).is_err());
    assert!(root.kel().delegated_devices().is_empty());
}

#[test]
fn unauthorized_device_is_rejected() {
    // A device names the root as delegator, but the root never delegated it.
    let root = root();
    let imposter = device(&root.did(), &X_C, &X_N);
    assert!(verify_delegation(&root.kel(), &imposter.kel()).is_err());
}

#[test]
fn device_claiming_wrong_root_is_rejected() {
    // The root delegates a device that belongs to a *different* delegator string;
    // the mutual check fails because the device does not name this root.
    let mut root = root();
    let other_root = Controller::incept_single_from_seeds(&[99u8; 32], &[98u8; 32]).unwrap();
    let dev = device(&other_root.did(), &A_C, &A_N);

    // Even if this root tries to claim it, the device's dip names other_root.
    root.delegate_device(&dev.did(), Capabilities::primary()).unwrap();
    assert!(verify_delegation(&root.kel(), &dev.kel()).is_err());
    // It does verify against its real delegator once that root authorizes it.
    let mut other_root = other_root;
    other_root.delegate_device(&dev.did(), Capabilities::primary()).unwrap();
    assert!(verify_delegation(&other_root.kel(), &dev.kel()).is_ok());
}

#[test]
fn re_delegation_updates_capabilities() {
    let mut root = root();
    let phone = device(&root.did(), &A_C, &A_N);
    root.delegate_device(&phone.did(), Capabilities::secondary()).unwrap();
    assert_eq!(
        verify_delegation(&root.kel(), &phone.kel()).unwrap(),
        Capabilities::secondary()
    );
    // Re-delegating the same device upgrades it (last write wins).
    root.delegate_device(&phone.did(), Capabilities::primary()).unwrap();
    assert_eq!(
        verify_delegation(&root.kel(), &phone.kel()).unwrap(),
        Capabilities::primary()
    );
    assert_eq!(root.kel().delegated_devices().len(), 1);
}

#[test]
fn root_kel_with_seals_still_verifies() {
    // Seal events are non-establishment: they must not disturb the root's own key
    // state.
    let mut root = root();
    let before = root.kel().verify().unwrap();
    let phone = device(&root.did(), &A_C, &A_N);
    root.delegate_device(&phone.did(), Capabilities::primary()).unwrap();
    let after = root.kel().verify().unwrap();
    assert_eq!(before.keys, after.keys);
    assert_eq!(after.sn, before.sn + 1);
}

#[test]
fn capabilities_are_a_narrowing_bitset() {
    let p = Capabilities::primary();
    assert!(p.contains(Capabilities::SIGN));
    assert!(p.contains(Capabilities::PAY));
    assert!(!p.contains(Capabilities::MANAGE_DEVICES)); // never in a default
    let empty = Capabilities::empty();
    assert!(!empty.contains(Capabilities::SIGN));
    assert_eq!(
        Capabilities::SIGN.with(Capabilities::PAY).bits(),
        Capabilities::SIGN.bits() | Capabilities::PAY.bits()
    );
}


#[test]
fn unknown_capability_bits_are_rejected() {
    assert!(Capabilities::from_bits(Capabilities::SIGN.bits()).is_ok());
    assert!(Capabilities::from_bits(1 << 31).is_err());
}

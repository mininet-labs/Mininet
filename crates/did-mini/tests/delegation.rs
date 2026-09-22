//! Integration tests for device delegation (SPEC-01 §6).
//!
//! Deterministic and offline. They prove the "many devices, one human" property:
//! a human-root authorizes device identifiers with capabilities, the link is
//! mutual (neither side can fake it), and revocation removes a device.

use did_mini::{verify_delegation, Capabilities, Controller, DeviceTier, Did};

// Distinct seeds for root + two devices (current/next per identity).
const ROOT_C: [u8; 32] = [10u8; 32];
const ROOT_N: [u8; 32] = [11u8; 32];
const A_C: [u8; 32] = [20u8; 32];
const A_N: [u8; 32] = [21u8; 32];
const B_C: [u8; 32] = [30u8; 32];
const B_N: [u8; 32] = [31u8; 32];
const X_C: [u8; 32] = [40u8; 32];
const X_N: [u8; 32] = [41u8; 32];
const C_C: [u8; 32] = [50u8; 32];
const C_N: [u8; 32] = [51u8; 32];

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

    root.delegate_device(&phone.did(), Capabilities::primary())
        .unwrap();
    root.delegate_device(&laptop.did(), Capabilities::secondary())
        .unwrap();

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
    root.delegate_device(&phone.did(), Capabilities::primary())
        .unwrap();
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
    root.delegate_device(&dev.did(), Capabilities::primary())
        .unwrap();
    assert!(verify_delegation(&root.kel(), &dev.kel()).is_err());
    // It does verify against its real delegator once that root authorizes it.
    let mut other_root = other_root;
    other_root
        .delegate_device(&dev.did(), Capabilities::primary())
        .unwrap();
    assert!(verify_delegation(&other_root.kel(), &dev.kel()).is_ok());
}

#[test]
fn re_delegation_updates_capabilities() {
    let mut root = root();
    let phone = device(&root.did(), &A_C, &A_N);
    root.delegate_device(&phone.did(), Capabilities::secondary())
        .unwrap();
    assert_eq!(
        verify_delegation(&root.kel(), &phone.kel()).unwrap(),
        Capabilities::secondary()
    );
    // Re-delegating the same device upgrades it (last write wins).
    root.delegate_device(&phone.did(), Capabilities::primary())
        .unwrap();
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
    root.delegate_device(&phone.did(), Capabilities::primary())
        .unwrap();
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

#[test]
fn storing_on_a_roots_behalf_is_never_granted_by_a_default() {
    // A storage commitment exposes the root to durable, publishable conflict
    // evidence about its own conduct, so STORE has to be granted on purpose
    // per storage device -- never inherited from "this is my primary phone".
    assert!(!Capabilities::primary().contains(Capabilities::STORE));
    assert!(!Capabilities::secondary().contains(Capabilities::STORE));
    assert!(Capabilities::ALL.contains(Capabilities::STORE));
    assert!(Capabilities::from_bits(Capabilities::STORE.bits()).is_ok());

    let granted = Capabilities::secondary().with(Capabilities::STORE);
    assert!(granted.contains(Capabilities::STORE));
    assert!(!granted.contains(Capabilities::VOTE));
    assert!(!granted.contains(Capabilities::MANAGE_DEVICES));
}

// --- Device hierarchy tiers (issue #14, D-0530) ---

#[test]
fn cold_root_tier_has_full_authority() {
    let caps = Capabilities::for_tier(DeviceTier::ColdRoot);
    for bit in [
        Capabilities::SIGN,
        Capabilities::PAY,
        Capabilities::POST,
        Capabilities::ATTEST,
        Capabilities::VOTE,
        Capabilities::MANAGE_DEVICES,
        Capabilities::STORE,
    ] {
        assert!(caps.contains(bit));
    }
    assert!(caps.contains(Capabilities::MANAGE_DEVICES));
    assert!(caps.contains(Capabilities::VOTE));
    assert!(caps.contains(Capabilities::STORE));
}

#[test]
fn hardware_token_tier_is_signing_only() {
    let caps = Capabilities::for_tier(DeviceTier::HardwareToken);
    assert_eq!(caps, Capabilities::SIGN);
    assert!(!caps.contains(Capabilities::MANAGE_DEVICES));
    assert!(!caps.contains(Capabilities::VOTE));
    assert!(!caps.contains(Capabilities::PAY));
    assert!(!caps.contains(Capabilities::POST));
    assert!(!caps.contains(Capabilities::STORE));
}

#[test]
fn daily_device_tier_has_everyday_authority_but_no_key_management() {
    let caps = Capabilities::for_tier(DeviceTier::DailyDevice);
    assert_eq!(caps, Capabilities::primary());
    assert!(caps.contains(Capabilities::SIGN));
    assert!(caps.contains(Capabilities::VOTE));
    assert!(!caps.contains(Capabilities::MANAGE_DEVICES));
    assert!(!caps.contains(Capabilities::STORE));
}

#[test]
fn emerging_tier_is_conservative_by_default() {
    // Future device shapes (implant/wearable, Directive 13) start bounded
    // exactly like a secondary device: no vote, no device management.
    let caps = Capabilities::for_tier(DeviceTier::Emerging);
    assert_eq!(caps, Capabilities::secondary());
    assert!(!caps.contains(Capabilities::VOTE));
    assert!(!caps.contains(Capabilities::MANAGE_DEVICES));
    assert!(!caps.contains(Capabilities::STORE));
}

#[test]
fn every_tier_is_bounded_by_capabilities_all() {
    for tier in [
        DeviceTier::ColdRoot,
        DeviceTier::HardwareToken,
        DeviceTier::DailyDevice,
        DeviceTier::Emerging,
    ] {
        assert!(Capabilities::ALL.contains(Capabilities::for_tier(tier)));
    }
}

#[test]
fn delegate_device_tier_matches_capabilities_for_tier() {
    let mut root = root();
    let key = device(&root.did(), &A_C, &A_N);
    root.delegate_device_tier(&key.did(), DeviceTier::HardwareToken)
        .unwrap();
    let caps = verify_delegation(&root.kel(), &key.kel()).unwrap();
    assert_eq!(caps, Capabilities::for_tier(DeviceTier::HardwareToken));
}

#[test]
fn revoke_devices_except_keeps_the_named_devices_and_cuts_the_rest() {
    let mut root = root();
    let cold = device(&root.did(), &A_C, &A_N);
    let token = device(&root.did(), &B_C, &B_N);
    let phone = device(&root.did(), &C_C, &C_N);

    root.delegate_device_tier(&cold.did(), DeviceTier::ColdRoot)
        .unwrap();
    root.delegate_device_tier(&token.did(), DeviceTier::HardwareToken)
        .unwrap();
    root.delegate_device_tier(&phone.did(), DeviceTier::DailyDevice)
        .unwrap();
    assert_eq!(root.kel().delegated_devices().len(), 3);

    // "I lost my phone" -- keep the cold root and hardware token, cut the rest.
    root.revoke_devices_except(&[cold.did(), token.did()])
        .unwrap();

    assert!(verify_delegation(&root.kel(), &cold.kel()).is_ok());
    assert!(verify_delegation(&root.kel(), &token.kel()).is_ok());
    assert!(verify_delegation(&root.kel(), &phone.kel()).is_err());
    assert_eq!(root.kel().delegated_devices().len(), 2);
}

#[test]
fn revoke_devices_except_is_a_noop_when_nothing_needs_cutting() {
    let mut root = root();
    let phone = device(&root.did(), &A_C, &A_N);
    root.delegate_device_tier(&phone.did(), DeviceTier::DailyDevice)
        .unwrap();
    let sn_before = root.kel().verify().unwrap().sn;

    root.revoke_devices_except(&[phone.did()]).unwrap();

    // No seal event was appended -- the KEL's sequence number is unchanged.
    assert_eq!(root.kel().verify().unwrap().sn, sn_before);
    assert!(verify_delegation(&root.kel(), &phone.kel()).is_ok());
}

#[test]
fn revoke_all_devices_cuts_every_delegated_device() {
    let mut root = root();
    let cold = device(&root.did(), &A_C, &A_N);
    let phone = device(&root.did(), &B_C, &B_N);
    root.delegate_device_tier(&cold.did(), DeviceTier::ColdRoot)
        .unwrap();
    root.delegate_device_tier(&phone.did(), DeviceTier::DailyDevice)
        .unwrap();

    root.revoke_all_devices().unwrap();

    assert!(root.kel().delegated_devices().is_empty());
    assert!(verify_delegation(&root.kel(), &cold.kel()).is_err());
    assert!(verify_delegation(&root.kel(), &phone.kel()).is_err());
}

#[test]
fn cold_root_tier_is_an_enumerated_set_not_whatever_all_grows_into() {
    // Any capability bit added to `Capabilities::ALL` later must not
    // silently enter the ColdRoot tier: the tier is exactly these bits.
    let reviewed = Capabilities::SIGN
        .with(Capabilities::PAY)
        .with(Capabilities::POST)
        .with(Capabilities::ATTEST)
        .with(Capabilities::VOTE)
        .with(Capabilities::MANAGE_DEVICES)
        .with(Capabilities::STORE);
    assert_eq!(Capabilities::for_tier(DeviceTier::ColdRoot), reviewed);
}

#[test]
fn revoke_all_devices_splits_more_than_one_seal_events_worth() {
    // 130 delegated devices is more than one seal event may carry (128).
    let mut root = root();
    let mut devices = Vec::new();
    for i in 0..130u8 {
        let d = device(&root.did(), &[i; 32], &[i.wrapping_add(200); 32]);
        root.delegate_device_tier(&d.did(), DeviceTier::DailyDevice)
            .unwrap();
        devices.push(d);
    }
    assert_eq!(root.kel().delegated_devices().len(), 130);

    root.revoke_all_devices().unwrap();

    assert!(root.kel().delegated_devices().is_empty());
    for d in &devices {
        assert!(verify_delegation(&root.kel(), &d.kel()).is_err());
    }
}

//! The keystone demo, end to end over the in-process bearer: everything the
//! two-phone demo does except the physical radio-free bearer (BLE / local Wi-Fi),
//! which implements the same `Bearer` trait on real devices.

use did_mini::{Capabilities, Controller};
use mini_bearer::pair;
use mini_keystone::{run_demo, DemoError, Participant};
use mini_presence::TransportKind;

fn alice() -> Participant {
    Participant::from_seeds([1; 32], [2; 32], [3; 32], [4; 32]).unwrap()
}

fn bob() -> Participant {
    Participant::from_seeds([5; 32], [6; 32], [7; 32], [8; 32]).unwrap()
}

#[test]
fn keystone_demo_end_to_end() {
    let a = alice();
    let b = bob();
    let (mut bearer_a, mut bearer_b) = pair();

    let report = run_demo(
        &a,
        &b,
        &mut bearer_a,
        &mut bearer_b,
        TransportKind::InProcess,
        1_000_000,
    )
    .unwrap();

    // Two distinct identity roots met.
    assert_ne!(report.initiator_root, report.responder_root);
    assert_eq!(report.initiator_root, a.root.did().as_str());
    assert_eq!(report.responder_root, b.root.did().as_str());

    // Both accrued the same base value from one fresh encounter (P2-symmetric),
    // not yet vested (P4 maturation).
    assert_eq!(report.initiator_account.accrued_points, 1_000);
    assert_eq!(report.responder_account.accrued_points, 1_000);
    assert_eq!(report.initiator_account.vested_points, 0);
    assert_eq!(report.initiator_account.distinct_counterparties, 1);
}

#[test]
fn demo_is_deterministic_per_identity_set() {
    // Same participants, two runs: identities and accrual identical; only the
    // channel binding differs (fresh ephemeral keys each session).
    let a = alice();
    let b = bob();

    let (mut a1, mut b1) = pair();
    let r1 = run_demo(&a, &b, &mut a1, &mut b1, TransportKind::InProcess, 1_000_000).unwrap();
    let (mut a2, mut b2) = pair();
    let r2 = run_demo(&a, &b, &mut a2, &mut b2, TransportKind::InProcess, 1_000_000).unwrap();

    assert_eq!(r1.initiator_root, r2.initiator_root);
    assert_eq!(r1.initiator_account, r2.initiator_account);
    assert_ne!(r1.channel_binding, r2.channel_binding); // forward secrecy: fresh session
}

#[test]
fn demo_refuses_a_peer_without_attest() {
    // Bob's device is delegated WITHOUT the attest capability: the flow must
    // refuse at identity verification, before any presence is signed.
    let a = alice();
    let mut b_root = Controller::incept_single_from_seeds(&[5; 32], &[6; 32]).unwrap();
    let b_device =
        Controller::incept_device_single_from_seeds(&b_root.did(), &[7; 32], &[8; 32]).unwrap();
    b_root
        .delegate_device(&b_device.did(), Capabilities::secondary())
        .unwrap();
    let b = Participant {
        root: b_root,
        device: b_device,
    };

    let (mut bearer_a, mut bearer_b) = pair();
    let result = run_demo(
        &a,
        &b,
        &mut bearer_a,
        &mut bearer_b,
        TransportKind::InProcess,
        1_000_000,
    );
    assert!(matches!(result, Err(DemoError::Presence(_))));
}

//! Integration tests for vouch attestation and verification.
//!
//! Deterministic and offline, mirroring `mini-presence`'s test pattern:
//! two identity roots each delegate an `ATTEST`-capable device, both sign
//! one mutual vouch transcript, and the verifier accepts only a well-formed,
//! non-replayed vouch between genuinely delegated devices.

use did_mini::{Capabilities, Controller};
use mini_presence::TransportKind;
use mini_uniqueness::{
    verify_vouch, InMemoryReplayGuard, UniquenessError, VerifyContext, VouchAttestation,
    VouchFields, VoucherParty, VOUCH_VERSION,
};

fn human(
    root_c: [u8; 32],
    root_n: [u8; 32],
    dev_c: [u8; 32],
    dev_n: [u8; 32],
    caps: Capabilities,
) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&root_c, &root_n).unwrap();
    let device = Controller::incept_device_single_from_seeds(&root.did(), &dev_c, &dev_n).unwrap();
    root.delegate_device(&device.did(), caps).unwrap();
    (root, device)
}

fn valid_vouch(a_device: &Controller, b_device: &Controller) -> VouchAttestation {
    let fields = VouchFields {
        version: VOUCH_VERSION,
        channel_binding: [7u8; 32],
        transport: TransportKind::InProcess,
        a: VoucherParty {
            device: a_device.did(),
            kel_digest: mini_presence::kel_digest(&a_device.kel()),
            nonce: [1u8; 32],
        },
        b: VoucherParty {
            device: b_device.did(),
            kel_digest: mini_presence::kel_digest(&b_device.kel()),
            nonce: [2u8; 32],
        },
        asserted_at_ms: 1_000,
    };
    let a_sig = fields.sign(a_device);
    let b_sig = fields.sign(b_device);
    VouchAttestation::new(fields, a_sig, b_sig)
}

fn resign(att: VouchAttestation, a: &Controller, b: &Controller) -> VouchAttestation {
    let a_sig = att.fields.sign(a);
    let b_sig = att.fields.sign(b);
    VouchAttestation::new(att.fields, a_sig, b_sig)
}

#[test]
fn valid_vouch_names_both_identity_roots() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let att = valid_vouch(&a_dev, &b_dev);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let ctx = VerifyContext {
        a_root: &a_root_kel,
        b_root: &b_root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    let verdict = verify_vouch(&att, &ctx, &mut replay).unwrap();

    assert_eq!(verdict.a_root.as_str(), a_root.did().as_str());
    assert_eq!(verdict.b_root.as_str(), b_root.did().as_str());
    let (p, q) = verdict.pair_key();
    assert!(p <= q);

    // Re-verifying the same vouch is a replay.
    let mut replay2 = replay;
    assert_eq!(
        verify_vouch(&att, &ctx, &mut replay2),
        Err(UniquenessError::Replay)
    );
}

#[test]
fn device_without_attest_capability_is_rejected() {
    let (a_root, a_dev) = human(
        [1; 32],
        [2; 32],
        [3; 32],
        [4; 32],
        Capabilities::secondary(),
    );
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let att = valid_vouch(&a_dev, &b_dev);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let ctx = VerifyContext {
        a_root: &a_root_kel,
        b_root: &b_root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_vouch(&att, &ctx, &mut replay),
        Err(UniquenessError::MissingAttestCapability)
    );
}

#[test]
fn revoked_device_is_rejected() {
    let (mut a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let att = valid_vouch(&a_dev, &b_dev);

    a_root.revoke_device(&a_dev.did()).unwrap();

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let ctx = VerifyContext {
        a_root: &a_root_kel,
        b_root: &b_root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_vouch(&att, &ctx, &mut replay),
        Err(UniquenessError::Identity(_))
    ));
}

#[test]
fn tampered_transcript_breaks_signatures() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let mut att = valid_vouch(&a_dev, &b_dev);
    att.fields.asserted_at_ms = 9_999;

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let ctx = VerifyContext {
        a_root: &a_root_kel,
        b_root: &b_root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_vouch(&att, &ctx, &mut replay),
        Err(UniquenessError::Identity(_))
    ));
}

#[test]
fn self_vouch_between_own_devices_is_rejected() {
    let mut root = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
    let dev_a =
        Controller::incept_device_single_from_seeds(&root.did(), &[3; 32], &[4; 32]).unwrap();
    let dev_b =
        Controller::incept_device_single_from_seeds(&root.did(), &[5; 32], &[6; 32]).unwrap();
    root.delegate_device(&dev_a.did(), Capabilities::primary())
        .unwrap();
    root.delegate_device(&dev_b.did(), Capabilities::primary())
        .unwrap();
    let att = valid_vouch(&dev_a, &dev_b);

    let root_kel = root.kel();
    let (a_dev_kel, b_dev_kel) = (dev_a.kel(), dev_b.kel());
    let ctx = VerifyContext {
        a_root: &root_kel,
        b_root: &root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_vouch(&att, &ctx, &mut replay),
        Err(UniquenessError::SelfVouch)
    );
}

#[test]
fn matching_nonces_are_rejected_as_replay() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let mut att = valid_vouch(&a_dev, &b_dev);
    att.fields.b.nonce = att.fields.a.nonce;
    let att = resign(att, &a_dev, &b_dev);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let ctx = VerifyContext {
        a_root: &a_root_kel,
        b_root: &b_root_kel,
        a_device: &a_dev_kel,
        b_device: &b_dev_kel,
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_vouch(&att, &ctx, &mut replay),
        Err(UniquenessError::Replay)
    );
}

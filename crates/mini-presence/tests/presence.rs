//! Integration tests for presence attestation and verification.
//!
//! Deterministic and offline. Two identity roots each delegate an `ATTEST`-capable device,
//! the devices form an encrypted channel (real binding), both sign one presence
//! transcript, and the verifier accepts only a well-formed, range-bound,
//! non-replayed attestation between genuinely delegated devices.

use did_mini::{Capabilities, Controller};
use mini_bearer::{Initiator, Responder};
use mini_presence::{
    kel_digest, verify_presence, AttestationFields, InMemoryReplayGuard, Party, PresenceAttestation,
    PresenceError, RangePolicy, TransportKind, VerifyContext, PRESENCE_VERSION,
};

/// Build a identity root controller and one delegated device with `caps`.
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

/// A fresh channel binding from a real handshake.
fn fresh_binding() -> [u8; 32] {
    let (initiator, hello1) = Initiator::start().unwrap();
    let (responder_channel, hello2) = Responder::respond(&hello1).unwrap();
    let initiator_channel = initiator.finish(&hello2).unwrap();
    assert_eq!(initiator_channel.channel_binding(), responder_channel.channel_binding());
    initiator_channel.channel_binding()
}

fn policy() -> RangePolicy {
    RangePolicy::ble_default()
}

/// Build a valid, signed attestation between the two devices for `binding`.
fn valid_attestation(
    init_device: &Controller,
    resp_device: &Controller,
    binding: [u8; 32],
) -> PresenceAttestation {
    let fields = AttestationFields {
        version: PRESENCE_VERSION,
        channel_binding: binding,
        initiator: Party {
            device: init_device.did(),
            kel_digest: kel_digest(&init_device.kel()),
            nonce: [1u8; 32],
        },
        responder: Party {
            device: resp_device.did(),
            kel_digest: kel_digest(&resp_device.kel()),
            nonce: [2u8; 32],
        },
        started_at_ms: 1_000,
        finished_at_ms: 1_006,
        rtt_samples_ms: vec![10, 12, 9, 11],
        transport: TransportKind::InProcess,
        location_commitment: None,
    };
    let init_sig = fields.sign(init_device);
    let resp_sig = fields.sign(resp_device);
    PresenceAttestation::new(fields, init_sig, resp_sig)
}

#[test]
fn valid_presence_names_both_identity_roots() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: Some(2_000),
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    let verdict = verify_presence(&att, &ctx, &mut replay).unwrap();

    assert_eq!(verdict.initiator_root.as_str(), a_root.did().as_str());
    assert_eq!(verdict.responder_root.as_str(), b_root.did().as_str());
    // The pair key is order-independent.
    let (p, q) = verdict.pair_key();
    assert!(p <= q);

    // Re-verifying the same attestation is a replay (nonces already seen).
    let mut replay2 = replay;
    assert_eq!(
        verify_presence(&att, &ctx, &mut replay2),
        Err(PresenceError::Replay)
    );
}

#[test]
fn device_without_attest_capability_is_rejected() {
    // Secondary devices lack ATTEST.
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::secondary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::MissingAttestCapability)
    );
}

#[test]
fn revoked_device_is_rejected() {
    let (mut a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);

    // Revoke A's device after the fact.
    a_root.revoke_device(&a_dev.did()).unwrap();

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::Identity(_))
    ));
}

#[test]
fn tampered_transcript_breaks_signatures() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let mut att = valid_attestation(&a_dev, &b_dev, binding);

    // Change a signed field after signing (still a valid time window).
    att.fields.started_at_ms = 1_001;

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::Identity(_))
    ));
}

#[test]
fn non_proximity_and_range_failures_are_rejected() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };

    // Relay transport cannot evidence co-presence.
    let mut relay_att = valid_attestation(&a_dev, &b_dev, binding);
    relay_att.fields.transport = TransportKind::Relay;
    relay_att.fields.initiator.nonce = [21; 32];
    relay_att.fields.responder.nonce = [22; 32];
    let relay_att = resign(relay_att, &a_dev, &b_dev);
    let mut r1 = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&relay_att, &ctx, &mut r1),
        Err(PresenceError::NotProximityTransport)
    );

    // Round-trip too far.
    let mut far_att = valid_attestation(&a_dev, &b_dev, binding);
    far_att.fields.rtt_samples_ms = vec![200, 210, 205, 220];
    far_att.fields.initiator.nonce = [31; 32];
    far_att.fields.responder.nonce = [32; 32];
    let far_att = resign(far_att, &a_dev, &b_dev);
    let mut r2 = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&far_att, &ctx, &mut r2),
        Err(PresenceError::RangeExceeded)
    );

    // Too few samples.
    let mut few_att = valid_attestation(&a_dev, &b_dev, binding);
    few_att.fields.rtt_samples_ms = vec![10];
    few_att.fields.initiator.nonce = [41; 32];
    few_att.fields.responder.nonce = [42; 32];
    let few_att = resign(few_att, &a_dev, &b_dev);
    let mut r3 = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&few_att, &ctx, &mut r3),
        Err(PresenceError::NotEnoughRangeSamples)
    );
}

#[test]
fn wrong_channel_binding_is_rejected() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some([0xAA; 32]), // not this session
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::BindingMismatch)
    );
}

/// Re-sign an attestation after mutating its fields (test helper).
fn resign(att: PresenceAttestation, init: &Controller, resp: &Controller) -> PresenceAttestation {
    let init_sig = att.fields.sign(init);
    let resp_sig = att.fields.sign(resp);
    PresenceAttestation::new(att.fields, init_sig, resp_sig)
}

#[test]
fn self_presence_between_own_devices_is_rejected() {
    // One human, two of their own ATTEST-capable devices: verification must
    // refuse — a human cannot be co-present with themselves (P2).
    let mut root = Controller::incept_single_from_seeds(&[1; 32], &[2; 32]).unwrap();
    let dev_a = Controller::incept_device_single_from_seeds(&root.did(), &[3; 32], &[4; 32]).unwrap();
    let dev_b = Controller::incept_device_single_from_seeds(&root.did(), &[5; 32], &[6; 32]).unwrap();
    root.delegate_device(&dev_a.did(), Capabilities::primary()).unwrap();
    root.delegate_device(&dev_b.did(), Capabilities::primary()).unwrap();

    let binding = fresh_binding();
    let att = valid_attestation(&dev_a, &dev_b, binding);

    let root_kel = root.kel();
    let (a_kel, b_kel) = (dev_a.kel(), dev_b.kel());
    let policy = policy();
    let ctx = VerifyContext {
        initiator_root: &root_kel,
        responder_root: &root_kel,
        initiator_device: &a_kel,
        responder_device: &b_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::SelfPresence)
    );
}

#[test]
fn failed_verification_does_not_burn_nonces() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let policy = policy();
    let mut replay = InMemoryReplayGuard::new();

    // First attempt: verifier expects a DIFFERENT binding -> fails early, and the
    // failure must not consume the nonces.
    let wrong_ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some([0xAA; 32]),
    };
    assert_eq!(
        verify_presence(&att, &wrong_ctx, &mut replay),
        Err(PresenceError::BindingMismatch)
    );

    // Second attempt with the correct binding and the SAME replay guard: the
    // attestation must still verify, proving no replay state was mutated.
    let right_ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: None,
        expected_binding: Some(binding),
    };
    assert!(verify_presence(&att, &right_ctx, &mut replay).is_ok());
}

#[test]
fn attestations_older_than_max_age_are_refused() {
    let (a_root, a_dev) = human([1; 32], [2; 32], [3; 32], [4; 32], Capabilities::primary());
    let (b_root, b_dev) = human([5; 32], [6; 32], [7; 32], [8; 32], Capabilities::primary());
    let binding = fresh_binding();
    let att = valid_attestation(&a_dev, &b_dev, binding);
    let finished = att.fields.finished_at_ms;

    let (a_root_kel, b_root_kel) = (a_root.kel(), b_root.kel());
    let (a_dev_kel, b_dev_kel) = (a_dev.kel(), b_dev.kel());
    let mut policy = policy();
    policy.max_age_ms = 1_000;

    // Verifier clock far past the attestation: refused even though everything
    // else is valid — replay windows stay finite across restarts.
    let ctx = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: Some(finished + 1_001),
        expected_binding: Some(binding),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_presence(&att, &ctx, &mut replay),
        Err(PresenceError::BadTimeWindow)
    );

    // Within the window: verifies.
    let ctx_ok = VerifyContext {
        initiator_root: &a_root_kel,
        responder_root: &b_root_kel,
        initiator_device: &a_dev_kel,
        responder_device: &b_dev_kel,
        policy: &policy,
        now_ms: Some(finished + 999),
        expected_binding: Some(binding),
    };
    let mut replay2 = InMemoryReplayGuard::new();
    assert!(verify_presence(&att, &ctx_ok, &mut replay2).is_ok());
}

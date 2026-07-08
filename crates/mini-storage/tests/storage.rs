//! Integration tests for storage-served receipt verification.
//!
//! Deterministic and offline. Two identity roots each delegate an
//! `ATTEST`-capable device; both sign one receipt transcript; the verifier
//! accepts only a well-formed, non-replayed receipt between genuinely
//! delegated devices of two distinct identity roots.

use did_mini::{Capabilities, Controller};
use mini_crypto::HashAlgorithm;
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_storage::{
    verify_serve, FreshnessPolicy, InMemoryReplayGuard, ReceiptFields, ServeReceipt,
    StorageProofError, VerifyContext, RECEIPT_VERSION,
};

/// Build an identity root controller and one delegated device with `caps`.
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

/// A real content-addressed id to serve, cheaply, via mini-objects directly.
fn content_id() -> ObjectId {
    let (root, device) = human(
        [1u8; 32],
        [2u8; 32],
        [3u8; 32],
        [4u8; 32],
        Capabilities::primary(),
    );
    let obj = ObjectBuilder::new(ObjectType::POST)
        .payload(Payload::Public(b"served content".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    obj.id().clone()
}

fn policy() -> FreshnessPolicy {
    FreshnessPolicy::default_policy()
}

fn valid_receipt(
    host_device: &Controller,
    witness_device: &Controller,
    content: &ObjectId,
    host_nonce: [u8; 32],
    witness_nonce: [u8; 32],
    at_ms: u64,
) -> ServeReceipt {
    let fields = ReceiptFields {
        version: RECEIPT_VERSION,
        content_id: content.clone(),
        bytes: 5 * (1 << 30),
        content_digest: HashAlgorithm::Blake3.digest(b"served content bytes"),
        host_device: host_device.did(),
        witness_device: witness_device.did(),
        host_nonce,
        witness_nonce,
        at_ms,
    };
    let host_sig = fields.sign(host_device);
    let witness_sig = fields.sign(witness_device);
    ServeReceipt::new(fields, host_sig, witness_sig)
}

#[test]
fn a_valid_receipt_verifies_and_names_both_roots() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    let verdict = verify_serve(&receipt, &ctx, &mut replay).unwrap();
    assert_eq!(verdict.host_root.as_str(), host_root.did().as_str());
    assert_eq!(verdict.witness_root.as_str(), witness_root.did().as_str());
    assert_eq!(verdict.bytes, 5 * (1 << 30));
    assert_eq!(verdict.at_ms, 1_000);
    assert_eq!(verdict.content_id.as_str(), content.as_str());
}

#[test]
fn zero_byte_receipts_are_rejected() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let mut receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );
    receipt.fields.bytes = 0;
    // Re-sign after mutating the fields, since the signature covers bytes.
    receipt.host_sig = receipt.fields.sign(&host_dev);
    receipt.witness_sig = receipt.fields.sign(&witness_dev);

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::ZeroBytes
    );
}

#[test]
fn a_device_without_attest_capability_is_rejected() {
    let (host_root, host_dev) = human([10; 32], [11; 32], [12; 32], [13; 32], Capabilities::PAY); // no ATTEST
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::MissingAttestCapability
    );
}

#[test]
fn a_revoked_host_device_is_rejected() {
    let (mut host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );
    host_root.revoke_device(&host_dev.did()).unwrap();

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_serve(&receipt, &ctx, &mut replay),
        Err(StorageProofError::Identity(_))
    ));
}

#[test]
fn matching_nonces_are_rejected_as_replay() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let same = [7u8; 32];
    let receipt = valid_receipt(&host_dev, &witness_dev, &content, same, same, 1_000);

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::Replay
    );
}

#[test]
fn a_replayed_receipt_is_rejected_on_second_verification() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(verify_serve(&receipt, &ctx, &mut replay).is_ok());
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::Replay
    );
}

#[test]
fn self_serve_is_rejected() {
    let (root, dev1) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let mut root = root;
    let dev2 =
        Controller::incept_device_single_from_seeds(&root.did(), &[14u8; 32], &[15u8; 32]).unwrap();
    root.delegate_device(&dev2.did(), Capabilities::primary())
        .unwrap();
    let content = content_id();
    let receipt = valid_receipt(&dev1, &dev2, &content, [1u8; 32], [2u8; 32], 1_000);

    let ctx = VerifyContext {
        host_root: &root.kel(),
        witness_root: &root.kel(),
        host_device: &dev1.kel(),
        witness_device: &dev2.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::SelfServe
    );
}

#[test]
fn a_tampered_transcript_breaks_the_signature() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let mut receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );
    // Tamper with the byte count after signing (a dishonest host inflating
    // the claim) without re-signing.
    receipt.fields.bytes = 500 * (1 << 30);

    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_serve(&receipt, &ctx, &mut replay),
        Err(StorageProofError::Identity(_))
    ));
}

#[test]
fn receipts_older_than_the_freshness_policy_are_refused() {
    let (host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );

    let strict = FreshnessPolicy { max_age_ms: 500 };
    let ctx = VerifyContext {
        host_root: &host_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &strict,
        now_ms: Some(10_000), // well past 1_000 + 500
    };
    let mut replay = InMemoryReplayGuard::new();
    assert_eq!(
        verify_serve(&receipt, &ctx, &mut replay).unwrap_err(),
        StorageProofError::TooOld
    );
}

#[test]
fn a_device_claiming_the_wrong_identity_is_rejected() {
    let (_host_root, host_dev) = human(
        [10; 32],
        [11; 32],
        [12; 32],
        [13; 32],
        Capabilities::primary(),
    );
    let (witness_root, witness_dev) = human(
        [20; 32],
        [21; 32],
        [22; 32],
        [23; 32],
        Capabilities::primary(),
    );
    let (other_root, _other_dev) = human(
        [30; 32],
        [31; 32],
        [32; 32],
        [33; 32],
        Capabilities::primary(),
    );
    let content = content_id();
    let receipt = valid_receipt(
        &host_dev,
        &witness_dev,
        &content,
        [1u8; 32],
        [2u8; 32],
        1_000,
    );

    // Verifier supplies a root KEL that doesn't match host_dev's real
    // delegator at all.
    let ctx = VerifyContext {
        host_root: &other_root.kel(),
        witness_root: &witness_root.kel(),
        host_device: &host_dev.kel(),
        witness_device: &witness_dev.kel(),
        policy: &policy(),
        now_ms: Some(2_000),
    };
    let mut replay = InMemoryReplayGuard::new();
    assert!(matches!(
        verify_serve(&receipt, &ctx, &mut replay),
        Err(StorageProofError::Identity(_))
    ));
}

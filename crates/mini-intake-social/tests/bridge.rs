//! Integration tests for the intake-to-social publication bridge.

use std::fs;

use did_mini::{Capabilities, Controller};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_intake::intake_local_file;
use mini_intake_social::{publish_accepted_intake_as_post, IntakeSocialError};
use mini_intake_types::{
    IntakeEnvelope, IntakeId, IntakeLink, MediaType, ReviewState, SourceRecord,
};
use mini_objects::ObjectType;
use mini_social::resolve_post;
use mini_store::{Backend, MemoryBackend, Store};
use tempfile::tempdir;

/// `mini-intake`'s coordinator key scheme is a private implementation
/// detail (`backend_key` in `coordinator.rs`), but it is documented as
/// exactly "base58btc-encode the digest bytes" -- the same convention
/// `mini_objects::ObjectId` already uses. Reproduced here only to store
/// bytes directly for envelopes this test hand-builds instead of going
/// through `intake_local_file` (which only ever accepts text/Markdown, so
/// it cannot itself produce the non-text/non-UTF-8 fixtures these tests
/// need).
fn backend_key(digest: &Multihash) -> String {
    mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &digest.to_bytes()).unwrap()
}

fn human(seed: u8) -> (did_mini::Did, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root.did(), device)
}

fn write_temp(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn an_accepted_text_envelope_publishes_as_a_real_post_with_a_matching_link() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "hello mininet intake");
    let mut intake_backend = MemoryBackend::new();
    let mut envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();

    let (human, device) = human(1);
    let mut social_store = Store::new(MemoryBackend::new());

    let (post, link) = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    )
    .unwrap();

    assert_eq!(post.object_type, ObjectType::POST);
    let resolved = resolve_post(&social_store, post.id()).unwrap();
    assert_eq!(resolved.text, "hello mininet intake");
    assert_eq!(resolved.author, human);

    match link {
        IntakeLink::Post(digest) => {
            let encoded =
                mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &digest.to_bytes())
                    .unwrap();
            assert_eq!(encoded, post.id().as_str());
        }
        other => panic!("expected IntakeLink::Post, got {other:?}"),
    }
}

#[test]
fn a_markdown_envelope_is_also_accepted() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "README.md", "# hello");
    let mut intake_backend = MemoryBackend::new();
    let mut envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();

    let (human, device) = human(2);
    let mut social_store = Store::new(MemoryBackend::new());

    let (post, _link) = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    )
    .unwrap();
    assert_eq!(
        resolve_post(&social_store, post.id()).unwrap().text,
        "# hello"
    );
}

#[test]
fn an_unreviewed_envelope_is_refused() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "not yet reviewed");
    let mut intake_backend = MemoryBackend::new();
    let envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    assert_eq!(envelope.review_state(), ReviewState::Unreviewed);

    let (human, device) = human(3);
    let mut social_store = Store::new(MemoryBackend::new());

    let result = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    );
    assert!(matches!(result, Err(IntakeSocialError::NotAccepted)));
    assert!(social_store.by_author(&human).unwrap().is_empty());
}

#[test]
fn an_under_review_envelope_is_also_refused() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "still under review");
    let mut intake_backend = MemoryBackend::new();
    let mut envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();

    let (human, device) = human(4);
    let mut social_store = Store::new(MemoryBackend::new());

    let result = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    );
    assert!(matches!(result, Err(IntakeSocialError::NotAccepted)));
}

#[test]
fn a_rejected_envelope_is_refused() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "rejected content");
    let mut intake_backend = MemoryBackend::new();
    let mut envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::Rejected)
        .unwrap();

    let (human, device) = human(5);
    let mut social_store = Store::new(MemoryBackend::new());

    let result = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    );
    assert!(matches!(result, Err(IntakeSocialError::NotAccepted)));
}

#[test]
fn publishing_the_same_accepted_envelope_twice_produces_two_distinct_signed_posts() {
    // This bridge does not itself deduplicate -- `mini-intake`'s own
    // content-addressed dedup already prevents re-intaking identical bytes
    // as a *second envelope*, but nothing stops a caller from calling this
    // bridge twice over the same already-Accepted envelope (e.g. after a
    // crash and retry). Each call produces its own freshly signed, distinct
    // `POST` object (different sequence/timestamp), which is honest: this
    // crate has no envelope-to-post dedup story of its own, and does not
    // pretend to.
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "published twice");
    let mut intake_backend = MemoryBackend::new();
    let mut envelope = intake_local_file(&mut intake_backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();

    let (human, device) = human(6);
    let mut social_store = Store::new(MemoryBackend::new());

    let (first, _) = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    )
    .unwrap();
    let (second, _) = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_001,
        2,
    )
    .unwrap();
    assert_ne!(first.id(), second.id());
}

#[test]
fn an_accepted_non_text_envelope_is_refused() {
    // `intake_local_file` only ever accepts text/Markdown, so a non-text
    // envelope is hand-built here -- exercising the bridge's own
    // defense-in-depth check, not `mini-intake`'s (already-tested)
    // extension gate.
    let digest = Multihash::of(HashAlgorithm::Blake3, b"%PDF-1.4 fake");
    let mut intake_backend = MemoryBackend::new();
    intake_backend
        .put_blob(&backend_key(&digest), b"%PDF-1.4 fake")
        .unwrap();
    let mut envelope = IntakeEnvelope::new(
        IntakeId(Multihash::of(HashAlgorithm::Blake3, &digest.to_bytes())),
        SourceRecord {
            digest,
            media_type: MediaType::Pdf,
            byte_length: 13,
            received_at_ms: 1_000,
            declared_name: Some("scan.pdf".to_string()),
        },
    );
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();

    let (human, device) = human(7);
    let mut social_store = Store::new(MemoryBackend::new());

    let result = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    );
    assert!(matches!(
        result,
        Err(IntakeSocialError::UnsupportedMediaType)
    ));
}

#[test]
fn non_utf8_source_bytes_are_refused_even_with_a_text_media_type() {
    // Defense in depth: `mini-intake`'s own coordinator already refuses
    // non-UTF-8 bytes at intake time, so this envelope is hand-built to
    // simulate a peer or older code path that skipped that check --
    // this bridge must not trust a declared `TextPlain` label over the
    // actual bytes.
    let raw = [0xff, 0xfe, 0xfd];
    let digest = Multihash::of(HashAlgorithm::Blake3, &raw);
    let mut intake_backend = MemoryBackend::new();
    intake_backend
        .put_blob(&backend_key(&digest), &raw)
        .unwrap();
    let mut envelope = IntakeEnvelope::new(
        IntakeId(Multihash::of(HashAlgorithm::Blake3, &digest.to_bytes())),
        SourceRecord {
            digest,
            media_type: MediaType::TextPlain,
            byte_length: raw.len() as u64,
            received_at_ms: 1_000,
            declared_name: None,
        },
    );
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();

    let (human, device) = human(8);
    let mut social_store = Store::new(MemoryBackend::new());

    let result = publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &device,
        &envelope,
        2_000,
        1,
    );
    assert!(matches!(result, Err(IntakeSocialError::NotUtf8)));
}

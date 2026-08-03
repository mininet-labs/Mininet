use std::fs;

use mini_intake::{
    intake_local_file, load_envelope, read_source_bytes, read_verified_source_bytes, save_envelope,
    IntakeCoordError,
};
use mini_intake_types::{AuthorityClass, MediaType, ReviewState};
use mini_store::{Backend, MemoryBackend};
use tempfile::tempdir;

/// Mirrors the private `backend_key` encoding in `mini-intake`'s
/// coordinator (base58btc of the raw digest bytes) so tests can reach in
/// and corrupt a blob under its exact content-addressed key, the same way
/// a corrupted/buggy/malicious local backend would.
fn blob_key(digest: &mini_crypto::Multihash) -> String {
    mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &digest.to_bytes()).unwrap()
}

fn write_temp(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn intaking_a_text_file_produces_an_unreviewed_untrusted_envelope() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "notes.txt", "hello mininet");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();

    assert_eq!(envelope.review_state(), ReviewState::Unreviewed);
    assert_eq!(envelope.authority(), AuthorityClass::UntrustedExternal);
    assert_eq!(envelope.source.media_type, MediaType::TextPlain);
    assert_eq!(envelope.source.byte_length, "hello mininet".len() as u64);
    assert_eq!(envelope.source.declared_name.as_deref(), Some("notes.txt"));
    assert_eq!(envelope.source.received_at_ms, 1_000);
}

#[test]
fn intaking_a_markdown_file_is_labeled_markdown() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "README.md", "# hi");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    assert_eq!(envelope.source.media_type, MediaType::Markdown);
}

#[test]
fn an_unsupported_extension_is_rejected_without_writing_anything() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "scan.pdf", "%PDF-1.4 fake");
    let mut backend = MemoryBackend::new();

    let result = intake_local_file(&mut backend, &path, 1_000);
    assert!(matches!(
        result,
        Err(IntakeCoordError::UnsupportedMediaType)
    ));
}

#[test]
fn non_utf8_bytes_are_rejected_even_with_a_txt_extension() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.txt");
    fs::write(&path, [0xff, 0xfe, 0x00, 0xff]).unwrap();
    let mut backend = MemoryBackend::new();

    let result = intake_local_file(&mut backend, &path, 1_000);
    assert!(matches!(result, Err(IntakeCoordError::NotUtf8)));
}

#[test]
fn intaking_identical_content_twice_from_different_paths_deduplicates() {
    let dir = tempdir().unwrap();
    let first_path = write_temp(dir.path(), "a.txt", "same bytes");
    let second_path = write_temp(dir.path(), "b.txt", "same bytes");
    let mut backend = MemoryBackend::new();

    let first = intake_local_file(&mut backend, &first_path, 1_000).unwrap();
    let second = intake_local_file(&mut backend, &second_path, 2_000).unwrap();

    // Same content -> same intake id, and the *first* intake's metadata wins
    // (received_at_ms/declared_name are not silently overwritten by the
    // second call).
    assert_eq!(first.intake_id, second.intake_id);
    assert_eq!(first, second);
    assert_eq!(second.source.declared_name.as_deref(), Some("a.txt"));
    assert_eq!(second.source.received_at_ms, 1_000);
}

#[test]
fn a_dedup_hit_never_resets_an_already_advanced_review_state() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "doc.md", "some evidence");
    let mut backend = MemoryBackend::new();

    let mut envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    envelope
        .advance_review_state(ReviewState::UnderReview)
        .unwrap();
    envelope
        .advance_review_state(ReviewState::Accepted)
        .unwrap();
    envelope
        .promote_authority(AuthorityClass::ReviewedEvidence)
        .unwrap();
    save_envelope(&mut backend, &envelope).unwrap();

    // Re-intaking the exact same bytes must return the *advanced* envelope,
    // not silently downgrade it back to a fresh Unreviewed/UntrustedExternal
    // one -- this is the "no automatic authority promotion" rule's other
    // half: no automatic demotion either.
    let reintaken = intake_local_file(&mut backend, &path, 9_999).unwrap();
    assert_eq!(reintaken.review_state(), ReviewState::Accepted);
    assert_eq!(reintaken.authority(), AuthorityClass::ReviewedEvidence);
    assert_eq!(reintaken.source.received_at_ms, 1_000);
}

#[test]
fn read_source_bytes_returns_the_exact_original_bytes() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "report.md", "the exact original bytes");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    let bytes = read_source_bytes(&backend, &envelope.source.digest).unwrap();
    assert_eq!(bytes, b"the exact original bytes");
}

#[test]
fn load_envelope_returns_none_for_content_never_intaken() {
    let backend = MemoryBackend::new();
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "ghost.txt", "never intaken");
    // Compute what the intake id *would* be without ever calling intake.
    let mut scratch_backend = MemoryBackend::new();
    let envelope = intake_local_file(&mut scratch_backend, &path, 1_000).unwrap();

    assert_eq!(load_envelope(&backend, &envelope.intake_id).unwrap(), None);
}

#[test]
fn a_real_fs_backend_round_trips_across_reopens() {
    let source_dir = tempdir().unwrap();
    let path = write_temp(source_dir.path(), "durable.txt", "durable content");
    let store_dir = tempdir().unwrap();

    let intake_id = {
        let mut backend = mini_store::FsBackend::open(store_dir.path()).unwrap();
        let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
        envelope.intake_id
    };

    // Reopen against the same directory: a fresh backend handle must see
    // the same immutable state (real persistence, not an in-memory fixture
    // artifact).
    let backend = mini_store::FsBackend::open(store_dir.path()).unwrap();
    let reloaded = load_envelope(&backend, &intake_id).unwrap().unwrap();
    assert_eq!(reloaded.review_state(), ReviewState::Unreviewed);
    assert_eq!(
        read_source_bytes(&backend, &reloaded.source.digest).unwrap(),
        b"durable content"
    );
}

#[test]
fn read_verified_source_bytes_accepts_a_valid_untouched_source() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "clean.md", "nothing corrupted here");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    let bytes = read_verified_source_bytes(&backend, &envelope).unwrap();
    assert_eq!(bytes, b"nothing corrupted here");
}

#[test]
fn read_verified_source_bytes_rejects_bytes_substituted_under_the_same_digest_key() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "reviewed.md", "the material that was reviewed");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    // Simulate a corrupted/malicious backend: overwrite the blob under the
    // exact same content-addressed key with different bytes of the *same
    // length* (built to match, not hand-counted), so a length-only check
    // alone would not catch it.
    let original_len = envelope.source.byte_length as usize;
    let substituted: Vec<u8> = b"XYZ".iter().cycle().take(original_len).copied().collect();
    assert_ne!(substituted, b"the material that was reviewed".to_vec());
    let key = blob_key(&envelope.source.digest);
    backend.put_blob(&key, &substituted).unwrap();

    let result = read_verified_source_bytes(&backend, &envelope);
    assert!(matches!(
        result,
        Err(IntakeCoordError::SourceDigestMismatch)
    ));
}

#[test]
fn read_verified_source_bytes_rejects_a_wrong_length_substitution() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "reviewed2.md", "original");
    let mut backend = MemoryBackend::new();

    let envelope = intake_local_file(&mut backend, &path, 1_000).unwrap();
    let key = blob_key(&envelope.source.digest);
    backend
        .put_blob(&key, b"a much longer substituted body")
        .unwrap();

    let result = read_verified_source_bytes(&backend, &envelope);
    assert!(matches!(
        result,
        Err(IntakeCoordError::SourceLengthMismatch)
    ));
}

#[test]
fn read_verified_source_bytes_rejects_a_hand_built_envelope_with_a_mismatched_intake_id() {
    let dir = tempdir().unwrap();
    let path = write_temp(dir.path(), "a.md", "content A");
    let mut backend = MemoryBackend::new();
    let envelope_a = intake_local_file(&mut backend, &path, 1_000).unwrap();

    let path_b = write_temp(dir.path(), "b.md", "content B, different length!!");
    let envelope_b = intake_local_file(&mut backend, &path_b, 2_000).unwrap();

    // Hand-build an envelope whose source record points at B's real,
    // uncorrupted digest but whose intake_id is A's — this must be caught
    // independently of the digest check, not assumed to follow from it.
    let mismatched =
        mini_intake_types::IntakeEnvelope::new(envelope_a.intake_id, envelope_b.source.clone());

    let result = read_verified_source_bytes(&backend, &mismatched);
    assert!(matches!(result, Err(IntakeCoordError::IntakeIdMismatch)));
}

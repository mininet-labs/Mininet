use std::fs;

use mini_intake::{
    intake_local_file, load_envelope, read_source_bytes, save_envelope, IntakeCoordError,
};
use mini_intake_types::{AuthorityClass, MediaType, ReviewState};
use mini_store::MemoryBackend;
use tempfile::tempdir;

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

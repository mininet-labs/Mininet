//! `mini intake` -- a real developer-facing caller driving Mininet Intake
//! (Track B1/B2) and the Track B5 publication bridge (`mini-intake-social`,
//! D-0429) together as one visible workflow: intake a local text/Markdown
//! file, advance its review state, and publish an already-`Accepted`
//! envelope as a real signed `mini-social` post -- closing the "still not
//! built" gap D-0429's own Required follow-up named (no CLI or UI caller
//! previously drove this pipeline end to end).
//!
//! Intake material is stored under a separate `FsBackend` rooted at
//! `<home>/intake`, distinct from the signed-object `--store` path --
//! the same two-storage-layer split `mini-intake`'s own docs already
//! justify (intake material has no signature at ingest time; `Store`
//! assumes self-certifying signed objects).
//!
//! This command group does not support `--json` yet (matching `identity`/
//! `kel`/`repo`/`pr`/`sync`'s existing convention, not the newer `build`/
//! `release`/`provenance`/`installer`/`team`/`task` one) -- `cli::dispatch`
//! rejects the flag cleanly rather than silently ignoring it.
//!
//! **`ReviewState::Accepted` is a local workflow state, not independent
//! attestation.** `mini intake advance <id> accepted` records only that a
//! local process changed a mutable field on this device -- no reviewer
//! identity, signature, reason, or evidence is captured. It proves a
//! workflow step happened, not that the material is factually correct,
//! that anyone besides the operator of this `--home` reviewed it, or that
//! it carries any public authority. Do not describe an `Accepted` envelope
//! as "verified evidence" without a separate signed evidence policy behind
//! that claim -- none exists yet (see D-0429's Required follow-up).

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use mini_intake::{intake_local_file, load_envelope, save_envelope};
use mini_intake_types::{IntakeId, IntakeLink, ReviewState};
use mini_objects::Object;
use mini_store::FsBackend;

use crate::error::{CliError, Result};
use crate::sequence;

fn intake_backend_path(home: &Path) -> PathBuf {
    home.join("intake")
}

fn open_intake_backend(home: &Path) -> Result<FsBackend> {
    FsBackend::open(&intake_backend_path(home)).map_err(|e| CliError::Store(e.to_string()))
}

fn publish_journal_dir(home: &Path) -> PathBuf {
    home.join("intake_publish_journal")
}

fn publish_lock_path(home: &Path, id: &IntakeId) -> PathBuf {
    publish_journal_dir(home).join(format!("{}.lock", encode_intake_id(id)))
}

fn publish_journal_path(home: &Path, id: &IntakeId) -> PathBuf {
    publish_journal_dir(home).join(encode_intake_id(id))
}

/// Hold an OS-backed exclusive lock over one intake id's publish attempt
/// for the caller's entire critical section, the same convention
/// `crate::sequence::next` already uses for the sequence counter — so two
/// `mini intake publish-post <id>` invocations racing over the same id
/// serialize instead of both signing and inserting a post.
fn acquire_publish_lock(home: &Path, id: &IntakeId) -> Result<File> {
    let dir = publish_journal_dir(home);
    fs::create_dir_all(&dir).map_err(|e| CliError::Io(e.to_string()))?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(publish_lock_path(home, id))
        .map_err(|e| CliError::Io(e.to_string()))?;
    #[allow(clippy::incompatible_msrv)]
    lock.lock().map_err(|e| CliError::Io(e.to_string()))?;
    Ok(lock)
}

/// Recover an interrupted previous publish attempt's already-signed post,
/// if one was left behind by a crash between signing and completing the
/// attempt. `None` means no attempt is in flight for this intake id.
fn read_publish_journal(home: &Path, id: &IntakeId) -> Result<Option<Object>> {
    match fs::read(publish_journal_path(home, id)) {
        Ok(bytes) => Object::from_bytes(&bytes)
            .map(Some)
            .map_err(|e| CliError::Object(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CliError::Io(e.to_string())),
    }
}

/// Durably persist the exact signed bytes of a newly built (not yet
/// inserted) post *before* inserting it anywhere, so a crash after this
/// point recovers the identical object on retry instead of allocating a
/// new sequence/timestamp and signing a second, distinct, still
/// feed-eligible post. Writes to a temp file then renames, matching
/// `FsBackend`'s own atomic-write convention.
fn write_publish_journal(home: &Path, id: &IntakeId, object: &Object) -> Result<()> {
    fs::create_dir_all(publish_journal_dir(home)).map_err(|e| CliError::Io(e.to_string()))?;
    let path = publish_journal_path(home, id);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, object.to_bytes()).map_err(|e| CliError::Io(e.to_string()))?;
    fs::rename(&tmp, &path).map_err(|e| CliError::Io(e.to_string()))?;
    Ok(())
}

/// Mark a publish attempt complete. Best-effort: an attempt that already
/// succeeded (the post is inserted and the link is attached) is complete
/// regardless of whether this cleanup itself runs, so a failure here is
/// not surfaced as a command error.
fn clear_publish_journal(home: &Path, id: &IntakeId) {
    let _ = fs::remove_file(publish_journal_path(home, id));
}

/// The `IntakeLink::Post` target this envelope already carries, if any.
fn existing_post_link(envelope: &mini_intake_types::IntakeEnvelope) -> Option<IntakeLink> {
    envelope
        .links()
        .iter()
        .find(|link| matches!(link, IntakeLink::Post(_)))
        .cloned()
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn encode_intake_id(id: &IntakeId) -> String {
    hex_encode(&id.to_bytes())
}

fn decode_intake_id(hex: &str) -> Result<IntakeId> {
    let bytes = crate::identity::hex_decode(hex)?;
    IntakeId::from_bytes(&bytes).map_err(|e| CliError::Usage(format!("bad intake id {hex:?}: {e}")))
}

fn parse_review_state(s: &str) -> Result<ReviewState> {
    match s {
        "unreviewed" => Ok(ReviewState::Unreviewed),
        "quarantined" => Ok(ReviewState::Quarantined),
        "under-review" => Ok(ReviewState::UnderReview),
        "accepted" => Ok(ReviewState::Accepted),
        "rejected" => Ok(ReviewState::Rejected),
        "superseded" => Ok(ReviewState::Superseded),
        other => Err(CliError::Usage(format!(
            "unknown review state {other:?} (expected one of: unreviewed, quarantined, \
             under-review, accepted, rejected, superseded)"
        ))),
    }
}

/// `mini intake add <path>` -- intake one local text/Markdown file.
pub fn cmd_add(home: &Path, path_str: &str) -> Result<String> {
    let mut backend = open_intake_backend(home)?;
    let path = PathBuf::from(path_str);
    let envelope = intake_local_file(&mut backend, &path, sequence::now_ms())
        .map_err(|e| CliError::Intake(e.to_string()))?;
    Ok(format!(
        "intake id {} -- review_state={:?} authority={:?} media_type={:?} bytes={}",
        encode_intake_id(&envelope.intake_id),
        envelope.review_state(),
        envelope.authority(),
        envelope.source.media_type,
        envelope.source.byte_length,
    ))
}

/// `mini intake show <id>` -- print an already-intaken envelope's state.
pub fn cmd_show(home: &Path, id_hex: &str) -> Result<String> {
    let backend = open_intake_backend(home)?;
    let id = decode_intake_id(id_hex)?;
    let envelope = load_envelope(&backend, &id)
        .map_err(|e| CliError::Intake(e.to_string()))?
        .ok_or_else(|| CliError::Usage(format!("no intake envelope for id {id_hex}")))?;
    Ok(format!(
        "intake id {} -- review_state={:?} authority={:?} media_type={:?} \
         declared_name={:?} bytes={} links={}",
        id_hex,
        envelope.review_state(),
        envelope.authority(),
        envelope.source.media_type,
        envelope.source.declared_name,
        envelope.source.byte_length,
        envelope.links().len(),
    ))
}

/// `mini intake advance <id> <state>` -- move an envelope's review state
/// forward one legal step (see `ReviewState::allows_transition_to`).
pub fn cmd_advance(home: &Path, id_hex: &str, next_state: &str) -> Result<String> {
    let mut backend = open_intake_backend(home)?;
    let id = decode_intake_id(id_hex)?;
    let mut envelope = load_envelope(&backend, &id)
        .map_err(|e| CliError::Intake(e.to_string()))?
        .ok_or_else(|| CliError::Usage(format!("no intake envelope for id {id_hex}")))?;
    let next = parse_review_state(next_state)?;
    let from = envelope.review_state();
    envelope
        .advance_review_state(next)
        .map_err(|e| CliError::Usage(format!("illegal review transition: {e}")))?;
    save_envelope(&mut backend, &envelope).map_err(|e| CliError::Intake(e.to_string()))?;
    Ok(format!("intake {id_hex} advanced {from:?} -> {next:?}"))
}

/// `mini intake publish-post <id>` -- publish an already-`Accepted`
/// envelope as a real signed `mini-social` post, then attach the matching
/// `IntakeLink::Post` back onto the envelope. Fails with the same
/// `NotAccepted` refusal `mini-intake-social` itself enforces if the
/// envelope has not reached `Accepted` yet.
///
/// Idempotent and crash-recoverable, under an OS-backed per-intake-id
/// lock held for the whole call:
///
/// - If the envelope already carries an `IntakeLink::Post` (from this
///   call, a previous call, or a concurrent call that reached the lock
///   first), that existing post is returned rather than signing a second
///   one -- callers wanting a genuinely independent second post publish a
///   *different* envelope, not retry this one (this crate's own
///   `mini_intake_social::publish_accepted_intake_as_post`, called
///   directly rather than through this command, still has no dedup of its
///   own -- documented there).
/// - Otherwise, a crash-recoverable journal keyed by intake id records the
///   exact signed object bytes *before* inserting them anywhere. A retry
///   after a crash between signing and completing the attempt reuses that
///   exact object (same id) rather than allocating a new
///   sequence/timestamp and signing a distinct, still feed-eligible
///   orphan post.
pub fn cmd_publish_post(home: &Path, store_path: &Path, id_hex: &str) -> Result<String> {
    let identity = crate::identity::load(home)?;
    let id = decode_intake_id(id_hex)?;
    let _lock = acquire_publish_lock(home, &id)?;

    let mut intake_backend = open_intake_backend(home)?;
    let mut envelope = load_envelope(&intake_backend, &id)
        .map_err(|e| CliError::Intake(e.to_string()))?
        .ok_or_else(|| CliError::Usage(format!("no intake envelope for id {id_hex}")))?;

    if let Some(IntakeLink::Post(digest)) = existing_post_link(&envelope) {
        clear_publish_journal(home, &id);
        return Ok(format!(
            "intake {id_hex} already published as post {}",
            hex_encode(&digest.to_bytes())
        ));
    }

    let mut social_store = crate::store::open_store(store_path)?;
    let human = identity.human_did();

    let object = match read_publish_journal(home, &id)? {
        Some(object) => object,
        None => {
            let seq = sequence::next(home)?;
            let now = sequence::now_ms();
            let object = mini_intake_social::build_accepted_intake_post(
                &intake_backend,
                &human,
                &identity.device,
                &envelope,
                now,
                seq,
            )
            .map_err(|e| CliError::Intake(e.to_string()))?;
            write_publish_journal(home, &id, &object)?;
            object
        }
    };

    social_store
        .insert(&object)
        .map_err(|e| CliError::Store(e.to_string()))?;

    let link = mini_intake_social::intake_link_for_post(&object)
        .map_err(|e| CliError::Intake(e.to_string()))?;
    envelope
        .add_link(link)
        .map_err(|e| CliError::Intake(e.to_string()))?;
    save_envelope(&mut intake_backend, &envelope).map_err(|e| CliError::Intake(e.to_string()))?;

    clear_publish_journal(home, &id);

    Ok(format!(
        "published post {} for intake {id_hex}",
        object.id().as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_intake_types::ReviewState;

    /// A fresh home/store pair with one already-`Accepted` text envelope,
    /// ready for `cmd_publish_post`.
    fn setup_accepted_envelope() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        IntakeId,
        crate::identity::Identity,
    ) {
        let home_dir = tempfile::tempdir().unwrap();
        let store_dir = tempfile::tempdir().unwrap();
        let home = home_dir.path();
        let identity = crate::identity::init(home).unwrap();

        let notes_path = home.join("notes.txt");
        std::fs::write(&notes_path, "crash recovery fixture").unwrap();
        let mut backend = open_intake_backend(home).unwrap();
        let mut envelope =
            mini_intake::intake_local_file(&mut backend, &notes_path, sequence::now_ms()).unwrap();
        envelope
            .advance_review_state(ReviewState::UnderReview)
            .unwrap();
        envelope
            .advance_review_state(ReviewState::Accepted)
            .unwrap();
        save_envelope(&mut backend, &envelope).unwrap();

        (home_dir, store_dir, envelope.intake_id, identity)
    }

    #[test]
    fn a_crash_between_signing_and_completing_is_recovered_not_duplicated() {
        let (home_dir, store_dir, id, identity) = setup_accepted_envelope();
        let home = home_dir.path();
        let store_path = store_dir.path();

        // Simulate the crash: build and journal a signed post exactly as
        // cmd_publish_post's fresh-attempt branch would, but stop before
        // inserting it into the store or linking the envelope -- as if
        // the process died right after the journal write.
        let backend = open_intake_backend(home).unwrap();
        let envelope = load_envelope(&backend, &id).unwrap().unwrap();
        let human = identity.human_did();
        let prebuilt = mini_intake_social::build_accepted_intake_post(
            &backend,
            &human,
            &identity.device,
            &envelope,
            12_345,
            1,
        )
        .unwrap();
        write_publish_journal(home, &id, &prebuilt).unwrap();

        // Resume: cmd_publish_post must reuse the exact journaled object
        // (same content id -- same signature, sequence, timestamp), not
        // sign a second, distinct one.
        let result = cmd_publish_post(home, store_path, &encode_intake_id(&id)).unwrap();
        assert!(result.contains(prebuilt.id().as_str()));

        // The store contains exactly the journaled object under its
        // author -- no second, orphaned post.
        let store = crate::store::open_store(store_path).unwrap();
        let objects = store.by_author(&human).unwrap();
        assert_eq!(objects, vec![prebuilt.id().clone()]);

        // The envelope links to that exact post, and the journal is
        // cleared once the attempt genuinely completes.
        let backend = open_intake_backend(home).unwrap();
        let envelope = load_envelope(&backend, &id).unwrap().unwrap();
        assert_eq!(envelope.links().len(), 1);
        assert!(read_publish_journal(home, &id).unwrap().is_none());
    }

    #[test]
    fn publish_post_is_idempotent_after_a_completed_attempt() {
        let (home_dir, store_dir, id, _identity) = setup_accepted_envelope();
        let home = home_dir.path();
        let store_path = store_dir.path();
        let id_hex = encode_intake_id(&id);

        let first = cmd_publish_post(home, store_path, &id_hex).unwrap();
        assert!(first.starts_with("published post "));
        let second = cmd_publish_post(home, store_path, &id_hex).unwrap();
        assert!(second.contains("already published as post"));

        let backend = open_intake_backend(home).unwrap();
        let envelope = load_envelope(&backend, &id).unwrap().unwrap();
        assert_eq!(envelope.links().len(), 1);
    }

    #[test]
    fn concurrent_publish_post_calls_produce_one_canonical_publication() {
        let (home_dir, store_dir, id, _identity) = setup_accepted_envelope();
        let home = home_dir.path().to_path_buf();
        let store_path = store_dir.path().to_path_buf();
        let id_hex = encode_intake_id(&id);

        let results: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let home = home.clone();
                    let store_path = store_path.clone();
                    let id_hex = id_hex.clone();
                    scope.spawn(move || cmd_publish_post(&home, &store_path, &id_hex).unwrap())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        let published = results
            .iter()
            .filter(|r| r.starts_with("published post "))
            .count();
        let already = results
            .iter()
            .filter(|r| r.contains("already published as post"))
            .count();
        assert_eq!(published, 1);
        assert_eq!(already, 3);

        let backend = open_intake_backend(&home).unwrap();
        let envelope = load_envelope(&backend, &id).unwrap().unwrap();
        assert_eq!(envelope.links().len(), 1);
    }
}

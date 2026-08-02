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

use std::path::{Path, PathBuf};

use mini_intake::{intake_local_file, load_envelope, save_envelope};
use mini_intake_types::{IntakeId, ReviewState};
use mini_store::FsBackend;

use crate::error::{CliError, Result};
use crate::sequence;

fn intake_backend_path(home: &Path) -> PathBuf {
    home.join("intake")
}

fn open_intake_backend(home: &Path) -> Result<FsBackend> {
    FsBackend::open(&intake_backend_path(home)).map_err(|e| CliError::Store(e.to_string()))
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
pub fn cmd_publish_post(home: &Path, store_path: &Path, id_hex: &str) -> Result<String> {
    let identity = crate::identity::load(home)?;
    let mut intake_backend = open_intake_backend(home)?;
    let id = decode_intake_id(id_hex)?;
    let mut envelope = load_envelope(&intake_backend, &id)
        .map_err(|e| CliError::Intake(e.to_string()))?
        .ok_or_else(|| CliError::Usage(format!("no intake envelope for id {id_hex}")))?;

    let mut social_store = crate::store::open_store(store_path)?;
    let human = identity.human_did();
    let seq = sequence::next(home)?;
    let now = sequence::now_ms();

    let (post, link) = mini_intake_social::publish_accepted_intake_as_post(
        &intake_backend,
        &mut social_store,
        &human,
        &identity.device,
        &envelope,
        now,
        seq,
    )
    .map_err(|e| CliError::Intake(e.to_string()))?;

    envelope
        .add_link(link)
        .map_err(|e| CliError::Intake(e.to_string()))?;
    save_envelope(&mut intake_backend, &envelope).map_err(|e| CliError::Intake(e.to_string()))?;

    Ok(format!(
        "published post {} for intake {id_hex}",
        post.id().as_str()
    ))
}

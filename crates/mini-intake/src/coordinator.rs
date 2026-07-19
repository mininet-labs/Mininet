//! The trusted intake coordinator (Track B2). Turns a local text/Markdown
//! file into a stored, immutable [`SourceRecord`] plus an
//! [`IntakeEnvelope`], deduplicating on content and never promoting or
//! demoting the envelope's [`ReviewState`]/[`AuthorityClass`] — that stays
//! exactly whatever [`IntakeEnvelope::new`] set it to (`Unreviewed`/
//! `UntrustedExternal`) until a separate, later, human-driven call advances
//! it (research report §25, PR B2: "no automatic authority promotion").

use std::fs;
use std::path::Path;

use mini_crypto::{HashAlgorithm, Multihash};
use mini_intake_types::{IntakeEnvelope, IntakeId, MediaType, SourceRecord};
use mini_store::Backend;

use crate::error::{IntakeCoordError, Result};
use crate::media::detect_media_type;

/// Derive this content's [`IntakeId`] from its digest, not from the file
/// path or name — two different local paths holding byte-identical
/// content must resolve to the exact same intake id (this is what makes
/// deduplication content-addressed rather than path-addressed).
fn intake_id_for(source_digest: &Multihash) -> IntakeId {
    IntakeId(Multihash::of(
        HashAlgorithm::Blake3,
        &source_digest.to_bytes(),
    ))
}

/// Encode a [`Multihash`] (or any digest-shaped byte string) as a
/// `Backend`-safe key: `Backend::put_blob`/`put_meta` require
/// alphanumeric/`[-_./]` keys, and multibase base58btc output is exactly
/// that (the same convention `mini_objects::ObjectId` already uses).
fn backend_key(bytes: &[u8]) -> String {
    mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, bytes)
        .expect("BASE58BTC is always a supported multibase prefix")
}

/// Look up an already-intaken envelope by its [`IntakeId`], without
/// creating one. `Ok(None)` means this exact content has never been
/// intaken into this backend.
pub fn load_envelope<B: Backend>(
    backend: &B,
    intake_id: &IntakeId,
) -> Result<Option<IntakeEnvelope>> {
    let key = backend_key(&intake_id.0.to_bytes());
    match backend.get_blob(&key)? {
        Some(bytes) => Ok(Some(IntakeEnvelope::from_bytes(&bytes)?)),
        None => Ok(None),
    }
}

/// Fetch back the original immutable bytes for a source digest. This is
/// the only way to read intake source bytes: [`IntakeEnvelope`]/
/// [`SourceRecord`] never carry them, only their content address
/// (`mini-intake-types`'s own documented scope).
pub fn read_source_bytes<B: Backend>(backend: &B, digest: &Multihash) -> Result<Vec<u8>> {
    let key = backend_key(&digest.to_bytes());
    backend
        .get_blob(&key)?
        .ok_or(IntakeCoordError::Store(mini_store::StoreError::NotFound))
}

/// Intake one local text/Markdown file: hash it, store the immutable
/// source bytes, and create (or, on a dedup hit, return the existing)
/// [`IntakeEnvelope`].
///
/// Deduplication is by content, not path: intaking byte-identical content
/// from two different paths (or the same path twice) returns the *same*
/// envelope both times, untouched — a caller that already advanced its
/// review state does not get silently reset back to `Unreviewed` just
/// because someone re-ran intake over the same bytes. Only the very first
/// call for a given digest writes anything.
///
/// Atomicity is honest, not oversold: each individual write (`put_blob`
/// for the source bytes, `put_blob` for the envelope) is atomic at the
/// backend level (`FsBackend` does tmp-file-then-rename). The two-step
/// pipeline as a whole is crash-*resumable*, not cross-file-transactional
/// — a crash between the two writes just means the next call re-derives
/// the identical source blob (a harmless no-op) and then completes the
/// still-missing envelope write.
pub fn intake_local_file<B: Backend>(
    backend: &mut B,
    path: &Path,
    received_at_ms: u64,
) -> Result<IntakeEnvelope> {
    let media_type: MediaType = detect_media_type(path)?;
    let bytes = fs::read(path)?;
    // Track B2 is scoped to text/Markdown; bytes that don't actually decode
    // as UTF-8 cannot honestly carry either label, regardless of extension.
    std::str::from_utf8(&bytes).map_err(|_| IntakeCoordError::NotUtf8)?;

    let digest = Multihash::of(HashAlgorithm::Blake3, &bytes);
    let intake_id = intake_id_for(&digest);
    let envelope_key = backend_key(&intake_id.0.to_bytes());

    if let Some(existing) = backend.get_blob(&envelope_key)? {
        return Ok(IntakeEnvelope::from_bytes(&existing)?);
    }

    let source_key = backend_key(&digest.to_bytes());
    backend.put_blob(&source_key, &bytes)?;

    // Best-effort only: a path with no representable file-name component
    // (e.g. one ending in `..`) just means no declared name, not a failure
    // — `SourceRecord.declared_name` is already `Option` for exactly this.
    let declared_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string);

    let source = SourceRecord {
        digest,
        media_type,
        byte_length: bytes.len() as u64,
        received_at_ms,
        declared_name,
    };
    let envelope = IntakeEnvelope::new(intake_id, source);
    backend.put_blob(&envelope_key, &envelope.to_bytes())?;
    Ok(envelope)
}

/// Persist a mutated envelope (e.g. after a caller-driven
/// `advance_review_state`/`promote_authority` call) back to the backend
/// under its own, unchanged [`IntakeId`]. This crate never calls this on
/// a caller's behalf — only [`intake_local_file`]'s first-time path does,
/// via its own internal write, and that only ever writes a fresh
/// `Unreviewed`/`UntrustedExternal` envelope.
pub fn save_envelope<B: Backend>(backend: &mut B, envelope: &IntakeEnvelope) -> Result<()> {
    let key = backend_key(&envelope.intake_id.0.to_bytes());
    backend.put_blob(&key, &envelope.to_bytes())?;
    Ok(())
}

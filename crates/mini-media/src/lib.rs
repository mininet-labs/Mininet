//! Chunked, content-addressed media (UI plan E7): the creator space's data
//! layer, and the artifact carrier for the forge's releases (D-0020: the
//! network is the store).
//!
//! A large payload is split into ≤1 MiB **chunk objects** (each
//! content-addressed, so per-chunk integrity is free) and described by one
//! **manifest object** (`MEDIA_MANIFEST`) that records the content type, total
//! length, the BLAKE3 digest of the whole payload, and an *ordered* list of
//! chunk links. Assembly re-verifies the whole-payload digest, so a manifest
//! cannot lie about what its chunks compose into.
//!
//! **Progressive & interruption-proof by construction:** chunks are ordinary
//! objects, so they ride `mini-sync` like everything else — arriving in any
//! order, across many short encounters (A3 store-and-forward).
//! [`missing_chunks`] says exactly what a player/updater still needs; nothing
//! ever restarts from zero.
//!
//! Honest limit: one manifest addresses up to 256 chunks (≈256 MiB). Long-form
//! media nests manifests in a later batch; the beta promises nearby-first,
//! relay-accelerated media — not a CDN.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use did_mini::{Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload, MAX_LINKS};
use mini_store::{Backend, Store, StoreError};

/// Chunk size (1 MiB).
pub const CHUNK_SIZE: usize = 1024 * 1024;
/// Maximum chunks one manifest may address (envelope link cap).
pub const MAX_CHUNKS: usize = MAX_LINKS;
/// The custom object type carrying one chunk of bytes.
pub const CHUNK_TYPE: &str = "mini/chunk";
/// Maximum content-type string bytes.
pub const MAX_CONTENT_TYPE_BYTES: usize = 128;
/// Maximum total payload one manifest may declare (allocation bound for
/// untrusted manifests).
pub const MAX_TOTAL_LEN: u64 = (MAX_CHUNKS * CHUNK_SIZE) as u64;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, MediaError>;

/// Why a media operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MediaError {
    /// The payload needs more than [`MAX_CHUNKS`] chunks.
    TooLarge,
    /// A field exceeded its limit.
    FieldTooLarge,
    /// The named object is not a valid manifest.
    BadManifest,
    /// A chunk object was not a valid chunk.
    BadChunk,
    /// Assembly produced bytes whose digest does not match the manifest.
    DigestMismatch,
    /// Chunks are still missing (see [`missing_chunks`]).
    Incomplete,
    /// Store failure.
    Store(StoreError),
    /// Object build failure.
    Object(mini_objects::ObjectError),
}

impl core::fmt::Display for MediaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MediaError::TooLarge => write!(f, "payload exceeds one manifest's capacity"),
            MediaError::FieldTooLarge => write!(f, "media field too large"),
            MediaError::BadManifest => write!(f, "not a valid media manifest"),
            MediaError::BadChunk => write!(f, "not a valid media chunk"),
            MediaError::DigestMismatch => write!(f, "assembled bytes do not match manifest digest"),
            MediaError::Incomplete => write!(f, "chunks missing"),
            MediaError::Store(e) => write!(f, "store: {e}"),
            MediaError::Object(e) => write!(f, "object: {e}"),
        }
    }
}
impl std::error::Error for MediaError {}
impl From<StoreError> for MediaError {
    fn from(e: StoreError) -> Self {
        MediaError::Store(e)
    }
}
impl From<mini_objects::ObjectError> for MediaError {
    fn from(e: mini_objects::ObjectError) -> Self {
        MediaError::Object(e)
    }
}

/// Parsed manifest metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The manifest object's id.
    pub id: ObjectId,
    /// MIME-style content type.
    pub content_type: String,
    /// Total payload length in bytes.
    pub total_len: u64,
    /// BLAKE3 digest of the whole payload.
    pub digest: [u8; 32],
    /// Ordered chunk ids.
    pub chunks: Vec<ObjectId>,
}

/// Split `bytes` into chunk objects plus one manifest, inserting everything
/// into the store. Returns the parsed manifest.
pub fn publish_media<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    content_type: &str,
    bytes: &[u8],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Manifest> {
    if content_type.len() > MAX_CONTENT_TYPE_BYTES {
        return Err(MediaError::FieldTooLarge);
    }
    let n_chunks = bytes.len().div_ceil(CHUNK_SIZE).max(1);
    if n_chunks > MAX_CHUNKS {
        return Err(MediaError::TooLarge);
    }

    let mut chunk_ids: Vec<ObjectId> = Vec::with_capacity(n_chunks);
    for (i, part) in bytes.chunks(CHUNK_SIZE).enumerate() {
        let chunk = ObjectBuilder::new(ObjectType::Custom(CHUNK_TYPE.to_string()))
            .timestamp_ms(timestamp_ms)
            .sequence(sequence.wrapping_add(i as u64))
            .payload(Payload::Public(part.to_vec()))
            .sign(human, device)?;
        store.insert(&chunk)?;
        chunk_ids.push(chunk.id().clone());
    }
    if bytes.is_empty() {
        // A single empty chunk keeps assembly uniform.
        let chunk = ObjectBuilder::new(ObjectType::Custom(CHUNK_TYPE.to_string()))
            .timestamp_ms(timestamp_ms)
            .sequence(sequence)
            .payload(Payload::Public(Vec::new()))
            .sign(human, device)?;
        store.insert(&chunk)?;
        chunk_ids.push(chunk.id().clone());
    }

    let digest = HashAlgorithm::Blake3.digest(bytes);
    let mut payload = Vec::new();
    payload.extend_from_slice(&(content_type.len() as u32).to_be_bytes());
    payload.extend_from_slice(content_type.as_bytes());
    payload.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    payload.extend_from_slice(&digest);

    let mut builder = ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload));
    for c in &chunk_ids {
        builder = builder.link("chunk", c.clone());
    }
    let manifest = builder.sign(human, device)?;
    store.insert(&manifest)?;

    Ok(Manifest {
        id: manifest.id().clone(),
        content_type: content_type.to_string(),
        total_len: bytes.len() as u64,
        digest,
        chunks: chunk_ids,
    })
}

/// Parse a manifest object.
pub fn read_manifest(obj: &Object) -> Result<Manifest> {
    if obj.object_type != ObjectType::MEDIA_MANIFEST {
        return Err(MediaError::BadManifest);
    }
    let b = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(MediaError::BadManifest),
    };
    if b.len() < 4 {
        return Err(MediaError::BadManifest);
    }
    let ct_len = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if ct_len > MAX_CONTENT_TYPE_BYTES || b.len() < 4 + ct_len + 8 + 32 {
        return Err(MediaError::BadManifest);
    }
    let content_type =
        String::from_utf8(b[4..4 + ct_len].to_vec()).map_err(|_| MediaError::BadManifest)?;
    let mut off = 4 + ct_len;
    let mut len8 = [0u8; 8];
    len8.copy_from_slice(&b[off..off + 8]);
    off += 8;
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&b[off..off + 32]);
    off += 32;
    // Strict: no trailing payload bytes.
    if off != b.len() {
        return Err(MediaError::BadManifest);
    }

    let chunks: Vec<ObjectId> = obj
        .links
        .iter()
        .filter(|l| l.rel == "chunk")
        .map(|l| l.target.clone())
        .collect();
    if chunks.is_empty() || chunks.len() > MAX_CHUNKS {
        return Err(MediaError::BadManifest);
    }
    let total_len = u64::from_be_bytes(len8);
    // Allocation bound: an untrusted manifest cannot demand more than its
    // chunks could possibly carry.
    if total_len > MAX_TOTAL_LEN || total_len > (chunks.len() as u64) * (CHUNK_SIZE as u64) {
        return Err(MediaError::BadManifest);
    }
    Ok(Manifest {
        id: obj.id().clone(),
        content_type,
        total_len,
        digest,
        chunks,
    })
}

/// Chunk ids from `manifest` not yet in the store — what to fetch next.
pub fn missing_chunks<B: Backend>(store: &Store<B>, manifest: &Manifest) -> Result<Vec<ObjectId>> {
    let mut out = Vec::new();
    for c in &manifest.chunks {
        if !store.contains(c)? {
            out.push(c.clone());
        }
    }
    Ok(out)
}

/// Assemble the full payload from the store, verifying length and whole-payload
/// digest. Returns [`MediaError::Incomplete`] while chunks are missing.
pub fn assemble<B: Backend>(store: &Store<B>, manifest: &Manifest) -> Result<Vec<u8>> {
    if !missing_chunks(store, manifest)?.is_empty() {
        return Err(MediaError::Incomplete);
    }
    // Bounded by validated total_len (read_manifest enforces the cap).
    let cap = manifest.total_len.min(MAX_TOTAL_LEN) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(cap);
    for c in &manifest.chunks {
        let obj = store.get(c)?;
        if obj.object_type != ObjectType::Custom(CHUNK_TYPE.to_string()) {
            return Err(MediaError::BadChunk);
        }
        match &obj.payload {
            Payload::Public(b) => {
                if b.len() > CHUNK_SIZE || out.len() + b.len() > cap {
                    // Early abort: chunks exceed what the manifest declared.
                    return Err(MediaError::DigestMismatch);
                }
                out.extend_from_slice(b);
            }
            Payload::Encrypted(_) => return Err(MediaError::BadChunk),
        }
    }
    if out.len() as u64 != manifest.total_len
        || HashAlgorithm::Blake3.digest(&out) != manifest.digest
    {
        return Err(MediaError::DigestMismatch);
    }
    Ok(out)
}

//! Self-contained bootstrap (`docs/BOOTSTRAP_AND_UPDATE.md`): the pieces that
//! let a brand-new device with **zero prior state** decide whether to trust,
//! and then pull, a genesis or update bundle — with no internet, no DNS, no
//! app store, no external service of any kind.
//!
//! ## The shape of a bootstrap exchange
//!
//! 1. A node broadcasts a tiny [`GenesisSeed`] (small enough for one BLE
//!    extended-advertising packet): a chain id, a [`PeerCard`], and a hash
//!    that pins the full [`CapsuleHeader`] object — so a receiver can verify
//!    *before* pulling anything larger that they're about to fetch the exact
//!    capsule the seed advertised, not a substitute.
//! 2. The receiver fetches the (still small) [`CapsuleHeader`] object and
//!    checks its hash against the seed.
//! 3. [`capsule_want_list`] says what to pull next — first the bundle
//!    manifest, then its chunks — exactly like `mini-sync`'s want-lists
//!    elsewhere, so the exchange is resumable across many short encounters
//!    (A3 store-and-forward) and never restarts from zero.
//! 4. Once complete, [`assemble_capsule`] reassembles and digest-verifies the
//!    bundle bytes (`mini-media`'s existing Merkle-manifest machinery — this
//!    crate does not reinvent chunking).
//!
//! ## What this batch implements, honestly
//!
//! [`CapsuleHeader`] is a signed, content-addressed object carrying
//! `chain_id`, `constitution_hash`, `schema_version`, and a link to the
//! bundle manifest — the load-bearing structural piece (self-certifying,
//! chunk-exchangeable, verifiable offline). The fuller genesis-file schema in
//! `docs/BOOTSTRAP_AND_UPDATE.md` (separate genesis-block hash, invariant
//! register hash, initial release-manifest CID *distinct* from the bootstrap
//! bundle, build-recipe hash, initial verifier KEL roots, rescue-bundle
//! hashes) is **not** all represented as distinct fields yet — that richer
//! schema is `pending`, the same honesty convention used throughout this
//! tree. Real BLE/local-Wi-Fi transport and the `MINI/BT0` handshake phases
//! are `mini-bearer`'s job and are also `pending`; this crate is
//! transport-agnostic and testable fully offline.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

use did_mini::{Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_media::{self, Manifest, MediaError};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Signed capsule-header object type.
pub const CAPSULE_TYPE: &str = "mini/genesis-capsule";
/// Maximum content-type string bytes for the wrapped bundle.
pub const MAX_CONTENT_TYPE_BYTES: usize = mini_media::MAX_CONTENT_TYPE_BYTES;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, BootstrapError>;

/// Why a bootstrap operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BootstrapError {
    /// A field exceeded its limit.
    FieldTooLarge,
    /// The object is not a structurally valid capsule header.
    BadCapsule,
    /// A seed's bytes were not a structurally valid [`GenesisSeed`].
    BadSeed,
    /// The fetched capsule header does not match what the seed advertised.
    SeedMismatch,
    /// `mini-media` failure (chunking/manifest/assembly).
    Media(MediaError),
    /// Store failure.
    Store(StoreError),
    /// Object build/decode failure.
    Object(mini_objects::ObjectError),
}

impl core::fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BootstrapError::FieldTooLarge => write!(f, "bootstrap field too large"),
            BootstrapError::BadCapsule => write!(f, "structurally invalid capsule header"),
            BootstrapError::BadSeed => write!(f, "structurally invalid genesis seed"),
            BootstrapError::SeedMismatch => {
                write!(
                    f,
                    "fetched capsule header does not match the advertised seed"
                )
            }
            BootstrapError::Media(e) => write!(f, "media: {e}"),
            BootstrapError::Store(e) => write!(f, "store: {e}"),
            BootstrapError::Object(e) => write!(f, "object: {e}"),
        }
    }
}
impl std::error::Error for BootstrapError {}
impl From<MediaError> for BootstrapError {
    fn from(e: MediaError) -> Self {
        BootstrapError::Media(e)
    }
}
impl From<StoreError> for BootstrapError {
    fn from(e: StoreError) -> Self {
        BootstrapError::Store(e)
    }
}
impl From<mini_objects::ObjectError> for BootstrapError {
    fn from(e: mini_objects::ObjectError) -> Self {
        BootstrapError::Object(e)
    }
}

/// What a capsule bundle claims to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapsuleKind {
    /// The genesis bootstrap capsule — the network's root of trust.
    Genesis,
    /// An update bundle for an already-bootstrapped device.
    Update,
}

impl CapsuleKind {
    fn to_byte(self) -> u8 {
        match self {
            CapsuleKind::Genesis => 0,
            CapsuleKind::Update => 1,
        }
    }
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(CapsuleKind::Genesis),
            1 => Some(CapsuleKind::Update),
            _ => None,
        }
    }
}

/// A compact, self-identifying peer card — phase 1 ("Advertise") of the
/// `MINI/BT0` bootstrap protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerCard {
    /// Bootstrap protocol version tag.
    pub protocol_tag: u8,
    /// First 4 bytes of the chain id.
    pub chain_id_prefix: [u8; 4],
    /// First 8 bytes of the advertised capsule hash.
    pub capsule_hash_prefix: [u8; 8],
    /// Hash of the advertising device's key (not the identity itself — just
    /// enough to deduplicate repeated advertisements from the same device).
    pub device_key_hash: [u8; 32],
}

const PEER_CARD_LEN: usize = 1 + 4 + 8 + 32;

impl PeerCard {
    /// Fixed-width wire encoding.
    pub fn to_bytes(&self) -> [u8; PEER_CARD_LEN] {
        let mut out = [0u8; PEER_CARD_LEN];
        out[0] = self.protocol_tag;
        out[1..5].copy_from_slice(&self.chain_id_prefix);
        out[5..13].copy_from_slice(&self.capsule_hash_prefix);
        out[13..45].copy_from_slice(&self.device_key_hash);
        out
    }

    /// Decode a fixed-width peer card.
    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() != PEER_CARD_LEN {
            return Err(BootstrapError::BadSeed);
        }
        let mut chain_id_prefix = [0u8; 4];
        chain_id_prefix.copy_from_slice(&b[1..5]);
        let mut capsule_hash_prefix = [0u8; 8];
        capsule_hash_prefix.copy_from_slice(&b[5..13]);
        let mut device_key_hash = [0u8; 32];
        device_key_hash.copy_from_slice(&b[13..45]);
        Ok(PeerCard {
            protocol_tag: b[0],
            chain_id_prefix,
            capsule_hash_prefix,
            device_key_hash,
        })
    }
}

const SEED_LEN: usize = 16 + 32 + PEER_CARD_LEN;

/// The tiny, first-broadcast bootstrap seed (`docs/BOOTSTRAP_AND_UPDATE.md`
/// "A tiny `GenesisSeed` may be transmitted first over BLE advertisements").
/// Small enough for one BLE extended-advertising packet (≤255 bytes); a
/// legacy 31-byte-payload variant would need further compaction and is
/// `pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenesisSeed {
    /// The chain this capsule belongs to.
    pub chain_id: [u8; 16],
    /// BLAKE3 digest of the full [`CapsuleHeader`] object's signed bytes —
    /// pins exactly which capsule this seed is advertising, checkable before
    /// a receiver commits to pulling the (much larger) bundle.
    pub capsule_hash: [u8; 32],
    /// The advertising peer's card.
    pub peer_card: PeerCard,
}

impl GenesisSeed {
    /// Fixed-width wire encoding.
    pub fn to_bytes(&self) -> [u8; SEED_LEN] {
        let mut out = [0u8; SEED_LEN];
        out[0..16].copy_from_slice(&self.chain_id);
        out[16..48].copy_from_slice(&self.capsule_hash);
        out[48..].copy_from_slice(&self.peer_card.to_bytes());
        out
    }

    /// Decode a fixed-width seed.
    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() != SEED_LEN {
            return Err(BootstrapError::BadSeed);
        }
        let mut chain_id = [0u8; 16];
        chain_id.copy_from_slice(&b[0..16]);
        let mut capsule_hash = [0u8; 32];
        capsule_hash.copy_from_slice(&b[16..48]);
        let peer_card = PeerCard::from_bytes(&b[48..])?;
        Ok(GenesisSeed {
            chain_id,
            capsule_hash,
            peer_card,
        })
    }
}

/// Parsed capsule-header metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleHeader {
    /// The header object's own content id.
    pub id: ObjectId,
    /// What this capsule claims to be.
    pub kind: CapsuleKind,
    /// The chain this capsule belongs to.
    pub chain_id: [u8; 16],
    /// Hash of the constitution this capsule was built against.
    pub constitution_hash: [u8; 32],
    /// Genesis/update schema version.
    pub schema_version: u32,
    /// The `mini-media` manifest carrying the bundle bytes.
    pub bundle_manifest: ObjectId,
}

/// Publish a genesis/update capsule: chunk+manifest the bundle bytes
/// (`mini-media`), then a small signed header committing to it. Returns the
/// parsed header.
#[allow(clippy::too_many_arguments)]
pub fn publish_capsule<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    kind: CapsuleKind,
    chain_id: [u8; 16],
    constitution_hash: [u8; 32],
    schema_version: u32,
    content_type: &str,
    bundle_bytes: &[u8],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<CapsuleHeader> {
    let manifest = mini_media::publish_media(
        store,
        human,
        device,
        content_type,
        bundle_bytes,
        timestamp_ms,
        sequence,
    )?;

    let mut payload = Vec::with_capacity(1 + 16 + 32 + 4);
    payload.push(kind.to_byte());
    payload.extend_from_slice(&chain_id);
    payload.extend_from_slice(&constitution_hash);
    payload.extend_from_slice(&schema_version.to_be_bytes());

    let header = ObjectBuilder::new(ObjectType::Custom(CAPSULE_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("bundle", manifest.id.clone())
        .sign(human, device)?;
    store.insert(&header)?;

    Ok(CapsuleHeader {
        id: header.id().clone(),
        kind,
        chain_id,
        constitution_hash,
        schema_version,
        bundle_manifest: manifest.id,
    })
}

/// Parse and structurally validate a capsule-header object.
pub fn read_capsule_header(obj: &Object) -> Result<CapsuleHeader> {
    if obj.object_type != ObjectType::Custom(CAPSULE_TYPE.to_string()) {
        return Err(BootstrapError::BadCapsule);
    }
    let b = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(BootstrapError::BadCapsule),
    };
    if b.len() != 1 + 16 + 32 + 4 {
        return Err(BootstrapError::BadCapsule);
    }
    let kind = CapsuleKind::from_byte(b[0]).ok_or(BootstrapError::BadCapsule)?;
    let mut chain_id = [0u8; 16];
    chain_id.copy_from_slice(&b[1..17]);
    let mut constitution_hash = [0u8; 32];
    constitution_hash.copy_from_slice(&b[17..49]);
    let schema_version = u32::from_be_bytes([b[49], b[50], b[51], b[52]]);

    let bundle_manifest = obj
        .links
        .iter()
        .find(|l| l.rel == "bundle")
        .ok_or(BootstrapError::BadCapsule)?
        .target
        .clone();
    if obj.links.len() != 1 {
        return Err(BootstrapError::BadCapsule); // strict: no unexplained extra links
    }

    Ok(CapsuleHeader {
        id: obj.id().clone(),
        kind,
        chain_id,
        constitution_hash,
        schema_version,
        bundle_manifest,
    })
}

/// The BLAKE3 digest of a capsule header's full signed bytes — what a
/// [`GenesisSeed::capsule_hash`] pins.
pub fn capsule_hash(header_obj: &Object) -> [u8; 32] {
    HashAlgorithm::Blake3.digest(&header_obj.to_bytes())
}

/// Build the seed a node would advertise for `header` (whose already-fetched
/// object bytes are `header_obj`).
pub fn seed_for(header_obj: &Object, header: &CapsuleHeader, peer_card: PeerCard) -> GenesisSeed {
    GenesisSeed {
        chain_id: header.chain_id,
        capsule_hash: capsule_hash(header_obj),
        peer_card,
    }
}

/// Verify a freshly-fetched capsule header object against a seed a peer
/// advertised earlier — the check a receiver runs *before* trusting the
/// header enough to start pulling the (much larger) bundle.
pub fn verify_header_matches_seed(header_obj: &Object, seed: &GenesisSeed) -> Result<()> {
    if capsule_hash(header_obj) != seed.capsule_hash {
        return Err(BootstrapError::SeedMismatch);
    }
    Ok(())
}

/// What to fetch next to complete `header`'s bundle: first the manifest
/// object itself, then — once it resolves — its missing chunks. Mirrors
/// `mini-store`'s and `mini-sync`'s want-list pattern so an interrupted
/// bootstrap resumes by idempotence, never restarting from zero.
pub fn capsule_want_list<B: Backend>(
    store: &Store<B>,
    header: &CapsuleHeader,
) -> Result<Vec<ObjectId>> {
    if !store.contains(&header.bundle_manifest)? {
        return Ok(vec![header.bundle_manifest.clone()]);
    }
    let manifest = resolve_manifest(store, header)?;
    Ok(mini_media::missing_chunks(store, &manifest)?)
}

/// Reassemble and digest-verify the bundle bytes. Fails with
/// [`MediaError::Incomplete`] (wrapped) while chunks are still missing — see
/// [`capsule_want_list`].
pub fn assemble_capsule<B: Backend>(store: &Store<B>, header: &CapsuleHeader) -> Result<Vec<u8>> {
    let manifest = resolve_manifest(store, header)?;
    Ok(mini_media::assemble(store, &manifest)?)
}

fn resolve_manifest<B: Backend>(store: &Store<B>, header: &CapsuleHeader) -> Result<Manifest> {
    let obj = store.get(&header.bundle_manifest)?;
    Ok(mini_media::read_manifest(&obj)?)
}

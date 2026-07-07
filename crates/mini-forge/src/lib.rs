//! The forge core (SPEC-11, UI plan E8): content-addressed repositories and the
//! release registry — the groundwork for building Mininet **from inside
//! Mininet**, with no external update authority.
//!
//! ## Repositories are objects
//!
//! Files ([`put_file`]), trees ([`put_tree`], nested), and commits
//! ([`commit`]) are ordinary signed, content-addressed objects, so code
//! replicates over `mini-sync` like any content, verifies offline, and needs no
//! hosting service. Branches are signed head pointers
//! (`subject = "branch.<name>"`), converging by the store's LWW rule. Trees
//! *link* their entries, so the sync want-list pulls a whole repo from a single
//! commit id. (Bit-exact git SHA-256 interop is a later, additive mapping.)
//!
//! ## The release registry encodes the guarantees
//!
//! A [`release`] names a version, its source commit, its artifact (a
//! `mini-media` manifest — the binary distributes through the network itself,
//! D-0020), and the digests of artifact and build recipe. The artifact itself
//! is checked by [`verify_release_artifact_only`]; **adoption** is gated by
//! [`verify_governed_release`], which additionally binds the source commit to
//! the governed canonical head (D-0030). Artifact-level checks:
//!
//! - **Independent reproducible-build attestations.** At least
//!   `min_attestations` **distinct verified identity roots** — one root's many
//!   devices count once, and the release author is excluded — must attest the
//!   artifact digest. Attestation is per verified identity root, never per
//!   balance: money never buys release authority (P1 / SPEC-11 [FREEZE]).
//!   (Identity root, not human: personhood is SPEC-02, pending — see
//!   [`oracle`].)
//! - **Timelock.** A release is not adoptable before
//!   `release.timestamp + timelock` — time to inspect, object, and fork.
//! - **Complete, digest-checked artifact.** The bytes must be fully present and
//!   match the attested digest before anything could be installed.
//! - **No forced update, no kill path [FREEZE].** This module only *verifies*;
//!   it contains no execution, no remote trigger, and no mechanism by which
//!   anyone can push code onto a device. Adoption is always the device owner's
//!   local act. (Until `mini-chain` lands, quorum finality is this attestation
//!   rule, labeled provisional — the chain replaces the counting, not the
//!   objects.)

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod governance;
mod oracle;
pub use governance::*;
pub use oracle::{IdentityOracle, KelDirectory};

use crate::oracle::author_verified;
use did_mini::{Controller, Did};
use mini_media::{assemble, read_manifest, Manifest};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// File blob object type (≤ envelope payload cap; larger files use media
/// manifests in a later batch).
pub const FILE_TYPE: &str = "mini/fblob";
/// Tree object type.
pub const TREE_TYPE: &str = "mini/tree";
/// Build attestation object type.
pub const ATTEST_TYPE: &str = "mini/attest";
/// Maximum entries per tree.
pub const MAX_TREE_ENTRIES: usize = 200;
/// Maximum tree recursion depth on checkout.
pub const MAX_TREE_DEPTH: usize = 32;
/// Maximum total files a single checkout may materialize (fan-out bound: depth
/// alone permits an exponential blow-up).
pub const MAX_CHECKOUT_FILES: usize = 100_000;
/// Maximum total bytes a single checkout may materialize.
pub const MAX_CHECKOUT_BYTES: usize = 256 * 1024 * 1024;
/// Maximum commit message bytes.
pub const MAX_MESSAGE_BYTES: usize = 4096;
/// Maximum version-string bytes.
pub const MAX_VERSION_BYTES: usize = 64;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ForgeError>;

/// Why a forge operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ForgeError {
    /// A field exceeded its limit.
    FieldTooLarge,
    /// A name (branch, tree entry) contained forbidden characters.
    BadName,
    /// The object is not what its type claims (malformed tree/commit/release…).
    BadObject,
    /// Tree nesting exceeded [`MAX_TREE_DEPTH`].
    TooDeep,
    /// The release's timelock has not elapsed.
    TimelockActive,
    /// Too few independent verified identity-root attestations.
    NotEnoughAttestations {
        /// Required distinct attesting identity roots (excluding the author).
        needed: u32,
        /// Found.
        got: u32,
    },
    /// The artifact is missing chunks or fails its digest.
    ArtifactUnavailable,
    /// The governance chain shows competing valid entries; adoption refuses to
    /// pick a side (chain finality resolves this later).
    ForkDetected,
    /// The release's source commit is not the canonical governed branch head
    /// (or the branch does not exist in governance).
    NotCanonical,
    /// Store failure.
    Store(StoreError),
    /// Object build failure.
    Object(mini_objects::ObjectError),
    /// Media failure.
    Media(mini_media::MediaError),
    /// Identity/KEL verification failure.
    Identity(did_mini::IdentityError),
}

impl core::fmt::Display for ForgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ForgeError::FieldTooLarge => write!(f, "forge field too large"),
            ForgeError::BadName => write!(f, "invalid name"),
            ForgeError::BadObject => write!(f, "malformed forge object"),
            ForgeError::TooDeep => write!(f, "tree nesting too deep"),
            ForgeError::TimelockActive => write!(f, "release timelock has not elapsed"),
            ForgeError::NotEnoughAttestations { needed, got } => {
                write!(f, "need {needed} independent attestations, got {got}")
            }
            ForgeError::ArtifactUnavailable => write!(f, "artifact incomplete or digest mismatch"),
            ForgeError::ForkDetected => write!(f, "governance fork detected; refusing adoption"),
            ForgeError::NotCanonical => {
                write!(
                    f,
                    "release source commit is not the canonical governed head"
                )
            }
            ForgeError::Store(e) => write!(f, "store: {e}"),
            ForgeError::Object(e) => write!(f, "object: {e}"),
            ForgeError::Media(e) => write!(f, "media: {e}"),
            ForgeError::Identity(e) => write!(f, "identity: {e}"),
        }
    }
}
impl std::error::Error for ForgeError {}
impl From<StoreError> for ForgeError {
    fn from(e: StoreError) -> Self {
        ForgeError::Store(e)
    }
}
impl From<mini_objects::ObjectError> for ForgeError {
    fn from(e: mini_objects::ObjectError) -> Self {
        ForgeError::Object(e)
    }
}
impl From<mini_media::MediaError> for ForgeError {
    fn from(e: mini_media::MediaError) -> Self {
        ForgeError::Media(e)
    }
}
impl From<did_mini::IdentityError> for ForgeError {
    fn from(e: did_mini::IdentityError) -> Self {
        ForgeError::Identity(e)
    }
}

pub(crate) fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && s != "."
        && s != ".."
}

/// One tree entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Entry name (path segment).
    pub name: String,
    /// Whether the target is a subtree (else a file blob).
    pub is_dir: bool,
    /// The target object id.
    pub target: ObjectId,
}

/// Store a file's bytes as a blob object; returns its id.
pub fn put_file<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    bytes: &[u8],
) -> Result<ObjectId> {
    let obj = ObjectBuilder::new(ObjectType::Custom(FILE_TYPE.to_string()))
        .payload(Payload::Public(bytes.to_vec()))
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj.id().clone())
}

/// Store a tree of entries; returns its id. Entries are canonically sorted by
/// name, so the same content always yields the same tree id.
pub fn put_tree<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    entries: &[TreeEntry],
) -> Result<ObjectId> {
    if entries.len() > MAX_TREE_ENTRIES {
        return Err(ForgeError::FieldTooLarge);
    }
    let mut sorted: Vec<&TreeEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    // Canonical trees: names are unique — two entries with one name would make
    // checkout ambiguous.
    if sorted.windows(2).any(|w| w[0].name == w[1].name) {
        return Err(ForgeError::BadName);
    }

    let mut payload = Vec::new();
    payload.extend_from_slice(&(sorted.len() as u32).to_be_bytes());
    let mut builder = ObjectBuilder::new(ObjectType::Custom(TREE_TYPE.to_string()));
    for e in &sorted {
        if !valid_name(&e.name) {
            return Err(ForgeError::BadName);
        }
        payload.push(u8::from(e.is_dir));
        payload.extend_from_slice(&(e.name.len() as u32).to_be_bytes());
        payload.extend_from_slice(e.name.as_bytes());
        payload.extend_from_slice(&(e.target.as_str().len() as u32).to_be_bytes());
        payload.extend_from_slice(e.target.as_str().as_bytes());
        builder = builder.link("entry", e.target.clone());
    }
    let obj = builder
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj.id().clone())
}

fn read_tree(obj: &Object) -> Result<Vec<TreeEntry>> {
    if obj.object_type != ObjectType::Custom(TREE_TYPE.to_string()) {
        return Err(ForgeError::BadObject);
    }
    let b = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(ForgeError::BadObject),
    };
    if b.len() < 4 {
        return Err(ForgeError::BadObject);
    }
    let n = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if n > MAX_TREE_ENTRIES {
        return Err(ForgeError::BadObject);
    }
    let mut off = 4usize;
    let mut out: Vec<TreeEntry> = Vec::with_capacity(n);
    for _ in 0..n {
        if off + 1 + 4 > b.len() {
            return Err(ForgeError::BadObject);
        }
        let is_dir = match b[off] {
            0 => false,
            1 => true,
            _ => return Err(ForgeError::BadObject), // strict flag byte
        };
        off += 1;
        let name = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
        if !valid_name(&name) {
            return Err(ForgeError::BadObject);
        }
        // Canonical order: strictly ascending names (also rules out duplicates
        // and non-canonical encodings from hostile peers).
        if let Some(prev) = out.last() {
            if prev.name.as_str() >= name.as_str() {
                return Err(ForgeError::BadObject);
            }
        }
        let id_str = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
        out.push(TreeEntry {
            name,
            is_dir,
            target: ObjectId::parse(&id_str)?,
        });
    }
    if off != b.len() {
        return Err(ForgeError::BadObject); // strict: no trailing bytes
    }
    Ok(out)
}

/// Create a commit: message + one tree + any parents.
#[allow(clippy::too_many_arguments)]
pub fn commit<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    message: &str,
    tree: &ObjectId,
    parents: &[ObjectId],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(ForgeError::FieldTooLarge);
    }
    let mut builder = ObjectBuilder::new(ObjectType::COMMIT)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(message.as_bytes().to_vec()))
        .link("tree", tree.clone());
    for p in parents {
        builder = builder.link("parent", p.clone());
    }
    let obj = builder.sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Move a branch head to `commit_id` (signed head pointer, LWW convergence).
pub fn set_branch<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    name: &str,
    commit_id: &ObjectId,
    sequence: u64,
) -> Result<()> {
    if !valid_name(name) {
        return Err(ForgeError::BadName);
    }
    let head = ObjectBuilder::new(ObjectType::HEAD)
        .sequence(sequence)
        .link("target", commit_id.clone())
        .payload(Payload::Public(format!("branch.{name}").into_bytes()))
        .sign(human, device)?;
    store.apply_head(&head)?;
    Ok(())
}

/// Resolve `human`'s branch `name` to a commit id.
pub fn resolve_branch<B: Backend>(
    store: &Store<B>,
    human: &Did,
    name: &str,
) -> Result<Option<ObjectId>> {
    if !valid_name(name) {
        return Err(ForgeError::BadName);
    }
    Ok(store.resolve_head(human, &format!("branch.{name}"))?)
}

/// Materialize the tree of `commit_id` as `(path, bytes)` pairs, recursing into
/// subtrees (depth-capped).
pub fn checkout<B: Backend>(
    store: &Store<B>,
    commit_id: &ObjectId,
) -> Result<Vec<(String, Vec<u8>)>> {
    let c = store.get(commit_id)?;
    if c.object_type != ObjectType::COMMIT {
        return Err(ForgeError::BadObject);
    }
    let tree = c
        .links
        .iter()
        .find(|l| l.rel == "tree")
        .ok_or(ForgeError::BadObject)?
        .target
        .clone();
    let mut out = Vec::new();
    let mut budget = Budget {
        files: MAX_CHECKOUT_FILES,
        bytes: MAX_CHECKOUT_BYTES,
        seen: Vec::new(),
    };
    walk(store, &tree, "", 0, &mut out, &mut budget)?;
    Ok(out)
}

/// Work budget for a single checkout (guards fan-out and cycles).
struct Budget {
    files: usize,
    bytes: usize,
    seen: Vec<String>,
}

fn walk<B: Backend>(
    store: &Store<B>,
    tree_id: &ObjectId,
    prefix: &str,
    depth: usize,
    out: &mut Vec<(String, Vec<u8>)>,
    budget: &mut Budget,
) -> Result<()> {
    if depth > MAX_TREE_DEPTH {
        return Err(ForgeError::TooDeep);
    }
    // Cycle guard: a tree that (transitively) links itself is refused, not
    // followed forever.
    let key = tree_id.as_str().to_string();
    if budget.seen.contains(&key) {
        return Err(ForgeError::BadObject);
    }
    budget.seen.push(key);

    for e in read_tree(&store.get(tree_id)?)? {
        let path = if prefix.is_empty() {
            e.name.clone()
        } else {
            format!("{prefix}/{}", e.name)
        };
        if e.is_dir {
            walk(store, &e.target, &path, depth + 1, out, budget)?;
        } else {
            let blob = store.get(&e.target)?;
            if blob.object_type != ObjectType::Custom(FILE_TYPE.to_string()) {
                return Err(ForgeError::BadObject);
            }
            match &blob.payload {
                Payload::Public(b) => {
                    if budget.files == 0 || b.len() > budget.bytes {
                        return Err(ForgeError::FieldTooLarge);
                    }
                    budget.files -= 1;
                    budget.bytes -= b.len();
                    out.push((path, b.clone()));
                }
                Payload::Encrypted(_) => return Err(ForgeError::BadObject),
            }
        }
    }
    budget.seen.pop();
    Ok(())
}

/// Publish a release: version, the governed project and branch it claims,
/// source commit, artifact manifest, and the digests of artifact bytes and
/// build recipe. Whether that claim is TRUE is judged by
/// [`verify_governed_release`], never assumed.
#[allow(clippy::too_many_arguments)]
pub fn release<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    version: &str,
    project_id: &ObjectId,
    branch: &str,
    source_commit: &ObjectId,
    artifact_manifest: &ObjectId,
    artifact_digest: [u8; 32],
    recipe_digest: [u8; 32],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if version.len() > MAX_VERSION_BYTES || branch.len() > MAX_VERSION_BYTES {
        return Err(ForgeError::FieldTooLarge);
    }
    if !valid_name(branch) {
        return Err(ForgeError::BadName);
    }
    let mut payload = Vec::new();
    payload.extend_from_slice(&(version.len() as u32).to_be_bytes());
    payload.extend_from_slice(version.as_bytes());
    payload.extend_from_slice(&(branch.len() as u32).to_be_bytes());
    payload.extend_from_slice(branch.as_bytes());
    payload.extend_from_slice(&artifact_digest);
    payload.extend_from_slice(&recipe_digest);
    let obj = ObjectBuilder::new(ObjectType::RELEASE)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .link("project", project_id.clone())
        .link("commit", source_commit.clone())
        .link("artifact", artifact_manifest.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Parsed release payload: (version, branch, artifact digest, recipe digest).
fn parse_release_payload(rel: &Object) -> Result<(String, String, [u8; 32], [u8; 32])> {
    let b = match &rel.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(ForgeError::BadObject),
    };
    let mut off = 0usize;
    let version = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let branch = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    if version.len() > MAX_VERSION_BYTES || version.is_empty() {
        return Err(ForgeError::BadObject);
    }
    if !valid_name(&branch) || branch.len() > MAX_VERSION_BYTES {
        return Err(ForgeError::BadObject);
    }
    if b.len() != off + 64 {
        return Err(ForgeError::BadObject); // strict: no trailing bytes
    }
    let mut artifact = [0u8; 32];
    artifact.copy_from_slice(&b[off..off + 32]);
    let mut recipe = [0u8; 32];
    recipe.copy_from_slice(&b[off + 32..off + 64]);
    Ok((version, branch, artifact, recipe))
}

/// Attest that an independent build of `release_id` reproduced
/// `artifact_digest` bit-for-bit. One attestation per verified identity root counts —
/// devices are irrelevant, balances are nonexistent.
pub fn attest<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    release_id: &ObjectId,
    artifact_digest: [u8; 32],
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let obj = ObjectBuilder::new(ObjectType::Custom(ATTEST_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(artifact_digest.to_vec()))
        .link("release", release_id.clone())
        .sign(human, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

/// Adoption policy for a release.
#[derive(Debug, Clone)]
pub struct ReleasePolicy {
    /// Distinct attesting identity roots required, excluding the release
    /// author (personhood, SPEC-02, later upgrades roots to humans).
    pub min_attestations: u32,
    /// Time that must elapse after the release's timestamp.
    pub timelock_ms: u64,
    /// The verifier's current time.
    pub now_ms: u64,
}

/// Floor: adoption never accepts fewer independent attestations than this.
pub const ADOPTION_MIN_ATTESTATIONS: u32 = 2;
/// Floor: adoption never accepts a shorter timelock than this (1 hour —
/// provisional; on-chain governance sets the real value later, upward only).
pub const ADOPTION_MIN_TIMELOCK_MS: u64 = 3_600_000;

impl ReleasePolicy {
    /// Enforce the adoption floors: a caller cannot weaken adoption below the
    /// frozen minimums (no zero-attestation or zero-timelock adoption, ever).
    pub fn validate_for_adoption(&self) -> Result<()> {
        if self.min_attestations < ADOPTION_MIN_ATTESTATIONS
            || self.timelock_ms < ADOPTION_MIN_TIMELOCK_MS
        {
            return Err(ForgeError::BadObject);
        }
        Ok(())
    }
}

/// A release that passed every gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedRelease {
    /// The release object's id.
    pub id: ObjectId,
    /// Version string.
    pub version: String,
    /// The verified artifact manifest.
    pub artifact: Manifest,
    /// Distinct independent attesting identity roots found.
    pub attesters: u32,
}

/// Verify a release's ARTIFACT layer only: author re-binding, timelock,
/// independent attestations, and a complete digest-checked artifact.
///
/// **NOT sufficient for adoption.** This does not prove the source commit went
/// through merge governance. The only adoption-safe gate is
/// [`verify_governed_release`], which adds the governance lineage. Nothing
/// here executes, fetches, or installs anything. [FREEZE: no forced update,
/// no kill path]
pub fn verify_release_artifact_only<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    release_id: &ObjectId,
    policy: &ReleasePolicy,
) -> Result<VerifiedRelease> {
    let rel = store.get(release_id)?;
    if rel.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }
    // The release object itself must re-pass provenance against a trusted KEL.
    if !author_verified(oracle, &rel) {
        return Err(ForgeError::BadObject);
    }
    let (version, _branch, artifact_digest, _recipe) = parse_release_payload(&rel)?;

    // Timelock first: no adoption inside the inspection window.
    if policy.now_ms < rel.timestamp_ms.saturating_add(policy.timelock_ms) {
        return Err(ForgeError::TimelockActive);
    }

    // Independent attestations: distinct identity roots, author excluded, digest match.
    let author_scid = rel.author_human.scid().to_string();
    let mut attesters: Vec<String> = Vec::new();
    for id in store.linking_to(release_id)? {
        let a = store.get(&id)?;
        if a.object_type != ObjectType::Custom(ATTEST_TYPE.to_string()) {
            continue;
        }
        let digest_ok = matches!(&a.payload, Payload::Public(p) if p.as_slice() == artifact_digest);
        if !digest_ok {
            continue;
        }
        // Re-bind: the attestation's author must be a verified identity root right now,
        // not merely claimed on a stored object.
        if !author_verified(oracle, &a) {
            continue;
        }
        let scid = a.author_human.scid().to_string();
        if scid == author_scid || attesters.contains(&scid) {
            continue; // author excluded; one identity root counts once
        }
        attesters.push(scid);
    }
    if (attesters.len() as u32) < policy.min_attestations {
        return Err(ForgeError::NotEnoughAttestations {
            needed: policy.min_attestations,
            got: attesters.len() as u32,
        });
    }

    // The artifact must be complete and match the attested digest.
    let manifest_link = rel
        .links
        .iter()
        .find(|l| l.rel == "artifact")
        .ok_or(ForgeError::BadObject)?;
    let manifest = read_manifest(&store.get(&manifest_link.target)?)?;
    if manifest.digest != artifact_digest {
        return Err(ForgeError::ArtifactUnavailable);
    }
    match assemble(store, &manifest) {
        Ok(_) => {}
        Err(_) => return Err(ForgeError::ArtifactUnavailable),
    }

    Ok(VerifiedRelease {
        id: release_id.clone(),
        version,
        artifact: manifest,
        attesters: attesters.len() as u32,
    })
}

/// The full "built from inside MINI" validity chain, machine-enforced:
///
/// ```text
/// release artifact
///   → complete + digest-checked                       (verify_release_artifact_only)
///   → attested by ≥N independent verified id roots     (verify_release_artifact_only)
///   → timelocked                                       (verify_release_artifact_only)
///   → source commit == canonical head of `branch`     (this function)
///   → canonical head exists via valid merge quorums   (resolve_project)
///   → quorums counted in distinct verified identity
///     roots under the policy as of each chain position (resolve_project)
/// ```
///
/// Refuses on any governance fork (`ForkDetected`) rather than picking a side
/// for an *adoption* decision — display may use the provisional tiebreak;
/// installing software may not. Nothing here executes or installs anything:
/// adoption remains the device owner's local act [FREEZE: no forced update].
pub fn verify_governed_release<B: Backend>(
    store: &Store<B>,
    oracle: &dyn IdentityOracle,
    release_id: &ObjectId,
    project_id: &ObjectId,
    branch: &str,
    policy: &ReleasePolicy,
) -> Result<VerifiedRelease> {
    // 0. Adoption floors: callers cannot weaken the gate below the frozen
    //    minimums.
    policy.validate_for_adoption()?;

    // 1. Resolve governance; refuse forks outright for adoption.
    let state = crate::resolve_project(store, oracle, project_id)?;
    if state.forks_detected {
        return Err(ForgeError::ForkDetected);
    }
    let canonical = state
        .branches
        .iter()
        .find(|(b, _)| b == branch)
        .map(|(_, c)| c.clone())
        .ok_or(ForgeError::NotCanonical)?;

    // 2. The release must claim exactly this project and branch...
    let rel = store.get(release_id)?;
    if rel.object_type != ObjectType::RELEASE {
        return Err(ForgeError::BadObject);
    }
    let claimed_project = rel
        .links
        .iter()
        .find(|l| l.rel == "project")
        .map(|l| &l.target);
    if claimed_project != Some(project_id) {
        return Err(ForgeError::BadObject);
    }
    let (_, claimed_branch, _, _) = parse_release_payload(&rel)?;
    if claimed_branch != branch {
        return Err(ForgeError::BadObject);
    }

    // 3. ...and its source commit must BE the governed canonical head.
    let source = rel
        .links
        .iter()
        .find(|l| l.rel == "commit")
        .map(|l| l.target.clone())
        .ok_or(ForgeError::BadObject)?;
    if source != canonical {
        return Err(ForgeError::NotCanonical);
    }

    // 4. Everything else: author re-binding, timelock, independent
    //    attestations, complete digest-checked artifact.
    verify_release_artifact_only(store, oracle, release_id, policy)
}

pub(crate) fn take_str(b: &[u8], off: &mut usize) -> Option<String> {
    if *off + 4 > b.len() {
        return None;
    }
    let len = u32::from_be_bytes([b[*off], b[*off + 1], b[*off + 2], b[*off + 3]]) as usize;
    *off += 4;
    if *off + len > b.len() || len > 4096 {
        return None;
    }
    let s = String::from_utf8(b[*off..*off + len].to_vec()).ok()?;
    *off += len;
    Some(s)
}

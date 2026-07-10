//! Git SHA-256-object-format export bridge — D-0004 retained SHA-256
//! specifically "for the SHA-256 Git-object interop path," and the
//! self-hosted-forge-spine design doc names this real work as still
//! pending. Exports a mini-forge commit chain (commit → tree → blobs,
//! recursively through every ancestor) as real git objects in git's
//! SHA-256 object format (`git init --object-format=sha256`) — the exact
//! same `"<kind> <len>\0<body>"` framing and SHA-256 hashing real `git`
//! computes, verified in this crate's own tests against the actual `git`
//! binary (`git hash-object`, `git cat-file`), not just self-consistency.
//!
//! ## Scope: export only, one direction
//!
//! This is genuinely one-directional: mini-forge → git objects. Import
//! (parsing an arbitrary git repository into mini-forge's own signed
//! object model) is **not** implemented here — verifying and re-signing
//! untrusted git history against this tree's identity model is a
//! materially different problem than emitting bytes from objects this
//! store already trusts, and calling this a complete "bridge" would
//! overclaim what exists.
//!
//! ## Honest lossiness
//!
//! Git commits require an author *name* and *email*; mini-forge commits
//! only ever carry a `did:mini` author. This module synthesizes a
//! deterministic, clearly-non-routable identity
//! (`mini:<scid> <<scid>@mininet.invalid>`) rather than inventing or
//! requiring a real email — `.invalid` is the top-level domain RFC 2606
//! reserves for exactly this "not a real address" purpose. Git also wants
//! a UTC offset per timestamp; mini-forge tracks no timezone, so every
//! exported commit is stamped `+0000`. File mode is always `100644`
//! (regular, non-executable) — mini-forge's tree model has no executable
//! bit to translate.

use std::collections::BTreeMap;

use did_mini::Did;
use mini_objects::{ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::{read_tree, ForgeError, Result, FILE_TYPE};

/// Hard ceiling on how many ancestor commits one export walks — the same
/// hostile-input bound every other recursive walk in this tree uses
/// (`checkout`'s depth budget, `resolve_project`'s chain walk).
pub const MAX_EXPORT_COMMITS: usize = 100_000;

/// One real git object in git's SHA-256 object format. `bytes` is the
/// exact framing git itself hashes; a caller writing a real on-disk git
/// object database must zlib-deflate `bytes` (git compresses at rest, but
/// the id and the framing are always over the *uncompressed* bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitObject {
    /// Git's SHA-256 object id, lowercase hex (64 characters).
    pub id: String,
    pub kind: GitObjectKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GitObjectKind {
    Blob,
    Tree,
    Commit,
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(ForgeError::BadObject);
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for chunk in bytes.chunks(2) {
        let hi = (chunk[0] as char)
            .to_digit(16)
            .ok_or(ForgeError::BadObject)?;
        let lo = (chunk[1] as char)
            .to_digit(16)
            .ok_or(ForgeError::BadObject)?;
        out.push(((hi << 4) | lo) as u8);
    }
    Ok(out)
}

fn frame(kind: &str, body: &[u8]) -> GitObject {
    let mut framed = Vec::with_capacity(kind.len() + 1 + 20 + body.len());
    framed.extend_from_slice(kind.as_bytes());
    framed.push(b' ');
    framed.extend_from_slice(body.len().to_string().as_bytes());
    framed.push(0);
    framed.extend_from_slice(body);
    let digest = mini_crypto::HashAlgorithm::Sha256.digest(&framed);
    let git_kind = match kind {
        "blob" => GitObjectKind::Blob,
        "tree" => GitObjectKind::Tree,
        "commit" => GitObjectKind::Commit,
        _ => unreachable!("frame() is only ever called with a fixed, known kind"),
    };
    GitObject {
        id: hex_encode(&digest),
        kind: git_kind,
        bytes: framed,
    }
}

/// Git's tree entry sort order treats a directory as if its name had a
/// trailing `/` appended for comparison purposes only (the name stored in
/// the entry itself has no slash) — this differs from a plain string sort
/// whenever one entry's name is a strict prefix of a sibling's (e.g. the
/// directory `"abc"` vs. the file `"abc-file"`: under git's rule
/// `"abc-file" < "abc/"` because `-` (0x2D) sorts before `/` (0x2F), so the
/// file comes first, the reverse of a naive string comparison of `"abc"`
/// vs. `"abc-file"`).
fn git_tree_sort_key(name: &str, is_dir: bool) -> Vec<u8> {
    let mut key = name.as_bytes().to_vec();
    if is_dir {
        key.push(b'/');
    }
    key
}

fn export_file<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
    objects: &mut BTreeMap<String, GitObject>,
) -> Result<String> {
    let obj = store.get(id)?;
    if obj.object_type != ObjectType::Custom(FILE_TYPE.to_string()) {
        return Err(ForgeError::BadObject);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b.clone(),
        Payload::Encrypted(_) => return Err(ForgeError::BadObject),
    };
    let git_obj = frame("blob", &bytes);
    let git_id = git_obj.id.clone();
    objects.entry(git_id.clone()).or_insert(git_obj);
    Ok(git_id)
}

fn export_tree<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
    objects: &mut BTreeMap<String, GitObject>,
) -> Result<String> {
    let obj = store.get(id)?;
    let mut entries = read_tree(&obj)?;
    entries.sort_by_key(|e| git_tree_sort_key(&e.name, e.is_dir));

    let mut body = Vec::new();
    for e in &entries {
        let child_git_id = if e.is_dir {
            export_tree(store, &e.target, objects)?
        } else {
            export_file(store, &e.target, objects)?
        };
        let mode = if e.is_dir { "40000" } else { "100644" };
        body.extend_from_slice(mode.as_bytes());
        body.push(b' ');
        body.extend_from_slice(e.name.as_bytes());
        body.push(0);
        body.extend_from_slice(&hex_decode(&child_git_id)?);
    }
    let git_obj = frame("tree", &body);
    let git_id = git_obj.id.clone();
    objects.entry(git_id.clone()).or_insert(git_obj);
    Ok(git_id)
}

/// The synthesized, clearly-non-routable git author identity for a
/// `did:mini` author — see this module's own "Honest lossiness" doc.
fn synthetic_identity(human: &Did) -> (String, String) {
    let scid = human.scid();
    (format!("mini:{scid}"), format!("{scid}@mininet.invalid"))
}

fn export_commit_rec<B: Backend>(
    store: &Store<B>,
    id: &ObjectId,
    objects: &mut BTreeMap<String, GitObject>,
    exported: &mut BTreeMap<String, String>,
    budget: &mut usize,
) -> Result<String> {
    if let Some(existing) = exported.get(id.as_str()) {
        return Ok(existing.clone());
    }
    *budget = budget.checked_sub(1).ok_or(ForgeError::FieldTooLarge)?;

    let obj = store.get(id)?;
    if obj.object_type != ObjectType::COMMIT {
        return Err(ForgeError::BadObject);
    }
    let tree_link = obj
        .links
        .iter()
        .find(|l| l.rel == "tree")
        .ok_or(ForgeError::BadObject)?;
    let tree_git_id = export_tree(store, &tree_link.target, objects)?;

    let mut parent_git_ids = Vec::new();
    for link in obj.links.iter().filter(|l| l.rel == "parent") {
        parent_git_ids.push(export_commit_rec(
            store,
            &link.target,
            objects,
            exported,
            budget,
        )?);
    }

    let message = match &obj.payload {
        Payload::Public(b) => String::from_utf8(b.clone()).map_err(|_| ForgeError::BadObject)?,
        Payload::Encrypted(_) => return Err(ForgeError::BadObject),
    };
    let (name, email) = synthetic_identity(&obj.author_human);
    let ts_secs = obj.timestamp_ms / 1000;

    let mut body = Vec::new();
    body.extend_from_slice(format!("tree {tree_git_id}\n").as_bytes());
    for p in &parent_git_ids {
        body.extend_from_slice(format!("parent {p}\n").as_bytes());
    }
    body.extend_from_slice(format!("author {name} <{email}> {ts_secs} +0000\n").as_bytes());
    body.extend_from_slice(format!("committer {name} <{email}> {ts_secs} +0000\n").as_bytes());
    body.push(b'\n');
    body.extend_from_slice(message.trim_end_matches('\n').as_bytes());
    body.push(b'\n');

    let git_obj = frame("commit", &body);
    let git_id = git_obj.id.clone();
    objects.entry(git_id.clone()).or_insert(git_obj);
    exported.insert(id.as_str().to_string(), git_id.clone());
    Ok(git_id)
}

/// Export `commit_id` and its full ancestor chain as real git SHA-256
/// objects. Returns the exported commit's git object id, plus every git
/// object (blobs, trees, commits, deduplicated by git id) needed to make
/// that commit resolvable in a real `git init --object-format=sha256`
/// repository.
pub fn export_commit_chain<B: Backend>(
    store: &Store<B>,
    commit_id: &ObjectId,
) -> Result<(String, Vec<GitObject>)> {
    let mut objects = BTreeMap::new();
    let mut exported = BTreeMap::new();
    let mut budget = MAX_EXPORT_COMMITS;
    let git_id = export_commit_rec(store, commit_id, &mut objects, &mut exported, &mut budget)?;
    Ok((git_id, objects.into_values().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 15, 16, 255, 128, 7];
        let hex = hex_encode(&bytes);
        assert_eq!(hex_decode(&hex).unwrap(), bytes);
    }

    #[test]
    fn hex_decode_rejects_odd_length_and_non_hex() {
        assert!(hex_decode("abc").is_err()); // odd length
        assert!(hex_decode("zz").is_err()); // not hex digits
    }

    #[test]
    fn blob_framing_matches_a_known_git_sha256_vector() {
        // Independently confirmed against a real `git init
        // --object-format=sha256` repository: `git hash-object -t blob
        // --stdin -w` on the literal bytes "hello world" produces exactly
        // this id. A fixed known-answer test, so this doesn't depend on
        // `git` being installed to catch a framing regression.
        let git_obj = frame("blob", b"hello world");
        assert_eq!(
            git_obj.id,
            "fee53a18d32820613c0527aa79be5cb30173c823a9b448fa4817767cc84c6f03"
        );
        assert_eq!(git_obj.kind, GitObjectKind::Blob);
        assert_eq!(git_obj.bytes, b"blob 11\0hello world");
    }

    #[test]
    fn git_tree_sort_places_a_prefix_file_before_its_sibling_directory() {
        // Git's actual rule: compare as if directory names had a trailing
        // `/`. `"abc-file"` (a file) sorts *before* `"abc"` (a directory)
        // because `-` (0x2D) < `/` (0x2F) -- the reverse of a naive plain
        // string comparison of `"abc"` vs. `"abc-file"`, where the shorter
        // string (the bare directory name) would sort first instead.
        assert!(git_tree_sort_key("abc-file", false) < git_tree_sort_key("abc", true));
    }
}

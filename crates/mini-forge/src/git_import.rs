//! Git SHA-256 import bridge (D-0418) — the import half of the bridge
//! `git_export.rs` only ever exported.
//!
//! See `docs/design/git-import-bridge.md` for the doctrine this module
//! implements. In short: `mini-forge` requires every object to carry a
//! real `did:mini` signature, but real git history carries none at all,
//! so imported content is re-signed by the **importer**, never spoofed as
//! the original git author. Blobs and trees are reconstructed via the
//! existing [`crate::put_file`]/[`crate::put_tree`] with content bytes
//! preserved exactly. Commits are built via the existing, unmodified
//! [`crate::commit`] — same strict shape `checkout()` already enforces —
//! so an imported commit is indistinguishable from a native one except for
//! its signed author. The original git commit's id, author, and committer
//! are recorded separately in a [`GitImportProvenance`] object that links
//! to the commit, never smuggled onto the commit's own payload or links.
//!
//! ## Scope
//!
//! Consumes already-parsed [`crate::GitObject`]s — the same shape
//! [`crate::export_commit_chain`] produces — so this module composes with
//! export's own fixtures directly. It does not itself walk a real git
//! object database or fetch anything over a network; that driver is later,
//! separate work. Only the canonical commit shape `git_export.rs` itself
//! writes is accepted (`tree`/`parent*`/`author`/`committer`/blank
//! line/message); any other header line (`gpgsig`, `encoding`, ...) is
//! rejected outright, not silently dropped. Only regular files (git mode
//! `100644`) and directories (`40000`) are supported — executable bits,
//! symlinks, and submodules are rejected, the same lossiness
//! `git_export.rs` already documents for the export direction.

use std::collections::BTreeMap;

use did_mini::{Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::git_export::hex_encode;
use crate::{
    commit, put_file, put_tree, take_str, valid_name, ForgeError, GitObject, GitObjectKind, Result,
    TreeEntry, MAX_TREE_ENTRIES,
};

/// The custom object type carrying a [`GitImportProvenance`] record.
pub const GIT_IMPORT_PROVENANCE_TYPE: &str = "mini/git-import-provenance";

/// Hard ceiling on how many ancestor commits one import walks — mirrors
/// `git_export::MAX_EXPORT_COMMITS`.
pub const MAX_IMPORT_COMMITS: usize = 100_000;

/// Raw SHA-256 git object ids are 32 bytes (this bridge, like
/// `git_export.rs`, only ever speaks git's SHA-256 object format).
const RAW_GIT_ID_LEN: usize = 32;

fn verify_object_id(object: &GitObject) -> Result<()> {
    let digest = HashAlgorithm::Sha256.digest(&object.bytes);
    if hex_encode(&digest) != object.id {
        return Err(ForgeError::BadObject);
    }
    Ok(())
}

/// Strip a git object's `"<kind> <len>\0"` framing, verifying `kind` and
/// the declared length against the actual body.
fn parse_framed<'a>(bytes: &'a [u8], expected_kind: &str) -> Result<&'a [u8]> {
    let space = bytes
        .iter()
        .position(|&b| b == b' ')
        .ok_or(ForgeError::BadObject)?;
    if &bytes[..space] != expected_kind.as_bytes() {
        return Err(ForgeError::BadObject);
    }
    let nul_rel = bytes[space + 1..]
        .iter()
        .position(|&b| b == 0)
        .ok_or(ForgeError::BadObject)?;
    let nul = space + 1 + nul_rel;
    let len_str = std::str::from_utf8(&bytes[space + 1..nul]).map_err(|_| ForgeError::BadObject)?;
    let len: usize = len_str.parse().map_err(|_| ForgeError::BadObject)?;
    let body = &bytes[nul + 1..];
    if body.len() != len {
        return Err(ForgeError::BadObject);
    }
    Ok(body)
}

/// One parsed git tree entry: name, whether it is a subtree, and the raw
/// git object id (hex) it targets.
struct RawTreeEntry {
    name: String,
    is_dir: bool,
    git_id: String,
}

fn parse_tree_body(body: &[u8]) -> Result<Vec<RawTreeEntry>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < body.len() {
        let space = body[i..]
            .iter()
            .position(|&b| b == b' ')
            .ok_or(ForgeError::BadObject)?
            + i;
        let mode = std::str::from_utf8(&body[i..space]).map_err(|_| ForgeError::BadObject)?;
        let nul = body[space + 1..]
            .iter()
            .position(|&b| b == 0)
            .ok_or(ForgeError::BadObject)?
            + space
            + 1;
        let name = std::str::from_utf8(&body[space + 1..nul]).map_err(|_| ForgeError::BadObject)?;
        if !valid_name(name) {
            return Err(ForgeError::BadName);
        }
        let id_start = nul + 1;
        let id_end = id_start
            .checked_add(RAW_GIT_ID_LEN)
            .ok_or(ForgeError::BadObject)?;
        if id_end > body.len() {
            return Err(ForgeError::BadObject);
        }
        let is_dir = match mode {
            "40000" => true,
            "100644" => false,
            // Executable bit, symlink, and submodule modes are rejected
            // outright in this slice (see module doc) -- fail closed, not
            // silently mislabeled as a plain file.
            _ => return Err(ForgeError::BadObject),
        };
        out.push(RawTreeEntry {
            name: name.to_string(),
            is_dir,
            git_id: hex_encode(&body[id_start..id_end]),
        });
        i = id_end;
    }
    Ok(out)
}

/// `(name, email, timestamp_secs, timezone_offset)` parsed from a git
/// `author`/`committer` header line's value (everything after the literal
/// `"author "`/`"committer "` prefix).
fn parse_identity_line(s: &str) -> Result<(String, String, u64, String)> {
    let lt = s.find('<').ok_or(ForgeError::BadObject)?;
    let gt = s[lt..].find('>').ok_or(ForgeError::BadObject)? + lt;
    let name = s[..lt].trim_end().to_string();
    if name.is_empty() {
        return Err(ForgeError::BadObject);
    }
    let email = s[lt + 1..gt].to_string();
    let rest = s[gt + 1..].trim_start();
    let mut parts = rest.splitn(2, ' ');
    let ts_str = parts.next().ok_or(ForgeError::BadObject)?;
    let tz = parts.next().ok_or(ForgeError::BadObject)?.to_string();
    let ts: u64 = ts_str.parse().map_err(|_| ForgeError::BadObject)?;
    Ok((name, email, ts, tz))
}

struct ParsedGitCommit {
    tree_git_id: String,
    parent_git_ids: Vec<String>,
    author_name: String,
    author_email: String,
    author_ts_secs: u64,
    author_tz: String,
    committer_name: String,
    committer_email: String,
    committer_ts_secs: u64,
    committer_tz: String,
    message: String,
}

/// Parse exactly the canonical shape `git_export.rs` writes: `tree`,
/// zero or more `parent`, `author`, `committer`, a blank line, then the
/// message. Any other header line (`gpgsig`, `encoding`, `mergetag`, ...)
/// is rejected, not silently skipped (D-0418).
fn parse_commit_body(body: &[u8]) -> Result<ParsedGitCommit> {
    let text = std::str::from_utf8(body).map_err(|_| ForgeError::BadObject)?;
    let mut tree_git_id = None;
    let mut parent_git_ids = Vec::new();
    let mut author = None;
    let mut committer = None;
    let mut header_done = false;
    let mut message_lines: Vec<&str> = Vec::new();

    for line in text.split('\n') {
        if header_done {
            message_lines.push(line);
            continue;
        }
        if line.is_empty() {
            header_done = true;
            continue;
        }
        if let Some(rest) = line.strip_prefix("tree ") {
            if tree_git_id.is_some() {
                return Err(ForgeError::BadObject);
            }
            tree_git_id = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("parent ") {
            parent_git_ids.push(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("author ") {
            if author.is_some() {
                return Err(ForgeError::BadObject);
            }
            author = Some(parse_identity_line(rest)?);
        } else if let Some(rest) = line.strip_prefix("committer ") {
            if committer.is_some() {
                return Err(ForgeError::BadObject);
            }
            committer = Some(parse_identity_line(rest)?);
        } else {
            return Err(ForgeError::BadObject);
        }
    }
    if !header_done {
        return Err(ForgeError::BadObject);
    }
    let tree_git_id = tree_git_id.ok_or(ForgeError::BadObject)?;
    let (author_name, author_email, author_ts_secs, author_tz) =
        author.ok_or(ForgeError::BadObject)?;
    let (committer_name, committer_email, committer_ts_secs, committer_tz) =
        committer.ok_or(ForgeError::BadObject)?;
    let message = message_lines.join("\n");
    let message = message.strip_suffix('\n').unwrap_or(&message).to_string();

    Ok(ParsedGitCommit {
        tree_git_id,
        parent_git_ids,
        author_name,
        author_email,
        author_ts_secs,
        author_tz,
        committer_name,
        committer_email,
        committer_ts_secs,
        committer_tz,
        message,
    })
}

/// Import one git blob, verifying its claimed id against its actual
/// SHA-256 digest, then storing its bytes verbatim as a `mini-forge` file
/// blob signed by `importer`/`device`.
pub fn import_git_blob<B: Backend>(
    store: &mut Store<B>,
    importer: &Did,
    device: &Controller,
    git_id: &str,
    objects: &BTreeMap<String, GitObject>,
) -> Result<ObjectId> {
    let object = objects.get(git_id).ok_or(ForgeError::BadObject)?;
    verify_object_id(object)?;
    if object.kind != GitObjectKind::Blob {
        return Err(ForgeError::BadObject);
    }
    let body = parse_framed(&object.bytes, "blob")?;
    put_file(store, importer, device, body)
}

/// Import one git tree (recursively importing every blob/subtree it
/// contains), producing a `mini-forge` tree signed by `importer`/`device`.
pub fn import_git_tree<B: Backend>(
    store: &mut Store<B>,
    importer: &Did,
    device: &Controller,
    git_id: &str,
    objects: &BTreeMap<String, GitObject>,
) -> Result<ObjectId> {
    let object = objects.get(git_id).ok_or(ForgeError::BadObject)?;
    verify_object_id(object)?;
    if object.kind != GitObjectKind::Tree {
        return Err(ForgeError::BadObject);
    }
    let body = parse_framed(&object.bytes, "tree")?;
    let entries = parse_tree_body(body)?;
    if entries.len() > MAX_TREE_ENTRIES {
        return Err(ForgeError::FieldTooLarge);
    }

    let mut tree_entries = Vec::with_capacity(entries.len());
    for e in entries {
        let target = if e.is_dir {
            import_git_tree(store, importer, device, &e.git_id, objects)?
        } else {
            import_git_blob(store, importer, device, &e.git_id, objects)?
        };
        tree_entries.push(TreeEntry {
            name: e.name,
            is_dir: e.is_dir,
            target,
        });
    }
    put_tree(store, importer, device, &tree_entries)
}

/// Git-only metadata for one imported commit: the original git commit's
/// SHA-256 id and its author/committer fields, exactly as the git object
/// stated them. **Unauthenticated data** — git never signs these fields,
/// so this crate makes no claim they are true, only that they are what
/// the cited git commit object literally said. Never confuse this
/// object's `import_signer` (below) with these `author_name`/
/// `committer_name` strings: the import signer is who actually signed
/// this record; the author/committer fields are an unverified claim about
/// a different, external history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitImportProvenance {
    pub original_git_commit_id: String,
    pub author_name: String,
    pub author_email: String,
    pub author_ts_secs: u64,
    pub author_tz: String,
    pub committer_name: String,
    pub committer_email: String,
    pub committer_ts_secs: u64,
    pub committer_tz: String,
}

fn put_string(payload: &mut Vec<u8>, s: &str) {
    payload.extend_from_slice(&(s.len() as u32).to_be_bytes());
    payload.extend_from_slice(s.as_bytes());
}

fn encode_provenance(p: &GitImportProvenance) -> Vec<u8> {
    let mut payload = Vec::new();
    put_string(&mut payload, &p.original_git_commit_id);
    put_string(&mut payload, &p.author_name);
    put_string(&mut payload, &p.author_email);
    payload.extend_from_slice(&p.author_ts_secs.to_be_bytes());
    put_string(&mut payload, &p.author_tz);
    put_string(&mut payload, &p.committer_name);
    put_string(&mut payload, &p.committer_email);
    payload.extend_from_slice(&p.committer_ts_secs.to_be_bytes());
    put_string(&mut payload, &p.committer_tz);
    payload
}

fn take_u64(b: &[u8], off: &mut usize) -> Option<u64> {
    if *off + 8 > b.len() {
        return None;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&b[*off..*off + 8]);
    *off += 8;
    Some(u64::from_be_bytes(arr))
}

fn decode_provenance(b: &[u8]) -> Result<GitImportProvenance> {
    let mut off = 0usize;
    let original_git_commit_id = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let author_name = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let author_email = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let author_ts_secs = take_u64(b, &mut off).ok_or(ForgeError::BadObject)?;
    let author_tz = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let committer_name = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let committer_email = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    let committer_ts_secs = take_u64(b, &mut off).ok_or(ForgeError::BadObject)?;
    let committer_tz = take_str(b, &mut off).ok_or(ForgeError::BadObject)?;
    if off != b.len() {
        return Err(ForgeError::BadObject);
    }
    Ok(GitImportProvenance {
        original_git_commit_id,
        author_name,
        author_email,
        author_ts_secs,
        author_tz,
        committer_name,
        committer_email,
        committer_ts_secs,
        committer_tz,
    })
}

/// Read back a [`GitImportProvenance`] previously created by
/// [`import_commit_chain`].
pub fn read_git_import_provenance(obj: &mini_objects::Object) -> Result<GitImportProvenance> {
    if obj.object_type != ObjectType::Custom(GIT_IMPORT_PROVENANCE_TYPE.to_string()) {
        return Err(ForgeError::BadObject);
    }
    match &obj.payload {
        Payload::Public(b) => decode_provenance(b),
        Payload::Encrypted(_) => Err(ForgeError::BadObject),
    }
}

/// One imported commit: the real, native-shaped `mini-forge` commit
/// object id, plus the separate, explicitly-linked provenance object id
/// recording the original git commit's id/author/committer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCommit {
    pub commit_id: ObjectId,
    pub provenance_id: ObjectId,
}

#[allow(clippy::too_many_arguments)]
fn import_commit_rec<B: Backend>(
    store: &mut Store<B>,
    importer: &Did,
    device: &Controller,
    git_id: &str,
    objects: &BTreeMap<String, GitObject>,
    imported_at_ms: u64,
    next_sequence: &mut u64,
    imported: &mut BTreeMap<String, ImportedCommit>,
    budget: &mut usize,
) -> Result<ImportedCommit> {
    if let Some(existing) = imported.get(git_id) {
        return Ok(existing.clone());
    }
    *budget = budget.checked_sub(1).ok_or(ForgeError::FieldTooLarge)?;

    let object = objects.get(git_id).ok_or(ForgeError::BadObject)?;
    verify_object_id(object)?;
    if object.kind != GitObjectKind::Commit {
        return Err(ForgeError::BadObject);
    }
    let body = parse_framed(&object.bytes, "commit")?;
    let parsed = parse_commit_body(body)?;

    let tree_id = import_git_tree(store, importer, device, &parsed.tree_git_id, objects)?;

    let mut parent_ids = Vec::with_capacity(parsed.parent_git_ids.len());
    for p in &parsed.parent_git_ids {
        let imported_parent = import_commit_rec(
            store,
            importer,
            device,
            p,
            objects,
            imported_at_ms,
            next_sequence,
            imported,
            budget,
        )?;
        parent_ids.push(imported_parent.commit_id);
    }

    let commit_seq = *next_sequence;
    *next_sequence += 1;
    let commit_obj = commit(
        store,
        importer,
        device,
        &parsed.message,
        &tree_id,
        &parent_ids,
        imported_at_ms,
        commit_seq,
    )?;

    let provenance = GitImportProvenance {
        original_git_commit_id: git_id.to_string(),
        author_name: parsed.author_name,
        author_email: parsed.author_email,
        author_ts_secs: parsed.author_ts_secs,
        author_tz: parsed.author_tz,
        committer_name: parsed.committer_name,
        committer_email: parsed.committer_email,
        committer_ts_secs: parsed.committer_ts_secs,
        committer_tz: parsed.committer_tz,
    };
    let provenance_seq = *next_sequence;
    *next_sequence += 1;
    let provenance_obj =
        ObjectBuilder::new(ObjectType::Custom(GIT_IMPORT_PROVENANCE_TYPE.to_string()))
            .timestamp_ms(imported_at_ms)
            .sequence(provenance_seq)
            .payload(Payload::Public(encode_provenance(&provenance)))
            .link("commit", commit_obj.id().clone())
            .sign(importer, device)
            .map_err(ForgeError::Object)?;
    store.insert(&provenance_obj)?;

    let result = ImportedCommit {
        commit_id: commit_obj.id().clone(),
        provenance_id: provenance_obj.id().clone(),
    };
    imported.insert(git_id.to_string(), result.clone());
    Ok(result)
}

/// Import `git_id` and its full ancestor chain (mirrors
/// [`crate::export_commit_chain`]'s shape for the reverse direction).
/// `objects` must contain every blob/tree/commit the chain touches (the
/// same set `export_commit_chain` produces). Every object's claimed id is
/// verified against its actual SHA-256 digest before being trusted -- a
/// caller-supplied map is data, never authority. Returns every imported
/// commit, keyed by its original git id, deduplicated across shared
/// ancestors.
pub fn import_commit_chain<B: Backend>(
    store: &mut Store<B>,
    importer: &Did,
    device: &Controller,
    git_id: &str,
    objects: &BTreeMap<String, GitObject>,
    imported_at_ms: u64,
) -> Result<BTreeMap<String, ImportedCommit>> {
    let mut imported = BTreeMap::new();
    let mut budget = MAX_IMPORT_COMMITS;
    let mut next_sequence = 0u64;
    import_commit_rec(
        store,
        importer,
        device,
        git_id,
        objects,
        imported_at_ms,
        &mut next_sequence,
        &mut imported,
        &mut budget,
    )?;
    Ok(imported)
}

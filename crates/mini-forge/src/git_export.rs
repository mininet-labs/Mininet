//! Git SHA-256-object-format export bridge — D-0004 retained SHA-256
//! specifically "for the SHA-256 Git-object interop path," and the
//! self-hosted-forge-spine design doc names this real work as still
//! pending. Exports a mini-forge commit chain (commit → tree → blobs,
//! iteratively through every ancestor) as real git objects in git's
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
//! exported commit is stamped `+0000`. File mode is `100644` for ordinary
//! blobs and `100755` for executable blobs; source bytes and that portable
//! executable bit are preserved.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use did_mini::Did;
use mini_objects::{ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::{read_tree, ForgeError, Result, EXECUTABLE_FILE_TYPE, FILE_TYPE};

/// Hard ceiling on how many ancestor commits one export walks — the same
/// hostile-input discipline used by other bounded walks in this tree
/// (`checkout`'s depth budget, `resolve_project`'s chain walk).
pub const MAX_EXPORT_COMMITS: usize = 4096;

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

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub(crate) fn hex_decode(s: &str) -> Result<Vec<u8>> {
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

/// The synthesized, clearly-non-routable git author identity for a did:mini.
fn synthetic_identity(human: &Did) -> (String, String) {
    let scid = human.scid();
    (format!("mini:{scid}"), format!("{scid}@mininet.invalid"))
}

const MAX_EXPORT_OBJECTS: usize = 65_536;
const MAX_EXPORT_BYTES: usize = 64 * 1024 * 1024;

enum ExportNode {
    File {
        bytes: Vec<u8>,
        executable: bool,
    },
    Tree(Vec<crate::TreeEntry>),
    Commit {
        human: Did,
        timestamp_ms: u64,
        message: String,
        tree: ObjectId,
        parents: Vec<ObjectId>,
    },
}
impl ExportNode {
    fn dependencies(&self) -> Vec<ObjectId> {
        match self {
            Self::File { .. } => Vec::new(),
            Self::Tree(entries) => entries.iter().map(|entry| entry.target.clone()).collect(),
            Self::Commit { tree, parents, .. } => std::iter::once(tree.clone())
                .chain(parents.iter().cloned())
                .collect(),
        }
    }
}
struct Converted {
    id: String,
    kind: GitObjectKind,
    executable: bool,
    tree_height: usize,
}

/// Export a complete ancestor DAG without recursive commit traversal. Every
/// native blob/tree/commit is converted once, including shared tree DAGs.
/// Limits apply before publication: 4096 commits, 65536 unique native/Git
/// objects, 64 MiB native/framed Git bytes and the normal Forge tree depth.
/// Missing dependencies, cycles and cross-kind links fail explicitly.
pub fn export_commit_chain<B: Backend>(
    store: &Store<B>,
    commit_id: &ObjectId,
) -> Result<(String, Vec<GitObject>)> {
    let mut pending = vec![commit_id.clone()];
    let mut queued = BTreeSet::from([commit_id.as_str().to_string()]);
    let mut nodes = BTreeMap::new();
    let mut native_bytes = 0usize;
    let mut commits = 0usize;
    while let Some(id) = pending.pop() {
        if nodes.len() >= MAX_EXPORT_OBJECTS {
            return Err(ForgeError::FieldTooLarge);
        }
        let obj = store.get(&id)?;
        native_bytes = native_bytes
            .checked_add(obj.to_bytes().len())
            .ok_or(ForgeError::FieldTooLarge)?;
        if native_bytes > MAX_EXPORT_BYTES {
            return Err(ForgeError::FieldTooLarge);
        }
        let node = if obj.object_type == ObjectType::COMMIT {
            commits += 1;
            if commits > MAX_EXPORT_COMMITS {
                return Err(ForgeError::FieldTooLarge);
            }
            let tree = crate::validated_commit_tree(&obj)?;
            let message = match &obj.payload {
                Payload::Public(bytes) => {
                    String::from_utf8(bytes.clone()).map_err(|_| ForgeError::BadObject)?
                }
                Payload::Encrypted(_) => return Err(ForgeError::BadObject),
            };
            ExportNode::Commit {
                human: obj.author_human.clone(),
                timestamp_ms: obj.timestamp_ms,
                message,
                tree,
                parents: obj
                    .links
                    .iter()
                    .filter(|link| link.rel == "parent")
                    .map(|link| link.target.clone())
                    .collect(),
            }
        } else if obj.object_type == ObjectType::Custom(crate::TREE_TYPE.to_string()) {
            let mut entries = read_tree(&obj)?;
            entries.sort_by_key(|entry| git_tree_sort_key(&entry.name, entry.is_dir));
            ExportNode::Tree(entries)
        } else if obj.object_type == ObjectType::Custom(FILE_TYPE.to_string())
            || obj.object_type == ObjectType::Custom(EXECUTABLE_FILE_TYPE.to_string())
        {
            let bytes = match &obj.payload {
                Payload::Public(bytes) => bytes.clone(),
                Payload::Encrypted(_) => return Err(ForgeError::BadObject),
            };
            ExportNode::File {
                bytes,
                executable: obj.object_type == ObjectType::Custom(EXECUTABLE_FILE_TYPE.to_string()),
            }
        } else {
            return Err(ForgeError::BadObject);
        };
        for dependency in node.dependencies() {
            if queued.insert(dependency.as_str().to_string()) {
                if queued.len() > MAX_EXPORT_OBJECTS {
                    return Err(ForgeError::FieldTooLarge);
                }
                pending.push(dependency);
            }
        }
        nodes.insert(id.as_str().to_string(), node);
    }
    if !matches!(
        nodes.get(commit_id.as_str()),
        Some(ExportNode::Commit { .. })
    ) {
        return Err(ForgeError::BadObject);
    }
    let mut counts = BTreeMap::new();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ready = VecDeque::new();
    for (id, node) in &nodes {
        let dependencies: BTreeSet<_> = node
            .dependencies()
            .into_iter()
            .map(|id| id.as_str().to_string())
            .collect();
        counts.insert(id.clone(), dependencies.len());
        if dependencies.is_empty() {
            ready.push_back(id.clone());
        }
        for dependency in dependencies {
            children.entry(dependency).or_default().push(id.clone());
        }
    }
    let mut converted: BTreeMap<String, Converted> = BTreeMap::new();
    let mut objects: BTreeMap<String, GitObject> = BTreeMap::new();
    let mut git_bytes = 0usize;
    while let Some(id) = ready.pop_front() {
        let node = nodes.remove(&id).ok_or(ForgeError::BadObject)?;
        let mut executable = false;
        let mut tree_height = 0usize;
        let object = match node {
            ExportNode::File {
                bytes,
                executable: mode,
            } => {
                executable = mode;
                frame("blob", &bytes)
            }
            ExportNode::Tree(entries) => {
                let mut body = Vec::new();
                for entry in entries {
                    let target = converted
                        .get(entry.target.as_str())
                        .ok_or(ForgeError::BadObject)?;
                    let mode = if entry.is_dir {
                        if target.kind != GitObjectKind::Tree {
                            return Err(ForgeError::BadObject);
                        }
                        tree_height = tree_height.max(target.tree_height + 1);
                        "40000"
                    } else {
                        if target.kind != GitObjectKind::Blob {
                            return Err(ForgeError::BadObject);
                        }
                        if target.executable {
                            "100755"
                        } else {
                            "100644"
                        }
                    };
                    if tree_height > crate::MAX_TREE_DEPTH {
                        return Err(ForgeError::TooDeep);
                    }
                    body.extend_from_slice(mode.as_bytes());
                    body.push(b' ');
                    body.extend_from_slice(entry.name.as_bytes());
                    body.push(0);
                    body.extend_from_slice(&hex_decode(&target.id)?);
                }
                frame("tree", &body)
            }
            ExportNode::Commit {
                human,
                timestamp_ms,
                message,
                tree,
                parents,
            } => {
                let target = converted.get(tree.as_str()).ok_or(ForgeError::BadObject)?;
                if target.kind != GitObjectKind::Tree {
                    return Err(ForgeError::BadObject);
                }
                let mut body = format!("tree {}\n", target.id).into_bytes();
                for parent in parents {
                    let target = converted
                        .get(parent.as_str())
                        .ok_or(ForgeError::BadObject)?;
                    if target.kind != GitObjectKind::Commit {
                        return Err(ForgeError::BadObject);
                    }
                    body.extend_from_slice(format!("parent {}\n", target.id).as_bytes());
                }
                let (name, email) = synthetic_identity(&human);
                let seconds = timestamp_ms / 1000;
                body.extend_from_slice(format!("author {name} <{email}> {seconds} +0000\ncommitter {name} <{email}> {seconds} +0000\n\n").as_bytes());
                // Preserve the established export framing: one terminal newline.
                body.extend_from_slice(message.trim_end_matches('\n').as_bytes());
                body.push(b'\n');
                frame("commit", &body)
            }
        };
        let git_id = object.id.clone();
        let kind = object.kind;
        if let std::collections::btree_map::Entry::Vacant(entry) = objects.entry(git_id.clone()) {
            git_bytes = git_bytes
                .checked_add(object.bytes.len())
                .ok_or(ForgeError::FieldTooLarge)?;
            if git_bytes > MAX_EXPORT_BYTES {
                return Err(ForgeError::FieldTooLarge);
            }
            entry.insert(object);
        }
        converted.insert(
            id.clone(),
            Converted {
                id: git_id,
                kind,
                executable,
                tree_height,
            },
        );
        for child in children.get(&id).into_iter().flatten() {
            let count = counts.get_mut(child).ok_or(ForgeError::BadObject)?;
            *count = count.checked_sub(1).ok_or(ForgeError::BadObject)?;
            if *count == 0 {
                ready.push_back(child.clone());
            }
        }
    }
    if !nodes.is_empty() {
        return Err(ForgeError::BadObject);
    }
    let root = converted
        .get(commit_id.as_str())
        .ok_or(ForgeError::BadObject)?;
    Ok((root.id.clone(), objects.into_values().collect()))
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

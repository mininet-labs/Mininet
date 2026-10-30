//! Local Git snapshot/history migration and SHA1/SHA256 export mirrors. Git metadata is
//! source evidence, never a did:mini signature by the original Git author.
//! No remote operation, checkout, filter, hook, or shell command is invoked.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mini_forge::{commit, export_commit_chain, put_file, put_tree, GitObjectKind, TreeEntry};
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{FsBackend, Store};

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};

const MAX_OBJECT_BYTES: usize = mini_objects::MAX_PAYLOAD_BYTES;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_OBJECTS: usize = 4096;
const MAX_EXPORT_OBJECTS: usize = 65_536;
const MAX_DEPTH: usize = mini_forge::MAX_TREE_DEPTH;
/// Signed import evidence includes original commit bytes and each source tree
/// and blob's mode/path/id. It is not a claim of original-author authenticity.
pub const SNAPSHOT_PROVENANCE_TYPE: &str = "mini/git-snapshot-provenance-v1";

fn fail(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}
fn io(error: std::io::Error) -> CliError {
    CliError::Io(error.to_string())
}
fn forge(error: mini_forge::ForgeError) -> CliError {
    CliError::Forge(error.to_string())
}
fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| fail("Git returned non-UTF8 metadata"))
}
fn valid_oid(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn valid_name(name: &str) -> bool {
    mini_forge::file_name_is_well_formed(name)
        && !name.trim_end_matches('.').eq_ignore_ascii_case(".git")
}

/// Isolated Git process: ignore inherited Git config/environment overrides,
/// disable replacement objects, lazy network fetch, hooks and fsmonitor.
/// Reads and writes only raw object bytes, never worktree/filter content.
fn command(directory: &Path) -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key
            .to_string_lossy()
            .to_ascii_uppercase()
            .starts_with("GIT_")
        {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "--no-pager",
            "--no-replace-objects",
            "--no-optional-locks",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
            "-C",
        ])
        .arg(directory);
    cmd
}

fn run_git(directory: &Path, args: &[&str], input: Option<&[u8]>, limit: usize) -> Result<Vec<u8>> {
    let mut cmd = command(directory);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::null());
    cmd.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd.spawn().map_err(io)?;
    if let Some(input) = input {
        // hash-object consumes stdin before emitting its tiny, bounded OID.
        if let Err(error) = child
            .stdin
            .take()
            .ok_or_else(|| fail("missing Git stdin"))?
            .write_all(input)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io(error));
        }
    }
    let mut output = Vec::new();
    let result = child
        .stdout
        .take()
        .ok_or_else(|| fail("missing Git stdout"))?
        .take(limit as u64 + 1)
        .read_to_end(&mut output);
    if result.is_err() || output.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        return Err(fail(
            "Git output exceeded migration limit or could not be read",
        ));
    }
    if !child.wait().map_err(io)?.success() {
        return Err(fail(format!(
            "Git {} failed; check repository, object integrity, and permissions",
            args[0]
        )));
    }
    Ok(output)
}

/// Stream parent-before-child history without retaining the complete rev-list.
/// Stop/reap Git as soon as the requested uncheckpointed frontier is known.
fn batch_frontier(
    directory: &Path,
    home: &Path,
    format: &str,
    tip: &str,
    oid_len: usize,
    batch: usize,
) -> Result<Option<String>> {
    let mut child = command(directory)
        .args(["rev-list", "--topo-order", "--reverse", tip, "--"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(io)?;
    let result = (|| {
        let mut output = std::io::BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| fail("missing Git stdout"))?,
        );
        let mut selected = None;
        let mut count = 0;
        loop {
            let mut line = Vec::with_capacity(oid_len + 1);
            // Fixed-width OIDs: never allocate from an untrusted line length.
            for _ in 0..=oid_len {
                let mut byte = [0u8; 1];
                if output.read(&mut byte).map_err(io)? == 0 {
                    break;
                }
                line.push(byte[0]);
                if byte[0] == b'\n' {
                    break;
                }
            }
            if line.is_empty() {
                break;
            }
            if line.pop() != Some(b'\n') {
                return Err(fail("invalid source history enumeration"));
            }
            let id = text(&line)?;
            if !valid_oid(id, oid_len) {
                return Err(fail("invalid source history enumeration"));
            }
            if !checkpoint_path(home, format, id).try_exists().map_err(io)? {
                selected = Some(id.to_string());
                count += 1;
                if count == batch {
                    return Ok((selected, true));
                }
            }
        }
        Ok((selected, false))
    })();
    if matches!(&result, Ok((_, true))) || result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(io)?;
    match result {
        Ok((selected, early)) if early || status.success() => Ok(selected),
        Ok(_) => Err(fail("Git history enumeration failed")),
        Err(error) => Err(error),
    }
}

struct Source {
    directory: PathBuf,
    oid_len: usize,
    total_bytes: usize,
    objects: usize,
    manifest: Vec<u8>,
}
impl Source {
    fn read(&mut self, kind: &str, id: &str) -> Result<Vec<u8>> {
        if !valid_oid(id, self.oid_len) || self.objects >= MAX_OBJECTS {
            return Err(fail(
                "invalid Git object id or snapshot exceeds 4096 objects",
            ));
        }
        self.objects += 1;
        let bytes = run_git(
            &self.directory,
            &["cat-file", kind, id],
            None,
            MAX_OBJECT_BYTES,
        )?;
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| fail("snapshot size overflow"))?;
        if self.total_bytes > MAX_TOTAL_BYTES {
            return Err(fail("snapshot exceeds 64 MiB migration limit"));
        }
        // cat-file alone does not authenticate the object's claimed name.
        let actual = run_git(
            &self.directory,
            &["hash-object", "--no-filters", "-t", kind, "--stdin"],
            Some(&bytes),
            128,
        )?;
        if text(&actual)?.trim() != id {
            return Err(fail("Git object hash mismatch"));
        }
        Ok(bytes)
    }
    fn tree(&mut self, id: &str, path: &str, depth: usize) -> Result<SnapshotTree> {
        if depth > MAX_DEPTH {
            return Err(fail("snapshot exceeds Forge tree depth limit"));
        }
        let bytes = self.read("tree", id)?;
        let mut tail = bytes.as_slice();
        let mut entries = BTreeMap::new();
        while !tail.is_empty() {
            let space = tail
                .iter()
                .position(|b| *b == b' ')
                .ok_or_else(|| fail("invalid Git tree"))?;
            let nul = tail
                .iter()
                .position(|b| *b == 0)
                .ok_or_else(|| fail("invalid Git tree"))?;
            if space >= nul {
                return Err(fail("invalid Git tree entry"));
            }
            let mode = text(&tail[..space])?;
            let name = text(&tail[space + 1..nul])?;
            if !valid_name(name) {
                return Err(fail(format!("unsupported Git path {path}{name:?}: Forge requires safe ASCII file names <=255 bytes (letters, digits, -_.), excluding .git")));
            }
            let raw_len = self.oid_len / 2;
            let raw_id = tail
                .get(nul + 1..nul + 1 + raw_len)
                .ok_or_else(|| fail("truncated Git tree id"))?;
            let child_id: String = raw_id.iter().map(|b| format!("{b:02x}")).collect();
            tail = &tail[nul + 1 + raw_len..];
            let full_path = format!("{path}{name}");
            self.manifest
                .extend_from_slice(format!("{mode} {child_id} {full_path}\n").as_bytes());
            let item = match mode {
                "40000" => {
                    SnapshotItem::Tree(self.tree(&child_id, &format!("{full_path}/"), depth + 1)?)
                }
                "100644" => SnapshotItem::File(self.read("blob", &child_id)?),
                "100755" => SnapshotItem::Executable(self.read("blob", &child_id)?),
                _ => {
                    return Err(fail(format!(
                        "unsupported Git mode {mode}: symlinks and submodules are not imported"
                    )))
                }
            };
            if entries.insert(name.to_string(), item).is_some() {
                return Err(fail("duplicate Git tree name"));
            }
            if entries.len() > mini_forge::MAX_TREE_ENTRIES {
                return Err(fail("Git directory exceeds Forge tree entry limit"));
            }
        }
        Ok(SnapshotTree(entries))
    }
}
struct SnapshotTree(BTreeMap<String, SnapshotItem>);
enum SnapshotItem {
    File(Vec<u8>),
    Executable(Vec<u8>),
    Tree(SnapshotTree),
}
impl SnapshotTree {
    fn store(
        &self,
        store: &mut Store<FsBackend>,
        identity: &crate::identity::Identity,
    ) -> Result<ObjectId> {
        let mut entries = Vec::new();
        for (name, item) in &self.0 {
            let (target, is_dir) = match item {
                SnapshotItem::File(bytes) => (
                    put_file(store, &identity.human_did(), &identity.device, bytes)
                        .map_err(forge)?,
                    false,
                ),
                SnapshotItem::Executable(bytes) => (
                    mini_forge::put_executable_file(
                        store,
                        &identity.human_did(),
                        &identity.device,
                        bytes,
                    )
                    .map_err(forge)?,
                    false,
                ),
                SnapshotItem::Tree(tree) => (tree.store(store, identity)?, true),
            };
            entries.push(TreeEntry {
                name: name.clone(),
                target,
                is_dir,
            });
        }
        put_tree(store, &identity.human_did(), &identity.device, &entries).map_err(forge)
    }
}

fn import_snapshot(
    home: &Path,
    store_path: &Path,
    path: &Path,
    reference: &str,
    executable_loss: bool,
) -> Result<CommandResult> {
    let directory = path.canonicalize().map_err(io)?;
    if reference != "HEAD" {
        if !(reference.starts_with("refs/heads/") || reference.starts_with("refs/tags/")) {
            return Err(fail(
                "--ref must be HEAD or an exact refs/heads/... or refs/tags/... name",
            ));
        }
        run_git(&directory, &["check-ref-format", reference], None, 128)?;
    }
    let format = run_git(
        &directory,
        &["rev-parse", "--show-object-format"],
        None,
        128,
    )?;
    let format = text(&format)?.trim().to_string();
    let oid_len = match format.as_str() {
        "sha1" => 40,
        "sha256" => 64,
        _ => return Err(fail("unsupported Git object format")),
    };
    let resolved = run_git(
        &directory,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
        None,
        128,
    )?;
    let original = text(&resolved)?.trim().to_string();
    let mut source = Source {
        directory,
        oid_len,
        total_bytes: 0,
        objects: 0,
        manifest: Vec::new(),
    };
    let original_commit = source.read("commit", &original)?;
    let first_line = original_commit
        .split(|b| *b == b'\n')
        .next()
        .ok_or_else(|| fail("empty Git commit"))?;
    let tree_id = text(first_line)?
        .strip_prefix("tree ")
        .ok_or_else(|| fail("Git commit lacks initial tree header"))?;
    let tree = source.tree(tree_id, "", 0)?;
    // Length framing keeps arbitrary commit messages unambiguous. Manifest paths
    // are restricted ASCII without newlines; original commit bytes stay exact.
    let header = format!("MINI_GIT_SNAPSHOT_V1\nformat {format}\ncommit {original}\nexecutable_mode_loss false\ncommit_bytes {}\n", original_commit.len());
    let mut evidence = header.into_bytes();
    evidence.extend_from_slice(&original_commit);
    evidence.extend_from_slice(b"\nmanifest\n");
    evidence.extend_from_slice(&source.manifest);
    if evidence.len() > mini_objects::MAX_PAYLOAD_BYTES {
        return Err(fail("snapshot provenance exceeds Forge payload limit"));
    }
    // Validate the entire snapshot before creating an identity or signed objects.
    let identity = crate::identity::load_or_init(home)?;
    let mut store = crate::store::open_store(store_path)?;
    let tree = tree.store(&mut store, &identity)?;
    let sequence = crate::sequence::next(home, store_path)?;
    let imported = commit(&mut store, &identity.human_did(), &identity.device,
        &format!("Imported Git snapshot {format}:{original}; original authorship is unauthenticated source metadata"),
        &tree, &[], crate::sequence::now_ms(), sequence).map_err(forge)?;
    let provenance_sequence = crate::sequence::next(home, store_path)?;
    let provenance = ObjectBuilder::new(ObjectType::Custom(SNAPSHOT_PROVENANCE_TYPE.to_string()))
        .timestamp_ms(crate::sequence::now_ms())
        .sequence(provenance_sequence)
        .payload(Payload::Public(evidence))
        .link("commit", imported.id().clone())
        .sign(&identity.human_did(), &identity.device)
        .map_err(|e| CliError::Object(e.to_string()))?;
    store
        .insert(&provenance)
        .map_err(|e| CliError::Store(e.to_string()))?;
    Ok(CommandResult::new(format!("snapshot imported: {}\nprovenance: {}\nsource: {format}:{original}\nSigned by importer; no Git history, original-author signature or canonical adoption is implied.", imported.id().as_str(), provenance.id().as_str()))
        .field("commit_id", JsonValue::str(imported.id().as_str()))
        .field("provenance_id", JsonValue::str(provenance.id().as_str()))
        .field("source_format", JsonValue::str(format)).field("source_commit", JsonValue::str(original))
        .field("history_imported", JsonValue::Bool(false)).field("executable_mode_loss_allowed", JsonValue::Bool(executable_loss))
        .field("executable_modes_preserved", JsonValue::Bool(true)))
}

/// Raw source metadata is an importer attestation, never original-author
/// authentication. Each record binds one native commit and its ordered parents.
const HISTORY_PROVENANCE_TYPE: &str = "mini/git-history-provenance-v1";
const DEFAULT_HISTORY_COMMITS: usize = 128;
const MAX_HISTORY_COMMITS: usize = 1024;

#[derive(Clone)]
struct HistoryCommit {
    raw: Vec<u8>,
    tree: String,
    parents: Vec<String>,
    message: String,
    resumed: Option<(ObjectId, ObjectId)>,
}
#[derive(Clone)]
struct HistoryEntry {
    name: String,
    mode: String,
    target: String,
}
struct HistoryGraph {
    source: Source,
    objects: BTreeMap<String, (String, Vec<u8>)>,
    trees: BTreeMap<String, Vec<HistoryEntry>>,
    commits: BTreeMap<String, HistoryCommit>,
}
impl HistoryGraph {
    fn read(&mut self, kind: &str, id: &str) -> Result<Vec<u8>> {
        if let Some((saved_kind, bytes)) = self.objects.get(id) {
            if saved_kind != kind {
                return Err(fail("Git object reused with incompatible types"));
            }
            return Ok(bytes.clone());
        }
        let bytes = self.source.read(kind, id)?;
        self.objects
            .insert(id.to_string(), (kind.to_string(), bytes.clone()));
        Ok(bytes)
    }
    fn tree(&mut self, id: &str, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(fail("Git history exceeds Forge tree depth limit"));
        }
        if self.trees.contains_key(id) {
            return Ok(());
        }
        let bytes = self.read("tree", id)?;
        let mut tail = bytes.as_slice();
        let mut entries = Vec::new();
        let mut names = BTreeSet::new();
        while !tail.is_empty() {
            let space = tail
                .iter()
                .position(|b| *b == b' ')
                .ok_or_else(|| fail("invalid history tree"))?;
            let nul = tail
                .iter()
                .position(|b| *b == 0)
                .ok_or_else(|| fail("invalid history tree"))?;
            if space >= nul {
                return Err(fail("invalid history tree entry"));
            }
            let mode = text(&tail[..space])?.to_string();
            let name = text(&tail[space + 1..nul])?.to_string();
            if !valid_name(&name) || !names.insert(name.clone()) {
                return Err(fail("unsupported or duplicate Git history filename"));
            }
            let raw_len = self.source.oid_len / 2;
            let target = hex(tail
                .get(nul + 1..nul + 1 + raw_len)
                .ok_or_else(|| fail("truncated history tree id"))?);
            tail = &tail[nul + 1 + raw_len..];
            match mode.as_str() {
                "40000" => self.tree(&target, depth + 1)?,
                "100644" | "100755" => {
                    self.read("blob", &target)?;
                }
                _ => {
                    return Err(fail(format!(
                        "unsupported Git history mode {mode}; symlinks/submodules are not imported"
                    )))
                }
            }
            entries.push(HistoryEntry { name, mode, target });
            if entries.len() > mini_forge::MAX_TREE_ENTRIES {
                return Err(fail("history directory exceeds Forge entry limit"));
            }
        }
        self.trees.insert(id.to_string(), entries);
        Ok(())
    }
    fn files(&self, root: &str) -> Result<Vec<(String, Vec<u8>, bool)>> {
        let mut stack = vec![(root.to_string(), String::new(), 0usize)];
        let mut files = Vec::new();
        let mut visits = 0usize;
        let mut bytes = 0usize;
        while let Some((id, prefix, depth)) = stack.pop() {
            visits += 1;
            if visits > MAX_OBJECTS || depth > MAX_DEPTH {
                return Err(fail("history tree exceeds occurrence/depth budget"));
            }
            for entry in self
                .trees
                .get(&id)
                .ok_or_else(|| fail("missing history tree"))?
            {
                visits += 1;
                if visits > MAX_OBJECTS {
                    return Err(fail("history tree exceeds occurrence budget"));
                }
                let path = format!("{prefix}{}", entry.name);
                if entry.mode == "40000" {
                    stack.push((entry.target.clone(), format!("{path}/"), depth + 1));
                } else {
                    let (_, data) = self
                        .objects
                        .get(&entry.target)
                        .ok_or_else(|| fail("missing history blob"))?;
                    bytes = bytes
                        .checked_add(data.len())
                        .ok_or_else(|| fail("history tree size overflow"))?;
                    if bytes > MAX_TOTAL_BYTES {
                        return Err(fail("history tree exceeds 64 MiB occurrence budget"));
                    }
                    files.push((path, data.clone(), entry.mode == "100755"));
                }
            }
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }
    fn store_tree(
        &self,
        id: &str,
        store: &mut Store<FsBackend>,
        identity: &crate::identity::Identity,
        saved: &mut BTreeMap<String, ObjectId>,
        blobs: &mut BTreeMap<(String, bool), ObjectId>,
    ) -> Result<ObjectId> {
        if let Some(found) = saved.get(id) {
            return Ok(found.clone());
        }
        let mut entries = Vec::new();
        for entry in self
            .trees
            .get(id)
            .ok_or_else(|| fail("missing validated tree"))?
        {
            let is_dir = entry.mode == "40000";
            let target = if is_dir {
                self.store_tree(&entry.target, store, identity, saved, blobs)?
            } else {
                let executable = entry.mode == "100755";
                let key = (entry.target.clone(), executable);
                if let Some(found) = blobs.get(&key) {
                    found.clone()
                } else {
                    let (_, bytes) = self
                        .objects
                        .get(&entry.target)
                        .ok_or_else(|| fail("missing validated blob"))?;
                    let id = if executable {
                        mini_forge::put_executable_file(
                            store,
                            &identity.human_did(),
                            &identity.device,
                            bytes,
                        )
                    } else {
                        put_file(store, &identity.human_did(), &identity.device, bytes)
                    }
                    .map_err(forge)?;
                    blobs.insert(key, id.clone());
                    id
                }
            };
            entries.push(TreeEntry {
                name: entry.name.clone(),
                target,
                is_dir,
            });
        }
        let result =
            put_tree(store, &identity.human_did(), &identity.device, &entries).map_err(forge)?;
        saved.insert(id.to_string(), result.clone());
        Ok(result)
    }
}
fn parse_history_commit(raw: Vec<u8>, oid_len: usize) -> Result<HistoryCommit> {
    let split = raw
        .windows(2)
        .position(|p| p == b"\n\n")
        .ok_or_else(|| fail("Git commit has no header/message separator"))?;
    let headers = &raw[..split];
    let first = headers
        .split(|b| *b == b'\n')
        .next()
        .ok_or_else(|| fail("empty Git commit"))?;
    let tree = text(first)?
        .strip_prefix("tree ")
        .ok_or_else(|| fail("Git commit must begin with tree"))?
        .to_string();
    if !valid_oid(&tree, oid_len) {
        return Err(fail("invalid source tree id"));
    }
    let mut parents = Vec::new();
    let mut author = 0usize;
    let mut committer = 0usize;
    for line in headers.split(|b| *b == b'\n').skip(1) {
        if let Some(value) = line.strip_prefix(b"parent ") {
            let id = text(value)?.to_string();
            if !valid_oid(&id, oid_len) || parents.contains(&id) {
                return Err(fail("invalid or duplicate source parent"));
            }
            parents.push(id);
        } else if line.starts_with(b"tree ") {
            return Err(fail("duplicate source tree header"));
        } else if line.starts_with(b"author ") {
            author += 1;
        } else if line.starts_with(b"committer ") {
            committer += 1;
        }
        // Other headers (including source gpgsig) stay exact in signed evidence;
        // they are never interpreted as Mininet signature or authority claims.
    }
    if author != 1 || committer != 1 || parents.len() + 1 > mini_objects::MAX_LINKS {
        return Err(fail("unsupported source commit metadata/parent count"));
    }
    let message = text(&raw[split + 2..])?.to_string();
    if message.len() > mini_forge::MAX_MESSAGE_BYTES {
        return Err(fail("source commit message exceeds Forge limit"));
    }
    Ok(HistoryCommit {
        raw,
        tree,
        parents,
        message,
        resumed: None,
    })
}
fn checkpoint_path(home: &Path, format: &str, source: &str) -> PathBuf {
    home.join("git-history").join(format).join(source)
}
fn history_payload(format: &str, source: &str, raw: &[u8], parents: &[ObjectId]) -> Vec<u8> {
    let mut payload = format!("MINI_GIT_HISTORY_V1\n{format}\n{source}\n").into_bytes();
    payload.extend_from_slice(&(raw.len() as u64).to_be_bytes());
    payload.extend_from_slice(raw);
    payload.extend_from_slice(&(parents.len() as u32).to_be_bytes());
    for parent in parents {
        payload.extend_from_slice(&(parent.as_str().len() as u32).to_be_bytes());
        payload.extend_from_slice(parent.as_str().as_bytes());
    }
    payload
}
fn history_checkpoint(
    home: &Path,
    store_path: &Path,
    format: &str,
    source: &str,
    parsed: &HistoryCommit,
    identity: Option<&crate::identity::Identity>,
    recovered: &BTreeMap<String, ObjectId>,
) -> Result<Option<(ObjectId, ObjectId)>> {
    let path = checkpoint_path(home, format, source);
    let mut raw_id = String::new();
    let missing_index = match std::fs::File::open(&path) {
        Ok(file) => {
            file.take(257).read_to_string(&mut raw_id).map_err(io)?;
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(id) = recovered.get(source) else {
                return Ok(None);
            };
            raw_id = id.as_str().to_string();
            true
        }
        Err(error) => return Err(io(error)),
    };
    if raw_id.len() > 256 {
        return Err(fail("oversized Git history checkpoint index"));
    }
    let provenance_id =
        ObjectId::parse(raw_id.trim()).map_err(|_| fail("corrupt Git history checkpoint index"))?;
    let identity =
        identity.ok_or_else(|| fail("history checkpoint lacks local signing identity"))?;
    let store = crate::store::open_store(store_path)?;
    let record = store
        .get(&provenance_id)
        .map_err(|e| CliError::Store(e.to_string()))?;
    mini_objects::verify_provenance(&record, &identity.human.kel(), &identity.device.kel())
        .map_err(|_| fail("unverified Git history checkpoint signature"))?;
    if record.object_type != ObjectType::Custom(HISTORY_PROVENANCE_TYPE.to_string())
        || record.links.len() != 1
        || record.links[0].rel != "commit"
    {
        return Err(fail("malformed signed history checkpoint"));
    }
    let native_id = record.links[0].target.clone();
    let native = store
        .get(&native_id)
        .map_err(|e| CliError::Store(e.to_string()))?;
    mini_objects::verify_provenance(&native, &identity.human.kel(), &identity.device.kel())
        .map_err(|_| fail("unverified resumed commit signature"))?;
    let parents: Vec<ObjectId> = native
        .links
        .iter()
        .filter(|link| link.rel == "parent")
        .map(|link| link.target.clone())
        .collect();
    let roots: Vec<_> = native
        .links
        .iter()
        .filter(|link| link.rel == "tree")
        .collect();
    if native.object_type != ObjectType::COMMIT
        || roots.len() != 1
        || native.links.len() != parents.len() + 1
        || parents.len() != parsed.parents.len()
        || native.payload != Payload::Public(parsed.message.as_bytes().to_vec())
        || record.payload != Payload::Public(history_payload(format, source, &parsed.raw, &parents))
    {
        return Err(fail(
            "history checkpoint does not bind exact source bytes/message/parents",
        ));
    }
    // Verify that each direct source parent is represented by a separately
    // signed import record; a checkpoint cannot silently invent a new root.
    for (source_parent, native_parent) in parsed.parents.iter().zip(&parents) {
        let parent_path = checkpoint_path(home, format, source_parent);
        let mut parent_id = String::new();
        std::fs::File::open(parent_path)
            .map_err(|_| fail("history checkpoint has missing source-parent checkpoint"))?
            .take(257)
            .read_to_string(&mut parent_id)
            .map_err(io)?;
        if parent_id.len() > 256 {
            return Err(fail("oversized parent checkpoint index"));
        }
        let parent_record = store
            .get(
                &ObjectId::parse(parent_id.trim())
                    .map_err(|_| fail("corrupt parent checkpoint index"))?,
            )
            .map_err(|e| CliError::Store(e.to_string()))?;
        mini_objects::verify_provenance(
            &parent_record,
            &identity.human.kel(),
            &identity.device.kel(),
        )
        .map_err(|_| fail("unverified history parent checkpoint"))?;
        let prefix = format!("MINI_GIT_HISTORY_V1\n{format}\n{source_parent}\n");
        if parent_record.object_type != ObjectType::Custom(HISTORY_PROVENANCE_TYPE.to_string())
            || parent_record.links.len() != 1
            || parent_record.links[0].rel != "commit"
            || parent_record.links[0].target != *native_parent
            || !matches!(&parent_record.payload, Payload::Public(bytes) if bytes.starts_with(prefix.as_bytes()))
        {
            return Err(fail("history checkpoint has inconsistent parent mapping"));
        }
    }
    preflight_export(&store, &roots[0].target)?;
    if missing_index {
        mini_durable::create_dir_all(
            path.parent()
                .ok_or_else(|| fail("checkpoint has no parent"))?,
        )
        .map_err(io)?;
        mini_durable::atomic_replace(&path, provenance_id.as_str().as_bytes()).map_err(io)?;
    }
    Ok(Some((native_id, provenance_id)))
}
fn import_history(
    home: &Path,
    store_path: &Path,
    path: &Path,
    reference: &str,
    max_commits: usize,
    batch_commits: Option<usize>,
) -> Result<CommandResult> {
    if max_commits == 0 || max_commits > MAX_HISTORY_COMMITS {
        return Err(fail(
            "--max-commits must be between 1 and 1024 (new commits per batch)",
        ));
    }
    let directory = path.canonicalize().map_err(io)?;
    if reference != "HEAD" {
        if !(reference.starts_with("refs/heads/") || reference.starts_with("refs/tags/")) {
            return Err(fail(
                "history --ref must be HEAD or an exact branch/tag ref",
            ));
        }
        run_git(&directory, &["check-ref-format", reference], None, 128)?;
    }
    let format_bytes = run_git(
        &directory,
        &["rev-parse", "--show-object-format"],
        None,
        128,
    )?;
    let format = text(&format_bytes)?.trim().to_string();
    let oid_len = match format.as_str() {
        "sha1" => 40,
        "sha256" => 64,
        _ => return Err(fail("unsupported Git object format")),
    };
    let tip_bytes = run_git(
        &directory,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
        None,
        128,
    )?;
    let requested_tip = text(&tip_bytes)?.trim().to_string();
    // Sequence allocation alone does not serialize source->native checkpoint
    // mappings. Keep one per-home migration owner from the first checkpoint
    // read until all signed records and durable indexes have been published.
    mini_durable::create_dir_all(home).map_err(io)?;
    let _history_lock =
        mini_durable::try_lock_exclusive(&home.join("git-history.lock")).map_err(|error| {
            CliError::Io(format!(
                "Git history import is busy or its lock is unavailable: {error}"
            ))
        })?;
    let mut tip = requested_tip.clone();
    if let Some(batch) = batch_commits {
        if batch == 0 || batch > MAX_HISTORY_COMMITS {
            return Err(fail("--batch-commits must be between 1 and 1024"));
        }
        let selected = batch_frontier(&directory, home, &format, &requested_tip, oid_len, batch)?;
        if let Some(selected) = selected {
            tip = selected;
        }
    }
    let identity = match crate::identity::load(home) {
        Ok(identity) => Some(identity),
        Err(CliError::NotInitialized) => None,
        Err(error) => return Err(error),
    };
    // A durable signed mapping can outlive a crash before its advisory index.
    // Index candidates once, then apply the complete existing checkpoint checks
    // before restoring an index or reusing a native commit.
    let mut recovered = BTreeMap::new();
    if let Some(identity) = &identity {
        let store = crate::store::open_store(store_path)?;
        for id in store
            .by_type(&ObjectType::Custom(HISTORY_PROVENANCE_TYPE.into()))
            .map_err(|e| CliError::Store(e.to_string()))?
        {
            let record = store.get(&id).map_err(|e| CliError::Store(e.to_string()))?;
            if mini_objects::verify_provenance(
                &record,
                &identity.human.kel(),
                &identity.device.kel(),
            )
            .is_err()
            {
                continue;
            }
            let Payload::Public(bytes) = &record.payload else {
                continue;
            };
            let mut lines = bytes.splitn(4, |b| *b == b'\n');
            if lines.next() != Some(b"MINI_GIT_HISTORY_V1".as_slice())
                || lines.next() != Some(format.as_bytes())
            {
                continue;
            }
            let Some(source) = lines.next().and_then(|v| std::str::from_utf8(v).ok()) else {
                continue;
            };
            if valid_oid(source, oid_len) && recovered.insert(source.to_string(), id).is_some() {
                return Err(fail(
                    "ambiguous signed Git history mappings; refusing to invent a canonical mapping",
                ));
            }
        }
    }
    let mut graph = HistoryGraph {
        source: Source {
            directory,
            oid_len,
            total_bytes: 0,
            objects: 0,
            manifest: Vec::new(),
        },
        objects: BTreeMap::new(),
        trees: BTreeMap::new(),
        commits: BTreeMap::new(),
    };
    let mut pending = vec![tip.clone()];
    let mut new_count = 0usize;
    while let Some(id) = pending.pop() {
        if graph.commits.contains_key(&id) {
            continue;
        }
        let raw = graph.read("commit", &id)?;
        let mut parsed = parse_history_commit(raw, oid_len)?;
        parsed.resumed = history_checkpoint(
            home,
            store_path,
            &format,
            &id,
            &parsed,
            identity.as_ref(),
            &recovered,
        )?;
        if parsed.resumed.is_none() {
            new_count += 1;
            if new_count > max_commits {
                return Err(fail("Git history exceeds --max-commits; no commits signed. Import an earlier exact ref first or raise the bounded limit; history is never truncated"));
            }
            pending.extend(parsed.parents.iter().cloned());
        }
        graph.tree(&parsed.tree, 0)?;
        let expected = graph.files(&parsed.tree)?;
        if let Some((native_id, _)) = &parsed.resumed {
            let store = crate::store::open_store(store_path)?;
            let mut actual = mini_forge::checkout_with_modes(&store, native_id).map_err(forge)?;
            actual.sort_by(|a, b| a.0.cmp(&b.0));
            if actual != expected {
                return Err(fail(
                    "resumed native tree does not match verified source content/modes",
                ));
            }
        }
        // Enforce provenance payload limits before any signed-object creation.
        if parsed.raw.len() + parsed.parents.len() * 256 + 256 > mini_objects::MAX_PAYLOAD_BYTES {
            return Err(fail("history provenance exceeds Forge payload limit"));
        }
        graph.commits.insert(id, parsed);
    }
    // All source objects and resume records passed; compute parent-before-child
    // order iteratively and reject any missing parent/cycle before signing.
    let mut waiting = BTreeMap::new();
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ready = VecDeque::new();
    for (id, item) in &graph.commits {
        let deps = if item.resumed.is_some() {
            0
        } else {
            item.parents.len()
        };
        waiting.insert(id.clone(), deps);
        if deps == 0 {
            ready.push_back(id.clone());
        }
        if item.resumed.is_none() {
            for parent in &item.parents {
                if !graph.commits.contains_key(parent) {
                    return Err(fail("source history has a missing parent"));
                }
                children.entry(parent.clone()).or_default().push(id.clone());
            }
        }
    }
    let mut order = Vec::new();
    while let Some(id) = ready.pop_front() {
        order.push(id.clone());
        for child in children.get(&id).into_iter().flatten() {
            let count = waiting
                .get_mut(child)
                .ok_or_else(|| fail("missing history dependency count"))?;
            *count = count
                .checked_sub(1)
                .ok_or_else(|| fail("inconsistent history dependencies"))?;
            if *count == 0 {
                ready.push_back(child.clone());
            }
        }
    }
    if order.len() != graph.commits.len() {
        return Err(fail("source history contains a cycle or missing parent"));
    }
    let identity = match identity {
        Some(identity) => identity,
        None => crate::identity::load_or_init(home)?,
    };
    let mut store = crate::store::open_store(store_path)?;
    let mut mapped: BTreeMap<String, (ObjectId, ObjectId)> = BTreeMap::new();
    let mut saved_trees = BTreeMap::new();
    let mut saved_blobs = BTreeMap::new();
    let mut imported_count = 0usize;
    for id in order {
        let item = &graph.commits[&id];
        if let Some(resumed) = &item.resumed {
            mapped.insert(id, resumed.clone());
            continue;
        }
        let parents: Vec<ObjectId> = item
            .parents
            .iter()
            .map(|parent| {
                mapped
                    .get(parent)
                    .map(|pair| pair.0.clone())
                    .ok_or_else(|| fail("unmapped validated source parent"))
            })
            .collect::<Result<_>>()?;
        let tree = graph.store_tree(
            &item.tree,
            &mut store,
            &identity,
            &mut saved_trees,
            &mut saved_blobs,
        )?;
        let sequence = crate::sequence::next(home, store_path)?;
        let native = commit(
            &mut store,
            &identity.human_did(),
            &identity.device,
            &item.message,
            &tree,
            &parents,
            crate::sequence::now_ms(),
            sequence,
        )
        .map_err(forge)?;
        let provenance_sequence = crate::sequence::next(home, store_path)?;
        let provenance =
            ObjectBuilder::new(ObjectType::Custom(HISTORY_PROVENANCE_TYPE.to_string()))
                .timestamp_ms(crate::sequence::now_ms())
                .sequence(provenance_sequence)
                .payload(Payload::Public(history_payload(
                    &format, &id, &item.raw, &parents,
                )))
                .link("commit", native.id().clone())
                .sign(&identity.human_did(), &identity.device)
                .map_err(|e| CliError::Object(e.to_string()))?;
        store
            .insert(&provenance)
            .map_err(|e| CliError::Store(e.to_string()))?;
        let index = checkpoint_path(home, &format, &id);
        mini_durable::create_dir_all(
            index
                .parent()
                .ok_or_else(|| fail("history checkpoint lacks parent directory"))?,
        )
        .map_err(io)?;
        mini_durable::atomic_replace(&index, provenance.id().as_str().as_bytes()).map_err(io)?;
        mapped.insert(id, (native.id().clone(), provenance.id().clone()));
        imported_count += 1;
    }
    let (native, provenance) = mapped
        .get(&tip)
        .ok_or_else(|| fail("missing imported source tip"))?;
    Ok(CommandResult::new(format!("history imported: {}\nprovenance: {}\nsource: {format}:{tip}\nnew commits: {imported_count}; verified resume frontiers: {}\ncompletion: {}\nImporter signatures attest migration; original Git authors, timestamps and signatures remain unauthenticated source metadata. No canonical adoption is implied.", native.as_str(), provenance.as_str(), mapped.len() - imported_count, if tip == requested_tip { "complete source history" } else { "PARTIAL batch; repeat the same --batch-commits command to continue" }))
        .field("commit_id", JsonValue::str(native.as_str())).field("provenance_id", JsonValue::str(provenance.as_str()))
        .field("source_format", JsonValue::str(format)).field("source_commit", JsonValue::str(tip.clone()))
        .field("requested_source_commit", JsonValue::str(requested_tip.clone())).field("complete", JsonValue::Bool(tip == requested_tip))
        .field("continue_required", JsonValue::Bool(tip != requested_tip))
        .field("imported_commits", JsonValue::num(imported_count as u64)).field("resumed_frontiers", JsonValue::num((mapped.len() - imported_count) as u64))
        .field("history_imported", JsonValue::Bool(true)).field("executable_modes_preserved", JsonValue::Bool(true)))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// Rewrite only structural Git references. File bytes, author metadata and
// commit messages remain exact; exported synthetic author metadata already
// comes from the signed Mininet commit through mini-forge's export bridge.
fn rewrite_body(
    kind: GitObjectKind,
    body: &[u8],
    mut resolve: impl FnMut(&str) -> Result<String>,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match kind {
        GitObjectKind::Blob => out.extend_from_slice(body),
        GitObjectKind::Tree => {
            let mut tail = body;
            while !tail.is_empty() {
                let nul = tail
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or_else(|| fail("invalid exported tree"))?;
                let old_id = tail
                    .get(nul + 1..nul + 33)
                    .ok_or_else(|| fail("truncated exported tree"))?;
                let new_id = resolve(&hex(old_id))?;
                out.extend_from_slice(&tail[..=nul]);
                for i in (0..new_id.len()).step_by(2) {
                    let byte = u8::from_str_radix(&new_id[i..i + 2], 16)
                        .map_err(|_| fail("invalid remapped Git id"))?;
                    out.push(byte);
                }
                tail = &tail[nul + 33..];
            }
        }
        GitObjectKind::Commit => {
            let mut header = true;
            for line in body.split_inclusive(|b| *b == b'\n') {
                if header && line == b"\n" {
                    header = false;
                }
                if header && (line.starts_with(b"tree ") || line.starts_with(b"parent ")) {
                    let space = line
                        .iter()
                        .position(|b| *b == b' ')
                        .ok_or_else(|| fail("invalid exported commit"))?;
                    let old_id = text(&line[space + 1..])?.trim_end_matches('\n');
                    out.extend_from_slice(&line[..=space]);
                    out.extend_from_slice(resolve(old_id)?.as_bytes());
                    out.push(b'\n');
                } else {
                    out.extend_from_slice(line);
                }
            }
        }
        _ => return Err(fail("unsupported export object kind")),
    }
    Ok(out)
}

// Topological remapping avoids recursive ancestry traversal in this driver.
// SHA-1 exists only inside the explicitly requested convenience mirror;
// signed Mininet object ids continue using their existing strong hashes.
fn write_mirror_objects(
    directory: &Path,
    objects: &[mini_forge::GitObject],
    tip: &str,
    format: &str,
) -> Result<String> {
    let mut pending = BTreeMap::new();
    let mut dependants: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ready = VecDeque::new();
    let mut remaining = BTreeMap::new();
    for object in objects {
        if !valid_oid(&object.id, 64)
            || hex(&mini_crypto::HashAlgorithm::Sha256.digest(&object.bytes)) != object.id
        {
            return Err(fail("invalid source export object hash"));
        }
        let nul = object
            .bytes
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| fail("invalid framed export object"))?;
        let body = &object.bytes[nul + 1..];
        let mut deps = BTreeSet::new();
        rewrite_body(object.kind, body, |id| {
            deps.insert(id.to_string());
            Ok(id.to_string())
        })?;
        if deps.is_empty() {
            ready.push_back(object.id.clone());
        }
        remaining.insert(object.id.clone(), deps.len());
        for dep in deps {
            dependants.entry(dep).or_default().push(object.id.clone());
        }
        pending.insert(object.id.clone(), (object.kind, body));
    }
    let mut mapped = BTreeMap::new();
    while let Some(id) = ready.pop_front() {
        let (kind, body) = pending
            .get(&id)
            .ok_or_else(|| fail("missing export dependency"))?;
        let rewritten = rewrite_body(*kind, body, |old| {
            mapped
                .get(old)
                .cloned()
                .ok_or_else(|| fail("missing export dependency"))
        })?;
        let kind = match kind {
            GitObjectKind::Blob => "blob",
            GitObjectKind::Tree => "tree",
            GitObjectKind::Commit => "commit",
            _ => return Err(fail("unsupported object kind")),
        };
        let actual = run_git(
            directory,
            &["hash-object", "--no-filters", "-w", "-t", kind, "--stdin"],
            Some(&rewritten),
            128,
        )?;
        let actual = text(&actual)?.trim().to_string();
        if !valid_oid(&actual, if format == "sha1" { 40 } else { 64 })
            || (format == "sha256" && actual != id)
        {
            return Err(fail("Git export object hash mismatch"));
        }
        mapped.insert(id.clone(), actual);
        for dep in dependants.get(&id).into_iter().flatten() {
            let count = remaining
                .get_mut(dep)
                .ok_or_else(|| fail("missing export dependency count"))?;
            *count = count
                .checked_sub(1)
                .ok_or_else(|| fail("invalid export dependency count"))?;
            if *count == 0 {
                ready.push_back(dep.clone());
            }
        }
    }
    if mapped.len() != objects.len() {
        return Err(fail("cyclic or missing Git export dependency"));
    }
    mapped
        .remove(tip)
        .ok_or_else(|| fail("missing exported tip"))
}

// Bound the unique native closure before library conversion. The exporter
// independently memoizes the DAG, validates types/tree height and caps commits;
// ancestry depth no longer consumes stack or counts shared subtrees repeatedly.
fn preflight_export(store: &Store<FsBackend>, root: &ObjectId) -> Result<()> {
    let mut stack = vec![root.clone()];
    let mut queued = BTreeSet::from([root.as_str().to_string()]);
    let mut bytes = 0usize;
    let mut commits = 0usize;
    while let Some(id) = stack.pop() {
        let object = store.get(&id).map_err(|e| CliError::Store(e.to_string()))?;
        bytes = bytes
            .checked_add(object.to_bytes().len())
            .ok_or_else(|| fail("export byte budget overflow"))?;
        if bytes > MAX_TOTAL_BYTES {
            return Err(fail("export exceeds 64 MiB unique-object budget"));
        }
        if object.object_type == ObjectType::COMMIT {
            commits += 1;
            if commits > mini_forge::MAX_EXPORT_COMMITS {
                return Err(fail("export exceeds 4096 commits"));
            }
        }
        for link in object.links {
            if queued.insert(link.target.as_str().to_string()) {
                if queued.len() > MAX_EXPORT_OBJECTS {
                    return Err(fail("export exceeds 65536 unique objects"));
                }
                stack.push(link.target);
            }
        }
    }
    Ok(())
}

fn export(
    store_path: &Path,
    id: &str,
    destination: &Path,
    branch: &str,
    format: &str,
) -> Result<CommandResult> {
    if !["sha1", "sha256"].contains(&format) {
        return Err(fail("--object-format must be sha1 or sha256"));
    }
    let id = ObjectId::parse(id).map_err(|e| CliError::Object(e.to_string()))?;
    let full_ref = format!("refs/heads/{branch}");
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize().map_err(io)?;
    let name = destination
        .file_name()
        .ok_or_else(|| fail("--to needs a new directory name"))?;
    let destination = parent.join(name);
    run_git(&parent, &["check-ref-format", &full_ref], None, 128)?;
    let store = crate::store::open_store(store_path)?;
    preflight_export(&store, &id)?;
    let (git_id, objects) = export_commit_chain(&store, &id).map_err(forge)?;
    if objects.len() > MAX_EXPORT_OBJECTS
        || objects
            .iter()
            .try_fold(0usize, |n, o| n.checked_add(o.bytes.len()))
            .is_none_or(|n| n > MAX_TOTAL_BYTES)
    {
        return Err(fail(
            "export exceeds migration limits (65536 unique objects / 64 MiB)",
        ));
    }
    // Never overwrite an existing repository or move any existing refs.
    std::fs::create_dir(&destination).map_err(io)?;
    let result = (|| {
        run_git(
            &destination,
            &[
                "init",
                "--bare",
                &format!("--object-format={format}"),
                "--template=",
            ],
            None,
            4096,
        )?;
        let git_id = write_mirror_objects(&destination, &objects, &git_id, format)?;
        run_git(
            &destination,
            &[
                "update-ref",
                &full_ref,
                &git_id,
                &"0".repeat(if format == "sha1" { 40 } else { 64 }),
            ],
            None,
            4096,
        )?;
        run_git(
            &destination,
            &["symbolic-ref", "HEAD", &full_ref],
            None,
            4096,
        )?;
        Ok(CommandResult::new(format!("{format} Git mirror created: {}\n{full_ref} -> {git_id}\nGit export is a convenience mirror; signatures and governance remain in Mininet.", destination.display()))
            .field("git_commit", JsonValue::str(git_id)).field("ref", JsonValue::str(full_ref))
            .field("destination", JsonValue::str(destination.to_string_lossy())).field("object_format", JsonValue::str(format)))
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&destination);
    }
    result
}

/// `mini git import-snapshot <local-repo> [--ref HEAD|refs/heads/main]
/// [--allow-executable-loss]`, or `mini git export <commit-id> --to <new-dir>
/// [--branch main] [--object-format sha1|sha256]`. Paths are local; this command never fetches or pushes.
pub fn dispatch(home: &Path, store_path: &Path, args: Vec<String>, json: bool) -> Result<String> {
    let usage = "mini git import-history <local-repo> [--ref <exact-ref>] [--max-commits 1..1024 | --batch-commits 1..1024] | mini git import-snapshot <local-repo> [--ref <exact-ref>] [--allow-executable-loss] | mini git export <commit-id> --to <new-dir> [--branch main] [--object-format sha1|sha256]";
    if args.len() < 2 {
        return Err(fail(usage));
    }
    let mut flags = BTreeMap::new();
    let mut allow_loss = false;
    let mut i = 2;
    while i < args.len() {
        let key = &args[i];
        if key == "--allow-executable-loss" && !allow_loss {
            allow_loss = true;
            i += 1;
            continue;
        }
        if ![
            "--ref",
            "--to",
            "--branch",
            "--object-format",
            "--max-commits",
            "--batch-commits",
        ]
        .contains(&key.as_str())
            || i + 1 >= args.len()
            || flags.insert(key.as_str(), args[i + 1].as_str()).is_some()
        {
            return Err(fail(format!(
                "unknown, duplicate, or missing-value Git option: {key}"
            )));
        }
        i += 2;
    }
    let result = match args[0].as_str() {
        "import-snapshot" if flags.keys().all(|key| *key == "--ref") => import_snapshot(
            home,
            store_path,
            Path::new(&args[1]),
            flags.get("--ref").copied().unwrap_or("HEAD"),
            allow_loss,
        )?,
        "import-history"
            if !(allow_loss
                || (flags.contains_key("--max-commits")
                    && flags.contains_key("--batch-commits")))
                && flags
                    .keys()
                    .all(|key| ["--ref", "--max-commits", "--batch-commits"].contains(key)) =>
        {
            import_history(
                home,
                store_path,
                Path::new(&args[1]),
                flags.get("--ref").copied().unwrap_or("HEAD"),
                flags
                    .get("--batch-commits")
                    .or_else(|| flags.get("--max-commits"))
                    .map(|raw| {
                        raw.parse::<usize>()
                            .map_err(|_| fail("invalid --max-commits"))
                    })
                    .transpose()?
                    .unwrap_or(DEFAULT_HISTORY_COMMITS),
                flags
                    .get("--batch-commits")
                    .map(|raw| {
                        raw.parse::<usize>()
                            .map_err(|_| fail("invalid --batch-commits"))
                    })
                    .transpose()?,
            )?
        }
        "export"
            if !allow_loss
                && flags.contains_key("--to")
                && flags
                    .keys()
                    .all(|key| ["--to", "--branch", "--object-format"].contains(key)) =>
        {
            export(
                store_path,
                &args[1],
                Path::new(flags["--to"]),
                flags.get("--branch").copied().unwrap_or("main"),
                flags.get("--object-format").copied().unwrap_or("sha256"),
            )?
        }
        _ => return Err(fail(usage)),
    };
    Ok(result.render(json, &format!("git.{}", args[0])))
}

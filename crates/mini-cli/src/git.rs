//! Local Git snapshot migration and SHA-256 export mirrors. Git metadata is
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

struct Source {
    directory: PathBuf,
    oid_len: usize,
    total_bytes: usize,
    objects: usize,
    manifest: Vec<u8>,
    executable_loss: bool,
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
            &["hash-object", "-t", kind, "--stdin"],
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
                "40000" => SnapshotItem::Tree(self.tree(&child_id, &format!("{full_path}/"), depth + 1)?),
                "100644" => SnapshotItem::File(self.read("blob", &child_id)?),
                "100755" if self.executable_loss => SnapshotItem::File(self.read("blob", &child_id)?),
                "100755" => return Err(fail("executable file: Forge cannot preserve mode; repeat with --allow-executable-loss to record modes in signed provenance and import bytes only")),
                _ => return Err(fail(format!("unsupported Git mode {mode}: symlinks and submodules are not imported"))),
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
        executable_loss,
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
    let header = format!("MINI_GIT_SNAPSHOT_V1\nformat {format}\ncommit {original}\nexecutable_mode_loss {executable_loss}\ncommit_bytes {}\n", original_commit.len());
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
        .field("history_imported", JsonValue::Bool(false)).field("executable_mode_loss_allowed", JsonValue::Bool(executable_loss)))
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
            &["hash-object", "-w", "-t", kind, "--stdin"],
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

// The library exporter predates this driver's budgets and recursively visits
// shared trees. Count occurrences (not just unique objects) before invoking
// it, so a small DAG with many paths cannot trigger exponential work. Cycles
// and deep ancestry fail without recursing in the driver.
fn preflight_export(store: &Store<FsBackend>, root: &ObjectId) -> Result<()> {
    let mut stack = vec![(root.clone(), 0usize)];
    let mut visits = 0usize;
    let mut bytes = 0usize;
    while let Some((id, depth)) = stack.pop() {
        visits += 1;
        if visits > MAX_OBJECTS || depth > 64 {
            return Err(fail(
                "export exceeds traversal budget (4096 object occurrences / 64 graph levels)",
            ));
        }
        let object = store.get(&id).map_err(|e| CliError::Store(e.to_string()))?;
        bytes = bytes
            .checked_add(object.to_bytes().len())
            .ok_or_else(|| fail("export byte budget overflow"))?;
        if bytes > MAX_TOTAL_BYTES {
            return Err(fail("export exceeds 64 MiB traversal budget"));
        }
        if stack.len() + object.links.len() > MAX_OBJECTS {
            return Err(fail("export exceeds pending-object traversal budget"));
        }
        stack.extend(
            object
                .links
                .iter()
                .map(|link| (link.target.clone(), depth + 1)),
        );
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
    if objects.len() > MAX_OBJECTS
        || objects
            .iter()
            .try_fold(0usize, |n, o| n.checked_add(o.bytes.len()))
            .is_none_or(|n| n > MAX_TOTAL_BYTES)
    {
        return Err(fail(
            "export exceeds migration limits (4096 objects / 64 MiB)",
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
    let usage = "mini git import-snapshot <local-repo> [--ref <exact-ref>] [--allow-executable-loss] | mini git export <commit-id> --to <new-dir> [--branch main] [--object-format sha1|sha256]";
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
        if !["--ref", "--to", "--branch", "--object-format"].contains(&key.as_str())
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

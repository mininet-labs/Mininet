//! `mini repo ...` — the file/tree/commit/branch layer, a thin wrapper over
//! `mini_forge`'s already-real primitives.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use did_mini::Did;
use mini_forge::{
    checkout_with_modes, commit, project, put_file, put_tree, resolve_branch, resolve_project,
    set_branch, Policy, TreeEntry,
};
use mini_objects::ObjectId;
use mini_store::{FsBackend, Store};

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use crate::project as project_alias;
use crate::sequence;
use crate::store::{build_oracle, open_store};

/// `mini repo init <name> [--maintainer <did>]... [--min-approvals N]`
pub fn init(
    home: &Path,
    store_path: &Path,
    name: &str,
    maintainers: Vec<Did>,
    min_approvals: u32,
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;

    let maintainers = if maintainers.is_empty() {
        vec![identity.human_did()]
    } else {
        maintainers
    };
    let min_approvals = if min_approvals == 0 { 1 } else { min_approvals };
    let policy = Policy {
        min_approvals,
        maintainers,
    };
    let obj = project(
        &mut store,
        &identity.human_did(),
        &identity.device,
        name,
        &policy,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;
    project_alias::track(home, name, obj.id())?;
    Ok(
        CommandResult::new(format!("project {name:?} created: {}", obj.id().as_str()))
            .field("project_id", JsonValue::str(obj.id().as_str()))
            .field("name", JsonValue::str(name)),
    )
}

/// `mini repo track <name> <project-id>` — record a local alias for a
/// project someone else created.
pub fn track(home: &Path, name: &str, project_id_str: &str) -> Result<String> {
    let id = ObjectId::parse(project_id_str).map_err(|e| CliError::Object(e.to_string()))?;
    project_alias::track(home, name, &id)?;
    Ok(format!("tracking {name:?} -> {}", id.as_str()))
}

/// `mini repo commit <project> --branch <b> --message <m> <path>...`
pub fn commit_paths(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    message: &str,
    paths: &[PathBuf],
) -> Result<CommandResult> {
    if paths.is_empty() {
        return Err(CliError::Usage("commit needs at least one path".into()));
    }
    commit_paths_inner(home, store_path, project_ref, branch, message, paths)
}

fn commit_paths_inner(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    message: &str,
    paths: &[PathBuf],
) -> Result<CommandResult> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let human = identity.human_did();

    let project_id = project_alias::resolve(home, project_ref)?;
    let oracle = build_oracle(home, &identity)?;
    let state = resolve_project(&store, &oracle, &project_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;
    if branch.is_empty()
        || branch.len() > mini_forge::MAX_VERSION_BYTES
        || !branch
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || branch == "."
        || branch == ".."
    {
        return Err(CliError::Usage("invalid branch name".into()));
    }
    if message.len() > mini_forge::MAX_MESSAGE_BYTES {
        return Err(CliError::Usage("commit message exceeds forge limit".into()));
    }
    // Validate and read the entire snapshot before creating any objects.
    let snapshot = read_snapshot(paths, 0, &mut SnapshotBudget::default())?;
    let entries = store_snapshot(&mut store, &human, &identity.device, &snapshot)?;
    let tree = put_tree(&mut store, &human, &identity.device, &entries)
        .map_err(|e| CliError::Forge(e.to_string()))?;

    // Raw author branch pointers are not scoped to a project. Only the
    // governed project head is a valid default base for this snapshot.
    let parent = state
        .branches
        .iter()
        .find(|(name, _)| name == branch)
        .map(|(_, id)| id.clone());
    let parents: Vec<ObjectId> = parent.into_iter().collect();

    let seq = sequence::next(home, store_path)?;
    let obj = commit(
        &mut store,
        &human,
        &identity.device,
        message,
        &tree,
        &parents,
        sequence::now_ms(),
        seq,
    )
    .map_err(|e| CliError::Forge(e.to_string()))?;

    Ok(
        CommandResult::new(format!("commit created: {}", obj.id().as_str()))
            .field("commit_id", JsonValue::str(obj.id().as_str()))
            .field("project_id", JsonValue::str(project_id.as_str()))
            .field("branch", JsonValue::str(branch))
            .field("tree", JsonValue::str(tree.as_str()))
            .field(
                "parents",
                JsonValue::strs(parents.iter().map(|id| id.as_str())),
            ),
    )
}

/// Snapshot the contents of a checked-out source directory without adding a
/// wrapper directory, so successive checkout/edit/commit cycles preserve paths.
pub fn commit_workspace(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch: &str,
    message: &str,
    workspace: &Path,
) -> Result<CommandResult> {
    reject_symlink_components(workspace)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(workspace).map_err(|e| CliError::Io(e.to_string()))? {
        paths.push(entry.map_err(|e| CliError::Io(e.to_string()))?.path());
        if paths.len() > mini_forge::MAX_TREE_ENTRIES + 3 {
            return Err(CliError::Usage(
                "workspace exceeds forge entry limit".into(),
            ));
        }
    }
    // An empty directory is an intentional empty snapshot (including deletion
    // of the last file); the positional-path interface still rejects no paths.
    commit_paths_inner(home, store_path, project_ref, branch, message, &paths)
}

enum Snapshot {
    File(String, Vec<u8>, bool),
    Directory(String, Vec<Snapshot>),
}

struct SnapshotBudget {
    files: usize,
    bytes: usize,
    nodes: usize,
}
impl Default for SnapshotBudget {
    fn default() -> Self {
        Self {
            files: mini_forge::MAX_CHECKOUT_FILES,
            bytes: mini_forge::MAX_CHECKOUT_BYTES,
            nodes: mini_forge::MAX_CHECKOUT_FILES
                + mini_forge::MAX_CHECKOUT_FILES * mini_forge::MAX_TREE_DEPTH,
        }
    }
}

fn excluded(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        ".git" | ".mininet" | ".mini"
    )
}

fn read_snapshot(
    paths: &[PathBuf],
    depth: usize,
    budget: &mut SnapshotBudget,
) -> Result<Vec<Snapshot>> {
    if depth > mini_forge::MAX_TREE_DEPTH {
        return Err(CliError::Usage(
            "snapshot exceeds forge tree depth limit".into(),
        ));
    }
    let mut paths = paths.to_vec();
    paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    let mut result = Vec::new();
    let mut previous = None;
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| CliError::Usage(format!("bad path: {}", path.display())))?
            .to_owned();
        if excluded(&name) {
            continue;
        }
        if previous.as_ref() == Some(&name) {
            return Err(CliError::Usage(format!("duplicate snapshot name: {name}")));
        }
        previous = Some(name.clone());
        if !mini_forge::file_name_is_well_formed(&name) {
            return Err(CliError::Usage(format!(
                "unsupported forge filename: {name:?}"
            )));
        }
        if result.len() == mini_forge::MAX_TREE_ENTRIES || budget.nodes == 0 {
            return Err(CliError::Usage("snapshot exceeds forge entry limit".into()));
        }
        budget.nodes -= 1;
        // Never dereference a symlink, including symlinks in an explicitly supplied path.
        reject_symlink_components(&path)?;
        let metadata = fs::symlink_metadata(&path).map_err(|e| CliError::Io(e.to_string()))?;
        if metadata.is_dir() {
            let mut children = Vec::new();
            for entry in fs::read_dir(&path).map_err(|e| CliError::Io(e.to_string()))? {
                children.push(entry.map_err(|e| CliError::Io(e.to_string()))?.path());
                if children.len() > mini_forge::MAX_TREE_ENTRIES + 3 {
                    return Err(CliError::Usage(
                        "directory exceeds forge entry limit".into(),
                    ));
                }
            }
            result.push(Snapshot::Directory(
                name,
                read_snapshot(&children, depth + 1, budget)?,
            ));
        } else if metadata.is_file() {
            let limit = budget.bytes.min(mini_objects::MAX_PAYLOAD_BYTES);
            if budget.files == 0 || metadata.len() > limit as u64 {
                return Err(CliError::Usage(
                    "snapshot exceeds forge file/byte limit".into(),
                ));
            }
            let mut bytes = Vec::new();
            fs::File::open(&path)
                .map_err(|e| CliError::Io(e.to_string()))?
                .take(limit as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| CliError::Io(e.to_string()))?;
            if bytes.len() > limit {
                return Err(CliError::Usage(
                    "file grew beyond snapshot byte limit".into(),
                ));
            }
            budget.files -= 1;
            budget.bytes -= bytes.len();
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            result.push(Snapshot::File(name, bytes, executable));
        } else {
            return Err(CliError::Usage(format!(
                "not a regular file or directory: {}",
                path.display()
            )));
        }
    }
    Ok(result)
}

fn reject_symlink_components(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, std::path::Component::Normal(name) if excluded(&name.to_string_lossy()))
        {
            return Err(CliError::Usage(
                "repository metadata cannot be snapshotted".into(),
            ));
        }
        let metadata = fs::symlink_metadata(&current).map_err(|e| CliError::Io(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(CliError::Usage(format!(
                "symlinks are not supported: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

fn store_snapshot(
    store: &mut Store<FsBackend>,
    human: &Did,
    device: &did_mini::Controller,
    snapshot: &[Snapshot],
) -> Result<Vec<TreeEntry>> {
    snapshot
        .iter()
        .map(|entry| {
            let (name, is_dir, target) = match entry {
                Snapshot::File(name, bytes, executable) => (
                    name,
                    false,
                    (if *executable {
                        mini_forge::put_executable_file(store, human, device, bytes)
                    } else {
                        put_file(store, human, device, bytes)
                    })
                    .map_err(|e| CliError::Forge(e.to_string()))?,
                ),
                Snapshot::Directory(name, children) => {
                    let entries = store_snapshot(store, human, device, children)?;
                    (
                        name,
                        true,
                        put_tree(store, human, device, &entries)
                            .map_err(|e| CliError::Forge(e.to_string()))?,
                    )
                }
            };
            Ok(TreeEntry {
                name: name.clone(),
                is_dir,
                target,
            })
        })
        .collect()
}

/// `mini repo checkout <project> <commit-id> <dest-dir>`
pub fn checkout_commit(
    home: &Path,
    store_path: &Path,
    commit_ref: &str,
    dest: &Path,
) -> Result<String> {
    let store = open_store(store_path)?;
    let commit_id = ObjectId::parse(commit_ref).map_err(|e| CliError::Object(e.to_string()))?;
    let files =
        checkout_with_modes(&store, &commit_id).map_err(|e| CliError::Forge(e.to_string()))?;
    // Publish only a fully materialized checkout. Existing destinations are
    // refused so local edits can never be silently overwritten.
    if fs::symlink_metadata(dest).is_ok() {
        return Err(CliError::Usage(
            "checkout destination already exists".into(),
        ));
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    reject_symlink_components(parent)?;
    let parent = fs::canonicalize(parent).map_err(|e| CliError::Io(e.to_string()))?;
    let mut staging = None;
    for attempt in 0..100 {
        let path = parent.join(format!(
            ".mini-checkout-{}-{}-{attempt}",
            std::process::id(),
            sequence::now_ms()
        ));
        match fs::create_dir(&path) {
            Ok(()) => {
                staging = Some(path);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(CliError::Io(e.to_string())),
        }
    }
    let staging = staging
        .ok_or_else(|| CliError::Io("could not allocate checkout staging directory".into()))?;
    let write_result = (|| -> Result<()> {
        for (path, bytes, executable) in &files {
            #[cfg(not(unix))]
            if *executable {
                return Err(CliError::Usage("this platform cannot materialize executable source modes; use a Unix workspace".into()));
            }
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || Path::new(path)
                    .components()
                    .any(|c| excluded(&c.as_os_str().to_string_lossy()))
            {
                return Err(CliError::Usage("unsafe checkout path".into()));
            }
            let full = staging.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|e| CliError::Io(e.to_string()))?;
            }
            fs::write(&full, bytes).map_err(|e| CliError::Io(e.to_string()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Preserve umask's read/write restrictions, changing only the
                // portable executable bit on newly staged regular files.
                let mode = fs::metadata(&full)
                    .map_err(|e| CliError::Io(e.to_string()))?
                    .permissions()
                    .mode();
                let mode = if *executable {
                    mode | ((mode & 0o444) >> 2)
                } else {
                    mode & !0o111
                };
                fs::set_permissions(&full, fs::Permissions::from_mode(mode))
                    .map_err(|e| CliError::Io(e.to_string()))?;
            }
        }
        if fs::symlink_metadata(dest).is_ok() {
            return Err(CliError::Usage(
                "checkout destination already exists".into(),
            ));
        }
        fs::rename(&staging, dest).map_err(|e| CliError::Io(e.to_string()))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    write_result?;
    let _ = home;
    Ok(format!(
        "checked out {} files to {}",
        files.len(),
        dest.display()
    ))
}

/// `mini repo branch <project> <branch> [--set <commit-id>]`
pub fn branch(
    home: &Path,
    store_path: &Path,
    project_ref: &str,
    branch_name: &str,
    set_to: Option<&str>,
) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let human = identity.human_did();

    if let Some(commit_ref) = set_to {
        let commit_id = ObjectId::parse(commit_ref).map_err(|e| CliError::Object(e.to_string()))?;
        let seq = sequence::next(home, store_path)?;
        set_branch(
            &mut store,
            &human,
            &identity.device,
            branch_name,
            &commit_id,
            seq,
        )
        .map_err(|e| CliError::Forge(e.to_string()))?;
        return Ok(format!(
            "raw branch pointer {branch_name:?} set to {} -- NOT canonical until governance resolves it (see `mini repo status`)",
            commit_id.as_str()
        ));
    }

    let raw =
        resolve_branch(&store, &human, branch_name).map_err(|e| CliError::Forge(e.to_string()))?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let oracle = build_oracle(home, &identity)?;
    let canonical = resolve_project(&store, &oracle, &project_id)
        .ok()
        .and_then(|s| {
            s.branches
                .into_iter()
                .find(|(n, _)| n == branch_name)
                .map(|(_, c)| c)
        });

    Ok(format!(
        "{branch_name:?}: raw pointer = {:?}, governed canonical = {:?}",
        raw.map(|i| i.as_str().to_string()),
        canonical.map(|i| i.as_str().to_string())
    ))
}

/// `mini repo status <project>`
pub fn status(home: &Path, store_path: &Path, project_ref: &str) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let store = open_store(store_path)?;
    let project_id = project_alias::resolve(home, project_ref)?;
    let oracle = build_oracle(home, &identity)?;
    let state = resolve_project(&store, &oracle, &project_id)
        .map_err(|e| CliError::Forge(e.to_string()))?;

    let mut out = format!(
        "project {:?}: {} entries applied, {} maintainer(s), min_approvals={}, forks_detected={}\n",
        state.name,
        state.entries,
        state.policy.maintainers.len(),
        state.policy.min_approvals,
        state.forks_detected
    );
    for (name, head) in &state.branches {
        out.push_str(&format!("  {name} -> {}\n", head.as_str()));
    }
    Ok(out)
}

#[cfg(test)]
mod workspace_tests {
    use super::*;

    #[test]
    fn snapshot_is_sorted_and_omits_repository_metadata() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("z.rs"), b"z").unwrap();
        fs::write(dir.path().join("a.rs"), b"a").unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/config"), b"credential").unwrap();
        let paths = [
            dir.path().join("z.rs"),
            dir.path().join(".git"),
            dir.path().join("a.rs"),
        ];
        let entries = read_snapshot(&paths, 0, &mut SnapshotBudget::default()).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            matches!(&entries[0], Snapshot::File(name, bytes, _) if name == "a.rs" && bytes == b"a")
        );
        assert!(matches!(&entries[1], Snapshot::File(name, _, _) if name == "z.rs"));
    }

    #[test]
    fn snapshot_excludes_case_variants_of_secret_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for name in [".GiT", ".MININET", ".Mini"] {
            let path = dir.path().join(name);
            fs::create_dir(&path).unwrap();
            fs::write(path.join("identity.seed"), b"must not be published").unwrap();
            paths.push(path);
        }
        assert!(read_snapshot(&paths, 0, &mut SnapshotBudget::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn snapshot_accepts_checkout_sized_files_with_directory_overhead() {
        let root = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        for directory in 0..128 {
            let path = root.path().join(format!("dir-{directory}"));
            fs::create_dir(&path).unwrap();
            for file in 0..32 {
                fs::write(path.join(format!("file-{file}")), b"x").unwrap();
            }
            paths.push(path);
        }
        let mut budget = SnapshotBudget {
            files: 128 * 32,
            ..SnapshotBudget::default()
        };
        assert!(read_snapshot(&paths, 0, &mut budget).is_ok());
        assert_eq!(budget.files, 0);
        let extra = root.path().join("extra");
        fs::write(&extra, b"x").unwrap();
        assert!(read_snapshot(&[extra], 0, &mut budget).is_err());
    }

    #[test]
    fn snapshot_refuses_duplicate_names_and_exhausted_budget() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("code.rs");
        fs::write(&path, b"hello").unwrap();
        assert!(read_snapshot(
            &[path.clone(), path.clone()],
            0,
            &mut SnapshotBudget::default()
        )
        .is_err());
        let mut budget = SnapshotBudget {
            files: 1,
            bytes: 4,
            nodes: 1,
        };
        assert!(read_snapshot(&[path], 0, &mut budget).is_err());
        assert!(read_snapshot(
            &[],
            mini_forge::MAX_TREE_DEPTH + 1,
            &mut SnapshotBudget::default()
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_refuses_symlink_files_and_ancestor_directories() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret");
        fs::write(&secret, b"private").unwrap();
        let link = dir.path().join("link");
        symlink(&secret, &link).unwrap();
        assert!(read_snapshot(&[link], 0, &mut SnapshotBudget::default()).is_err());
        let alias = dir.path().join("alias");
        symlink(dir.path(), &alias).unwrap();
        assert!(read_snapshot(&[alias.join("secret")], 0, &mut SnapshotBudget::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn native_workspace_preserves_executable_modes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let store = dir.path().join("store");
        init(&home, &store, "demo", vec![], 1).unwrap();
        let workspace = dir.path().join("source");
        fs::create_dir(&workspace).unwrap();
        let script = workspace.join("run.sh");
        let text = workspace.join("text.sh");
        fs::write(&script, b"#!/bin/sh\necho hello\n").unwrap();
        fs::write(&text, b"text").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();
        fs::set_permissions(&text, fs::Permissions::from_mode(0o640)).unwrap();
        let result = commit_workspace(&home, &store, "demo", "main", "modes", &workspace).unwrap();
        let id = result.human.strip_prefix("commit created: ").unwrap();
        let dest = dir.path().join("checkout");
        checkout_commit(&home, &store, id, &dest).unwrap();
        assert_ne!(
            fs::metadata(dest.join("run.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
        assert_eq!(
            fs::metadata(dest.join("text.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0
        );
        let modes = commit_workspace(&home, &store, "demo", "main", "again", &dest).unwrap();
        let id = ObjectId::parse(modes.human.strip_prefix("commit created: ").unwrap()).unwrap();
        let store = open_store(&store).unwrap();
        let files = checkout_with_modes(&store, &id).unwrap();
        assert!(
            files
                .iter()
                .find(|(name, _, _)| name == "run.sh")
                .unwrap()
                .2
        );
        assert!(
            !files
                .iter()
                .find(|(name, _, _)| name == "text.sh")
                .unwrap()
                .2
        );
    }

    #[test]
    fn failed_checkout_preserves_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let store = dir.path().join("store");
        init(&home, &store, "demo", vec![], 1).unwrap();
        let source = dir.path().join("source.rs");
        fs::write(&source, b"source").unwrap();
        let output = commit_paths(&home, &store, "demo", "main", "initial", &[source]).unwrap();
        let id = output.human.strip_prefix("commit created: ").unwrap();
        let dest = dir.path().join("checkout");
        checkout_commit(&home, &store, id, &dest).unwrap();
        fs::write(dest.join("source.rs"), b"local edit").unwrap();
        assert!(checkout_commit(&home, &store, id, &dest).is_err());
        assert_eq!(fs::read(dest.join("source.rs")).unwrap(), b"local edit");
        let edited = commit_workspace(&home, &store, "demo", "main", "edit", &dest).unwrap();
        let edited_id = edited.human.strip_prefix("commit created: ").unwrap();
        let second = dir.path().join("second");
        checkout_commit(&home, &store, edited_id, &second).unwrap();
        assert_eq!(fs::read(second.join("source.rs")).unwrap(), b"local edit");
        assert!(!second.join("checkout").exists());
        fs::remove_file(second.join("source.rs")).unwrap();
        let empty = commit_workspace(&home, &store, "demo", "main", "delete", &second).unwrap();
        let empty_id = empty.human.strip_prefix("commit created: ").unwrap();
        let third = dir.path().join("third");
        checkout_commit(&home, &store, empty_id, &third).unwrap();
        assert_eq!(fs::read_dir(third).unwrap().count(), 0);
    }
}

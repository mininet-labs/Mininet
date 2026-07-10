//! `mini repo ...` — the file/tree/commit/branch layer, a thin wrapper over
//! `mini_forge`'s already-real primitives.

use std::fs;
use std::path::{Path, PathBuf};

use did_mini::Did;
use mini_forge::{
    checkout, commit, project, put_file, put_tree, resolve_branch, resolve_project, set_branch,
    Policy, TreeEntry,
};
use mini_objects::ObjectId;
use mini_store::{FsBackend, Store};

use crate::error::{CliError, Result};
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
) -> Result<String> {
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
    Ok(format!("project {name:?} created: {}", obj.id().as_str()))
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
) -> Result<String> {
    let identity = crate::identity::load_or_init(home)?;
    let mut store = open_store(store_path)?;
    let human = identity.human_did();

    if paths.is_empty() {
        return Err(CliError::Usage(
            "commit needs at least one path".to_string(),
        ));
    }
    let entries = build_tree_entries(&mut store, &human, &identity.device, paths)?;
    let tree = put_tree(&mut store, &human, &identity.device, &entries)
        .map_err(|e| CliError::Forge(e.to_string()))?;

    let parent =
        resolve_branch(&store, &human, branch).map_err(|e| CliError::Forge(e.to_string()))?;
    let parents: Vec<ObjectId> = parent.into_iter().collect();

    let seq = sequence::next(home)?;
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

    let _ = project_alias::resolve(home, project_ref)?; // validate the project reference exists
    Ok(format!("commit created: {}", obj.id().as_str()))
}

fn build_tree_entries(
    store: &mut Store<FsBackend>,
    human: &Did,
    device: &did_mini::Controller,
    paths: &[PathBuf],
) -> Result<Vec<TreeEntry>> {
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| CliError::Usage(format!("bad path: {}", path.display())))?
            .to_string();
        if path.is_dir() {
            let mut children: Vec<PathBuf> = fs::read_dir(path)
                .map_err(|e| CliError::Io(e.to_string()))?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<_>>()
                .map_err(|e| CliError::Io(e.to_string()))?;
            children.sort();
            let child_entries = build_tree_entries(store, human, device, &children)?;
            let target = put_tree(store, human, device, &child_entries)
                .map_err(|e| CliError::Forge(e.to_string()))?;
            entries.push(TreeEntry {
                name,
                is_dir: true,
                target,
            });
        } else {
            let bytes = fs::read(path).map_err(|e| CliError::Io(e.to_string()))?;
            let target = put_file(store, human, device, &bytes)
                .map_err(|e| CliError::Forge(e.to_string()))?;
            entries.push(TreeEntry {
                name,
                is_dir: false,
                target,
            });
        }
    }
    Ok(entries)
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
    let files = checkout(&store, &commit_id).map_err(|e| CliError::Forge(e.to_string()))?;
    for (path, bytes) in &files {
        let full = dest.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| CliError::Io(e.to_string()))?;
        }
        fs::write(&full, bytes).map_err(|e| CliError::Io(e.to_string()))?;
    }
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
        let seq = sequence::next(home)?;
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

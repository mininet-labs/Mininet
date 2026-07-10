//! Genuine git SHA-256 interop verification — not just self-consistency.
//! Builds a small mini-forge commit chain, exports it, and independently
//! asks the real `git` binary (via `git hash-object`/`git mktree`/
//! `git commit-tree` against a real `git init --object-format=sha256`
//! repository) to compute the same objects from the same inputs, then
//! asserts the ids match byte-for-byte. Skips (rather than fails) if `git`
//! is not on `PATH` or does not support SHA-256 repositories, since this
//! is an environment capability, not something this crate controls.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use did_mini::{Capabilities, Controller};
use mini_forge::{commit, export_commit_chain, put_file, put_tree, GitObjectKind, TreeEntry};
use mini_store::{MemoryBackend, Store};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-forge-git-export-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

/// A real `git init --object-format=sha256` repository, or `None` if this
/// environment's `git` can't do that (older git, or no `git` at all) —
/// callers skip rather than fail in that case.
fn sha256_git_repo() -> Option<PathBuf> {
    let dir = tempdir("repo");
    std::fs::create_dir_all(&dir).ok()?;
    let status = Command::new("git")
        .args(["init", "--object-format=sha256", "-q", "."])
        .current_dir(&dir)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    Some(dir)
}

fn git_hash_blob(repo: &Path, content: &[u8]) -> String {
    let mut child = Command::new("git")
        .args(["hash-object", "-t", "blob", "--stdin", "-w"])
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git hash-object failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// `entries` is `(mode, name, git_id)`; `git mktree` sorts internally
/// regardless of input order, so this is real ground truth independent of
/// whatever order this test happens to list entries in.
fn git_mktree(repo: &Path, entries: &[(&str, &str, &str)]) -> String {
    let mut input = String::new();
    for (mode, name, id) in entries {
        let kind = if *mode == "40000" { "tree" } else { "blob" };
        input.push_str(&format!("{mode} {kind} {id}\t{name}\n"));
    }
    let mut child = Command::new("git")
        .arg("mktree")
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git mktree failed: {input:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[allow(clippy::too_many_arguments)]
fn git_commit_tree(
    repo: &Path,
    tree: &str,
    parents: &[&str],
    message: &str,
    name: &str,
    email: &str,
    ts_secs: u64,
) -> String {
    let mut args = vec!["commit-tree".to_string(), tree.to_string()];
    for p in parents {
        args.push("-p".to_string());
        args.push(p.to_string());
    }
    args.push("-m".to_string());
    args.push(message.to_string());
    let date = format!("@{ts_secs} +0000");
    let child = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .env("GIT_COMMITTER_DATE", &date)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "git commit-tree failed");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn a_single_commit_matches_real_git_exactly() {
    let Some(repo) = sha256_git_repo() else {
        eprintln!("skipping: no sha256-capable git on PATH");
        return;
    };

    let (author, author_dev) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let content = b"pub fn hello() {}".to_vec();
    let file = put_file(&mut store, &author.did(), &author_dev, &content).unwrap();
    let tree = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "lib.rs".into(),
            is_dir: false,
            target: file,
        }],
    )
    .unwrap();
    let c = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "add hello",
        &tree,
        &[],
        123_000,
        1,
    )
    .unwrap();

    let (git_id, objects) = export_commit_chain(&store, c.id()).unwrap();

    // Independently reconstruct the same objects via real git.
    let expected_blob = git_hash_blob(&repo, &content);
    let expected_tree = git_mktree(&repo, &[("100644", "lib.rs", &expected_blob)]);
    let scid = author.did().scid().to_string();
    let name = format!("mini:{scid}");
    let email = format!("{scid}@mininet.invalid");
    let expected_commit =
        git_commit_tree(&repo, &expected_tree, &[], "add hello", &name, &email, 123);

    assert_eq!(git_id, expected_commit, "commit id mismatch vs real git");
    assert!(
        objects
            .iter()
            .any(|o| o.id == expected_blob && o.kind == GitObjectKind::Blob),
        "exported blob id does not match real git's"
    );
    assert!(
        objects
            .iter()
            .any(|o| o.id == expected_tree && o.kind == GitObjectKind::Tree),
        "exported tree id does not match real git's"
    );
    assert_eq!(
        objects.len(),
        3,
        "expected exactly blob+tree+commit, got {objects:?}"
    );

    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn a_two_commit_chain_with_a_subdirectory_matches_real_git() {
    let Some(repo) = sha256_git_repo() else {
        eprintln!("skipping: no sha256-capable git on PATH");
        return;
    };

    let (author, author_dev) = human(20);
    let mut store = Store::new(MemoryBackend::new());

    // First commit: one top-level file.
    let readme = b"# demo".to_vec();
    let readme_blob = put_file(&mut store, &author.did(), &author_dev, &readme).unwrap();
    let tree1 = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "README.md".into(),
            is_dir: false,
            target: readme_blob.clone(),
        }],
    )
    .unwrap();
    let c1 = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "init",
        &tree1,
        &[],
        100_000,
        1,
    )
    .unwrap();

    // Second commit: adds a subdirectory containing a file, parent = c1.
    let nested = b"mod inner;".to_vec();
    let nested_blob = put_file(&mut store, &author.did(), &author_dev, &nested).unwrap();
    let subtree = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "inner.rs".into(),
            is_dir: false,
            target: nested_blob.clone(),
        }],
    )
    .unwrap();
    let tree2 = put_tree(
        &mut store,
        &author.did(),
        &author_dev,
        &[
            TreeEntry {
                name: "README.md".into(),
                is_dir: false,
                target: readme_blob,
            },
            TreeEntry {
                name: "src".into(),
                is_dir: true,
                target: subtree,
            },
        ],
    )
    .unwrap();
    let c2 = commit(
        &mut store,
        &author.did(),
        &author_dev,
        "add src/inner.rs",
        &tree2,
        &[c1.id().clone()],
        200_000,
        2,
    )
    .unwrap();

    let (git_id, objects) = export_commit_chain(&store, c2.id()).unwrap();

    let scid = author.did().scid().to_string();
    let name = format!("mini:{scid}");
    let email = format!("{scid}@mininet.invalid");

    let expected_readme_blob = git_hash_blob(&repo, &readme);
    let expected_tree1 = git_mktree(&repo, &[("100644", "README.md", &expected_readme_blob)]);
    let expected_c1 = git_commit_tree(&repo, &expected_tree1, &[], "init", &name, &email, 100);

    let expected_nested_blob = git_hash_blob(&repo, &nested);
    let expected_subtree = git_mktree(&repo, &[("100644", "inner.rs", &expected_nested_blob)]);
    let expected_tree2 = git_mktree(
        &repo,
        &[
            ("100644", "README.md", &expected_readme_blob),
            ("40000", "src", &expected_subtree),
        ],
    );
    let expected_c2 = git_commit_tree(
        &repo,
        &expected_tree2,
        &[&expected_c1],
        "add src/inner.rs",
        &name,
        &email,
        200,
    );

    assert_eq!(git_id, expected_c2, "second commit id mismatch vs real git");
    let commit_count = objects
        .iter()
        .filter(|o| o.kind == GitObjectKind::Commit)
        .count();
    assert_eq!(commit_count, 2, "expected both ancestor commits exported");
    assert!(objects.iter().any(|o| o.id == expected_c1));

    std::fs::remove_dir_all(&repo).ok();
}

//! Bounded, resumable real Git DAG migration; source identities are metadata,
//! while every imported native object is signed by the local importer.
use std::fs;
use std::path::Path;
use std::process::Command;

use mini_objects::{ObjectId, Payload};

fn git(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-C",
        ])
        .arg(path)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Original Author")
        .env("GIT_AUTHOR_EMAIL", "original@example.invalid")
        .env("GIT_COMMITTER_NAME", "Original Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
fn run(home: &Path, store: &Path, args: &[&str]) -> mini_cli::Result<String> {
    let mut all = vec![
        "--home".into(),
        home.to_string_lossy().into_owned(),
        "--store".into(),
        store.to_string_lossy().into_owned(),
        "--json".into(),
        "git".into(),
    ];
    all.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&all)
}
fn field<'a>(json: &'a str, key: &str) -> &'a str {
    json.split_once(&format!("\"{key}\":\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}
fn root(path: &Path, format: &str) -> String {
    fs::create_dir(path).unwrap();
    git(
        path,
        &[
            "init",
            &format!("--object-format={format}"),
            "--template=",
            "-b",
            "main",
        ],
    );
    fs::write(path.join("run.sh"), "#!/bin/sh\nprintf hello\\n\n").unwrap();
    git(path, &["add", "run.sh"]);
    git(path, &["update-index", "--chmod=+x", "run.sh"]);
    git(path, &["commit", "-m", "Original root"]);
    git(path, &["tag", "batch-root"]);
    git(path, &["rev-parse", "HEAD"])
}
fn checkpoints(home: &Path, format: &str) -> usize {
    fs::read_dir(home.join("git-history").join(format))
        .map(|iter| iter.count())
        .unwrap_or(0)
}

#[test]
fn git_history_sha1_sha256_preserve_merge_order_messages_modes_and_resume() {
    for format in ["sha1", "sha256"] {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("source");
        let original_root = root(&repo, format);
        git(&repo, &["checkout", "-b", "side"]);
        fs::write(repo.join("side.txt"), "side content\n").unwrap();
        git(&repo, &["add", "side.txt"]);
        git(&repo, &["commit", "-m", "Side change"]);
        git(&repo, &["checkout", "main"]);
        fs::write(repo.join("main.txt"), "main content\n").unwrap();
        git(&repo, &["add", "main.txt"]);
        git(&repo, &["commit", "-m", "Main change"]);
        git(&repo, &["merge", "--no-ff", "side", "-m", "Merge side"]);
        let original_tip = git(&repo, &["rev-parse", "HEAD"]);
        let home = temp.path().join("home");
        let store_path = temp.path().join("store");
        let rejected = run(
            &home,
            &store_path,
            &[
                "import-history",
                repo.to_str().unwrap(),
                "--max-commits",
                "1",
            ],
        )
        .unwrap_err();
        assert!(rejected.to_string().contains("never truncated"));
        assert!(matches!(
            mini_cli::identity::load(&home),
            Err(mini_cli::CliError::NotInitialized)
        ));
        let first = run(
            &home,
            &store_path,
            &[
                "import-history",
                repo.to_str().unwrap(),
                "--ref",
                "refs/tags/batch-root",
                "--max-commits",
                "1",
            ],
        )
        .unwrap();
        assert_eq!(field(&first, "source_commit"), original_root);
        assert!(first.contains("\"imported_commits\":1"));
        assert_eq!(checkpoints(&home, format), 1);
        assert!(run(
            &home,
            &store_path,
            &[
                "import-history",
                repo.to_str().unwrap(),
                "--max-commits",
                "2"
            ]
        )
        .is_err());
        assert_eq!(
            checkpoints(&home, format),
            1,
            "over-budget import must not partially sign"
        );
        let result = run(
            &home,
            &store_path,
            &[
                "import-history",
                repo.to_str().unwrap(),
                "--max-commits",
                "3",
            ],
        )
        .unwrap();
        assert_eq!(field(&result, "source_commit"), original_tip);
        assert!(result.contains("\"imported_commits\":3"));
        assert!(result.contains("\"resumed_frontiers\":1"));
        let repeated = run(
            &home,
            &store_path,
            &[
                "import-history",
                repo.to_str().unwrap(),
                "--max-commits",
                "1",
            ],
        )
        .unwrap();
        assert_eq!(field(&repeated, "commit_id"), field(&result, "commit_id"));
        assert!(repeated.contains("\"imported_commits\":0"));
        assert_eq!(checkpoints(&home, format), 4);
        let identity = mini_cli::identity::load(&home).unwrap();
        let store = mini_cli::store::open_store(&store_path).unwrap();
        let tip_id = ObjectId::parse(field(&result, "commit_id")).unwrap();
        let tip = store.get(&tip_id).unwrap();
        mini_objects::verify_provenance(&tip, &identity.human.kel(), &identity.device.kel())
            .unwrap();
        assert_eq!(tip.author_human, identity.human_did());
        assert_eq!(tip.payload, Payload::Public(b"Merge side\n".to_vec()));
        let parents: Vec<_> = tip
            .links
            .iter()
            .filter(|link| link.rel == "parent")
            .map(|link| store.get(&link.target).unwrap())
            .collect();
        assert_eq!(parents.len(), 2);
        assert_eq!(
            parents[0].payload,
            Payload::Public(b"Main change\n".to_vec())
        );
        assert_eq!(
            parents[1].payload,
            Payload::Public(b"Side change\n".to_vec())
        );
        let provenance = store
            .get(&ObjectId::parse(field(&result, "provenance_id")).unwrap())
            .unwrap();
        mini_objects::verify_provenance(&provenance, &identity.human.kel(), &identity.device.kel())
            .unwrap();
        assert!(
            matches!(provenance.payload, Payload::Public(bytes) if String::from_utf8_lossy(&bytes).contains("Original Author <original@example.invalid>"))
        );
        for output_format in ["sha1", "sha256"] {
            let mirror = temp.path().join(format!("mirror-{output_format}.git"));
            run(
                &home,
                &store_path,
                &[
                    "export",
                    tip_id.as_str(),
                    "--to",
                    mirror.to_str().unwrap(),
                    "--object-format",
                    output_format,
                ],
            )
            .unwrap();
            assert_eq!(git(&mirror, &["rev-list", "--count", "HEAD"]), "4");
            assert_eq!(
                git(&mirror, &["log", "-1", "--format=%s", "HEAD^1"]),
                "Main change"
            );
            assert_eq!(
                git(&mirror, &["log", "-1", "--format=%s", "HEAD^2"]),
                "Side change"
            );
            assert!(git(&mirror, &["ls-tree", "HEAD", "run.sh"]).starts_with("100755 blob "));
            assert_eq!(git(&mirror, &["show", "HEAD:side.txt"]), "side content");
            git(&mirror, &["fsck", "--full"]);
        }
    }
}

#[test]
fn git_history_rejects_tampered_checkpoint_source_and_missing_ancestors() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    let original = root(&repo, "sha1");
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let first = run(&home, &store, &["import-history", repo.to_str().unwrap()]).unwrap();
    let checkpoint = home.join("git-history/sha1").join(&original);
    let saved = fs::read(&checkpoint).unwrap();
    fs::write(&checkpoint, field(&first, "commit_id")).unwrap();
    assert!(
        run(&home, &store, &["import-history", repo.to_str().unwrap()])
            .unwrap_err()
            .to_string()
            .contains("checkpoint")
    );
    fs::write(&checkpoint, saved).unwrap();
    fs::write(repo.join("second.txt"), "second\n").unwrap();
    git(&repo, &["add", "second.txt"]);
    git(&repo, &["commit", "-m", "Second"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    let original_object = repo
        .join(".git/objects")
        .join(&original[..2])
        .join(&original[2..]);
    let second_object = repo
        .join(".git/objects")
        .join(&second[..2])
        .join(&second[2..]);
    let saved_second = fs::read(&second_object).unwrap();
    fs::remove_file(&second_object).unwrap();
    fs::copy(&original_object, &second_object).unwrap();
    let objects_before = mini_cli::store::open_store(&store)
        .unwrap()
        .all_ids()
        .unwrap();
    let error = run(&home, &store, &["import-history", repo.to_str().unwrap()])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("hash mismatch") || (error.contains("Git ") && error.contains("failed")),
        "corrupted source must fail integrity validation: {error}"
    );
    assert_eq!(checkpoints(&home, "sha1"), 1);
    assert_eq!(
        mini_cli::store::open_store(&store)
            .unwrap()
            .all_ids()
            .unwrap(),
        objects_before,
        "integrity failure must not publish signed objects"
    );
    fs::remove_file(&second_object).unwrap();
    fs::write(&second_object, saved_second).unwrap();
    fs::remove_file(&original_object).unwrap();
    let other_home = temp.path().join("other-home");
    assert!(run(
        &other_home,
        &temp.path().join("other-store"),
        &["import-history", repo.to_str().unwrap()]
    )
    .is_err());
    assert!(matches!(
        mini_cli::identity::load(&other_home),
        Err(mini_cli::CliError::NotInitialized)
    ));
    assert_eq!(checkpoints(&home, "sha1"), 1);
}

#[test]
fn git_history_automatic_batches_continue_same_ref_without_truncating_parents() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    root(&repo, "sha1");
    for index in 1..4 {
        fs::write(repo.join("counter.txt"), index.to_string()).unwrap();
        git(&repo, &["add", "counter.txt"]);
        git(&repo, &["commit", "-m", &format!("Change {index}")]);
    }
    let original_tip = git(&repo, &["rev-parse", "HEAD"]);
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let args = [
        "import-history",
        repo.to_str().unwrap(),
        "--batch-commits",
        "1",
    ];
    let mut completed = None;
    for index in 0..4 {
        let result = run(&home, &store, &args).unwrap();
        assert_eq!(field(&result, "requested_source_commit"), original_tip);
        assert!(result.contains("\"imported_commits\":1"));
        assert_eq!(result.contains("\"complete\":true"), index == 3);
        assert_eq!(result.contains("\"continue_required\":true"), index != 3);
        completed = Some(result);
    }
    let repeated = run(&home, &store, &args).unwrap();
    assert!(repeated.contains("\"imported_commits\":0"));
    assert!(repeated.contains("\"complete\":true"));
    let result = completed.unwrap();
    assert_eq!(field(&result, "commit_id"), field(&repeated, "commit_id"));
    let mirror = temp.path().join("complete.git");
    run(
        &home,
        &store,
        &[
            "export",
            field(&result, "commit_id"),
            "--to",
            mirror.to_str().unwrap(),
            "--object-format",
            "sha1",
        ],
    )
    .unwrap();
    assert_eq!(git(&mirror, &["rev-list", "--count", "HEAD"]), "4");
    assert!(git(&mirror, &["ls-tree", "HEAD", "run.sh"]).starts_with("100755 blob "));
    git(&mirror, &["fsck", "--full"]);
}

#[test]
fn git_history_busy_lock_prevents_publication_and_retry_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    root(&repo, "sha1");
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    mini_durable::create_dir_all(&home).unwrap();
    let owner = mini_durable::try_lock_exclusive(&home.join("git-history.lock")).unwrap();
    let args = ["import-history", repo.to_str().unwrap()];
    let error = run(&home, &store, &args).unwrap_err();
    assert!(error.to_string().contains("history import is busy"));
    assert_eq!(checkpoints(&home, "sha1"), 0);
    assert!(!store.exists());
    assert!(matches!(
        mini_cli::identity::load(&home),
        Err(mini_cli::CliError::NotInitialized)
    ));
    drop(owner);
    let first = run(&home, &store, &args).unwrap();
    let second = run(&home, &store, &args).unwrap();
    assert_eq!(field(&first, "commit_id"), field(&second, "commit_id"));
    assert!(second.contains("\"imported_commits\":0"));
    assert_eq!(checkpoints(&home, "sha1"), 1);
}

#[test]
fn git_history_recovers_published_mapping_after_index_loss() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    let source = root(&repo, "sha1");
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let args = ["import-history", repo.to_str().unwrap()];
    let first = run(&home, &store, &args).unwrap();
    fs::remove_file(home.join("git-history/sha1").join(source)).unwrap();
    let second = run(&home, &store, &args).unwrap();
    assert_eq!(field(&first, "commit_id"), field(&second, "commit_id"));
    assert_eq!(
        field(&first, "provenance_id"),
        field(&second, "provenance_id")
    );
    assert!(second.contains("\"imported_commits\":0"));
    assert_eq!(checkpoints(&home, "sha1"), 1);
}

#[test]
fn history_batch_streams_past_the_old_4096_commit_enumeration_cap() {
    use std::io::Write;
    use std::process::Stdio;
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--quiet"]);
    let mut child = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for _ in 0..4100 {
        input.write_all(b"commit refs/heads/main\ncommitter Import Test <test@example.invalid> 1 +0000\ndata 1\nx\n\n").unwrap();
    }
    input.write_all(b"done\n").unwrap();
    drop(input);
    assert!(child.wait().unwrap().success());
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let args = [
        "import-history",
        repo.to_str().unwrap(),
        "--ref",
        "refs/heads/main",
        "--batch-commits",
        "1",
    ];
    let first = run(&home, &store, &args).unwrap();
    assert!(first.contains("\"complete\":false"));
    assert_eq!(checkpoints(&home, "sha1"), 1);
    let second = run(&home, &store, &args).unwrap();
    assert_ne!(field(&first, "commit_id"), field(&second, "commit_id"));
    assert_eq!(checkpoints(&home, "sha1"), 2);
}

#[test]
fn resumed_history_fails_when_a_native_ancestor_was_lost_from_the_store() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    root(&repo, "sha1");
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let first = run(&home, &store, &["import-history", repo.to_str().unwrap()]).unwrap();
    let root_native = field(&first, "commit_id").to_string();
    fs::write(repo.join("second.txt"), "second\n").unwrap();
    git(&repo, &["add", "second.txt"]);
    git(&repo, &["commit", "-m", "Second"]);
    let second = run(&home, &store, &["import-history", repo.to_str().unwrap()]).unwrap();
    assert_ne!(field(&second, "commit_id"), root_native);
    // A partial restore: the checkpointed tip survives but its parent does not.
    let lost = store
        .join("blobs")
        .join(&root_native[..2])
        .join(&root_native);
    assert!(
        lost.exists(),
        "the native root commit is stored at {lost:?}"
    );
    fs::remove_file(&lost).unwrap();
    let error = run(&home, &store, &["import-history", repo.to_str().unwrap()])
        .expect_err("a resumed import must not report broken ancestry as complete")
        .to_string();
    assert!(
        error.contains("not found") || error.contains("ancestry"),
        "unexpected error: {error}"
    );
}

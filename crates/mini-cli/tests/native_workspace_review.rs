//! Exact source review before native signing, without executing source or Git.
use serde_json::Value;
use std::fs;
use std::path::Path;

fn call(home: &Path, store: &Path, args: &[&str], json: bool) -> mini_cli::Result<String> {
    let mut all = vec![
        "--home".into(),
        home.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
    ];
    if json {
        all.push("--json".into());
    }
    all.extend(args.iter().map(|a| a.to_string()));
    mini_cli::run(&all)
}
fn run(home: &Path, store: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&call(home, store, args, true).unwrap()).unwrap()
}
fn state(root: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, path: &Path, output: &mut Vec<(String, String)>) {
        if !path.exists() {
            return;
        }
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, output);
            } else {
                output.push((
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    blake3::hash(&fs::read(&path).unwrap()).to_hex().to_string(),
                ));
            }
        }
    }
    let mut output = vec![];
    walk(root, root, &mut output);
    output.sort();
    output
}
#[test]
fn source_preview_is_read_only_complete_and_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.rs"), "unpublished source bytes").unwrap();
    fs::write(root.join("binary.dat"), [0, 255]).unwrap();
    fs::create_dir(root.join("empty")).unwrap();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "excluded metadata").unwrap();
    fs::write(root.join(".resident-draft.json"), "private checkpoint").unwrap();
    let args = [
        "repo",
        "snapshot-preview",
        "--workspace",
        root.to_str().unwrap(),
    ];
    let first = run(&home, &store, &args);
    let again = run(&home, &store, &args);
    assert_eq!(first, again);
    assert!(!home.exists() && !store.exists());
    assert!(!first.to_string().contains("unpublished source bytes"));
    let entries = first["entries"].as_array().unwrap();
    let paths = entries
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(paths, ["a.rs", "binary.dat", "empty"]);
    assert_eq!(first["files"], 2);
    assert_eq!(first["directories"], 1);
    fs::write(root.join("a.rs"), "changed source bytes").unwrap();
    let changed = run(&home, &store, &args);
    assert_ne!(first["snapshot_id"], changed["snapshot_id"]);
    fs::create_dir(root.join("other_empty")).unwrap();
    assert_ne!(
        changed["snapshot_id"],
        run(&home, &store, &args)["snapshot_id"]
    );
    assert!(!home.exists() && !store.exists());
}
#[test]
fn confirmation_rejects_stale_source_before_store_writes_and_never_initializes_identity() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lib.rs"), "original").unwrap();
    run(&home, &store, &["repo", "init", "review-demo"]);
    let preview = run(
        &home,
        &store,
        &[
            "repo",
            "snapshot-preview",
            "--workspace",
            root.to_str().unwrap(),
        ],
    );
    let id = preview["snapshot_id"].as_str().unwrap();
    let before_home = state(&home);
    let before_store = state(&store);
    let args = [
        "repo",
        "commit",
        "review-demo",
        "--branch",
        "main",
        "--message",
        "reviewed snapshot",
        "--workspace",
        root.to_str().unwrap(),
        "--confirm-snapshot",
        id,
    ];
    fs::write(root.join("lib.rs"), "owner changed it").unwrap();
    assert!(call(&home, &store, &args, true).is_err());
    assert_eq!(state(&home), before_home);
    assert_eq!(state(&store), before_store);
    fs::write(root.join("lib.rs"), "original").unwrap();
    let missing = temp.path().join("uninitialized");
    assert!(call(&missing, &store, &args, true).is_err());
    assert!(!missing.exists());
    assert_eq!(state(&store), before_store);
    let committed = run(&home, &store, &args);
    assert_eq!(committed["snapshot_id"], id);
    let checkout = temp.path().join("checkout");
    call(
        &home,
        &store,
        &[
            "repo",
            "checkout",
            committed["commit_id"].as_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(checkout.join("lib.rs")).unwrap(),
        "original"
    );
    assert!(call(
        &home,
        &store,
        &[
            "repo",
            "commit",
            "review-demo",
            "--branch",
            "main",
            "--message",
            "reject",
            "--confirm-snapshot",
            id,
            root.join("lib.rs").to_str().unwrap()
        ],
        true
    )
    .is_err());
}

#[test]
fn full_commit_plan_binds_reviewed_source_and_signing_metadata_before_any_writes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lib.rs"), "source").unwrap();
    run(&home, &store, &["repo", "init", "plan-demo"]);
    run(&home, &store, &["repo", "init", "other-demo"]);
    let root_arg = root.to_str().unwrap();
    let preview = |project: &str, branch: &str, message: &str| {
        run(
            &home,
            &store,
            &[
                "repo",
                "commit-preview",
                project,
                "--branch",
                branch,
                "--message",
                message,
                "--workspace",
                root_arg,
            ],
        )
    };
    let before_home = state(&home);
    let before_store = state(&store);
    let plan = preview("plan-demo", "main", "reviewed message");
    let id = plan["plan_id"].as_str().unwrap();
    assert_eq!(plan["branch"], "main");
    assert_eq!(plan["message"], "reviewed message");
    assert!(plan["parent"].is_null());
    assert!(plan["human_did"].as_str().unwrap().starts_with("did:mini:"));
    assert!(plan["device_did"]
        .as_str()
        .unwrap()
        .starts_with("did:mini:"));
    assert_eq!(
        preview("plan-demo", "main", "reviewed message")["plan_id"],
        id
    );
    for (project, branch, message) in [
        ("plan-demo", "feature", "reviewed message"),
        ("plan-demo", "main", "different message"),
        ("other-demo", "main", "reviewed message"),
    ] {
        let changed = preview(project, branch, message);
        assert_ne!(changed["plan_id"], id);
        assert_eq!(changed["snapshot_id"], plan["snapshot_id"]);
        assert!(call(
            &home,
            &store,
            &[
                "repo",
                "commit",
                project,
                "--branch",
                branch,
                "--message",
                message,
                "--workspace",
                root_arg,
                "--confirm-plan",
                id
            ],
            true
        )
        .is_err());
        assert_eq!(state(&store), before_store);
        assert_eq!(state(&home), before_home);
    }
    fs::write(root.join("lib.rs"), "changed source").unwrap();
    assert!(call(
        &home,
        &store,
        &[
            "repo",
            "commit",
            "plan-demo",
            "--branch",
            "main",
            "--message",
            "reviewed message",
            "--workspace",
            root_arg,
            "--confirm-plan",
            id
        ],
        true
    )
    .is_err());
    assert_eq!(state(&store), before_store);
    assert_eq!(state(&home), before_home);
    fs::write(root.join("lib.rs"), "source").unwrap();
    assert!(call(
        &home,
        &store,
        &[
            "repo",
            "commit",
            "plan-demo",
            "--branch",
            "main",
            "--message",
            "reviewed message",
            "--workspace",
            root_arg,
            "--confirm-plan",
            id,
            "--confirm-snapshot",
            plan["snapshot_id"].as_str().unwrap()
        ],
        true
    )
    .is_err());
    assert_eq!(state(&store), before_store);
    let accepted = run(
        &home,
        &store,
        &[
            "repo",
            "commit",
            "plan-demo",
            "--branch",
            "main",
            "--message",
            "reviewed message",
            "--workspace",
            root_arg,
            "--confirm-plan",
            id,
        ],
    );
    assert_eq!(accepted["plan_id"], id);
    assert_eq!(accepted["snapshot_id"], plan["snapshot_id"]);
    let checkout = temp.path().join("result");
    call(
        &home,
        &store,
        &[
            "repo",
            "checkout",
            accepted["commit_id"].as_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(checkout.join("lib.rs")).unwrap(),
        "source"
    );
}

#[test]
fn proposal_plan_handoff_binds_metadata_without_approving_or_merging() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let root = temp.path().join("source");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("lib.rs"), "reviewed source").unwrap();
    run(&home, &store, &["repo", "init", "handoff"]);
    let source = root.to_str().unwrap();
    let plan = run(
        &home,
        &store,
        &[
            "repo",
            "commit-preview",
            "handoff",
            "--branch",
            "main",
            "--message",
            "source",
            "--workspace",
            source,
        ],
    );
    let commit = run(
        &home,
        &store,
        &[
            "repo",
            "commit",
            "handoff",
            "--branch",
            "main",
            "--message",
            "source",
            "--workspace",
            source,
            "--confirm-plan",
            plan["plan_id"].as_str().unwrap(),
        ],
    );
    let head = commit["commit_id"].as_str().unwrap();
    let before_home = state(&home);
    let before_store = state(&store);
    let proposal = run(
        &home,
        &store,
        &[
            "pr",
            "propose-preview",
            "handoff",
            "--branch",
            "main",
            "--title",
            "Reviewed contribution",
            "--head",
            head,
        ],
    );
    let id = proposal["plan_id"].as_str().unwrap();
    assert_eq!(proposal["head"], head);
    assert_eq!(before_home, state(&home));
    assert_eq!(before_store, state(&store));
    for (branch, title) in [
        ("feature", "Reviewed contribution"),
        ("main", "Changed title"),
    ] {
        assert!(call(
            &home,
            &store,
            &[
                "pr",
                "propose",
                "handoff",
                "--branch",
                branch,
                "--title",
                title,
                "--head",
                head,
                "--confirm-plan",
                id
            ],
            true
        )
        .is_err());
        assert_eq!(before_home, state(&home));
        assert_eq!(before_store, state(&store));
    }
    let submitted = run(
        &home,
        &store,
        &[
            "pr",
            "propose",
            "handoff",
            "--branch",
            "main",
            "--title",
            "Reviewed contribution",
            "--head",
            head,
            "--confirm-plan",
            id,
        ],
    );
    assert_eq!(submitted["plan_id"], id);
    let view = run(
        &home,
        &store,
        &["pr", "show", submitted["pr_id"].as_str().unwrap()],
    );
    assert_eq!(view["proposal"]["head"], head);
    assert_eq!(view["proposal"]["approvals"], 0);
    assert_eq!(view["proposal"]["quorum_met"], false);
    assert!(view["proposal"]["current_branch_head"].is_null());
}

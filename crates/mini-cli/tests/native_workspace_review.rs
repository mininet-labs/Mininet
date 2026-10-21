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

//! Real local Git migration, without GitHub, network access, checkout hooks,
//! assumed source-author signatures, or SHA-1 as a Mininet trust root.
use std::fs;
use std::path::Path;
use std::process::Command;

use mini_objects::{ObjectId, ObjectType, Payload};

fn git_cmd(path: &Path, args: &[&str]) -> String {
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
        .env("GIT_AUTHOR_NAME", "Source Author")
        .env("GIT_AUTHOR_EMAIL", "source@example.invalid")
        .env("GIT_COMMITTER_NAME", "Source Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@example.invalid")
        .output()
        .expect("Git must be installed for migration tests");
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
fn field<'a>(json: &'a str, key: &str) -> &'a str {
    json.split_once(&format!("\"{key}\":\""))
        .unwrap()
        .1
        .split('"')
        .next()
        .unwrap()
}
fn dispatch(home: &Path, store: &Path, args: &[&str]) -> mini_cli::Result<String> {
    let mut all = vec![
        "--home".to_string(),
        home.to_string_lossy().into_owned(),
        "--store".to_string(),
        store.to_string_lossy().into_owned(),
        "--json".to_string(),
        "git".to_string(),
    ];
    all.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&all)
}
fn fixture(path: &Path, format: &str) -> String {
    fs::create_dir(path).unwrap();
    git_cmd(
        path,
        &[
            "init",
            &format!("--object-format={format}"),
            "--template=",
            "-b",
            "main",
        ],
    );
    fs::create_dir(path.join("src")).unwrap();
    fs::write(path.join("src/main.rs"), b"fn main() {}\n").unwrap();
    fs::write(path.join("binary.dat"), [0, 0xff, 10, 7]).unwrap();
    git_cmd(path, &["add", "--all"]);
    git_cmd(path, &["commit", "-m", "Source commit"]);
    git_cmd(path, &["rev-parse", "HEAD"])
}

#[test]
fn git_bridge_sha1_and_sha256_snapshot_export_preserve_content_and_provenance() {
    for format in ["sha1", "sha256"] {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("source");
        let original = fixture(&repo, format);
        // Snapshot means committed object bytes, not mutable worktree content.
        fs::write(repo.join("src/main.rs"), b"uncommitted and excluded").unwrap();
        let home = temp.path().join("home");
        let store_path = temp.path().join("objects");
        let output = dispatch(
            &home,
            &store_path,
            &[
                "import-snapshot",
                repo.to_str().unwrap(),
                "--ref",
                "refs/heads/main",
            ],
        )
        .unwrap();
        assert_eq!(field(&output, "source_commit"), original);
        assert_eq!(field(&output, "source_format"), format);
        assert!(output.contains("\"history_imported\":false"));
        let identity = mini_cli::identity::load(&home).unwrap();
        let mut store = mini_cli::store::open_store(&store_path).unwrap();
        let imported_id = ObjectId::parse(field(&output, "commit_id")).unwrap();
        let imported = store.get(&imported_id).unwrap();
        mini_objects::verify_provenance(&imported, &identity.human.kel(), &identity.device.kel())
            .unwrap();
        assert_eq!(imported.author_human, identity.human_did());
        let provenance = store
            .get(&ObjectId::parse(field(&output, "provenance_id")).unwrap())
            .unwrap();
        mini_objects::verify_provenance(&provenance, &identity.human.kel(), &identity.device.kel())
            .unwrap();
        assert_eq!(
            provenance.object_type,
            ObjectType::Custom("mini/git-snapshot-provenance-v1".to_string())
        );
        let Payload::Public(payload) = &provenance.payload else {
            panic!("expected public evidence")
        };
        let source = String::from_utf8_lossy(payload);
        assert!(source.contains(&original));
        assert!(source.contains("Source Author <source@example.invalid>"));
        assert!(source.contains("100644 "));
        assert!(source.contains(" src/main.rs\n"));
        let tree_id = imported
            .links
            .iter()
            .find(|link| link.rel == "tree")
            .unwrap()
            .target
            .clone();
        let child = mini_forge::commit(
            &mut store,
            &identity.human_did(),
            &identity.device,
            "Native child commit",
            &tree_id,
            std::slice::from_ref(&imported_id),
            1234,
            1000,
        )
        .unwrap();
        let child_id = child.id().clone();
        for export_format in ["sha256", "sha1"] {
            let mirror = temp.path().join(format!("mirror-{export_format}.git"));
            let exported = dispatch(
                &home,
                &store_path,
                &[
                    "export",
                    child_id.as_str(),
                    "--to",
                    mirror.to_str().unwrap(),
                    "--object-format",
                    export_format,
                ],
            )
            .unwrap();
            assert_eq!(
                git_cmd(&mirror, &["rev-parse", "--show-object-format"]),
                export_format
            );
            assert_eq!(
                git_cmd(&mirror, &["rev-parse", "refs/heads/main"]),
                field(&exported, "git_commit")
            );
            assert_eq!(
                git_cmd(&mirror, &["show", "HEAD:src/main.rs"]),
                "fn main() {}"
            );
            assert!(dispatch(
                &home,
                &store_path,
                &[
                    "export",
                    imported_id.as_str(),
                    "--to",
                    mirror.to_str().unwrap()
                ]
            )
            .is_err());
            assert_eq!(git_cmd(&mirror, &["rev-list", "--count", "HEAD"]), "2");
            assert_eq!(
                git_cmd(&mirror, &["log", "-1", "--format=%s"]),
                "Native child commit"
            );
            assert_eq!(
                git_cmd(&mirror, &["show", "HEAD^:src/main.rs"]),
                "fn main() {}"
            );
            git_cmd(&mirror, &["fsck", "--full"]);
        }
    }
}

#[test]
fn git_bridge_mode_loss_requires_consent_and_unsupported_names_fail_before_identity() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    fixture(&repo, "sha1");
    git_cmd(&repo, &["update-index", "--chmod=+x", "src/main.rs"]);
    git_cmd(&repo, &["commit", "-m", "executable"]);
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    let result = dispatch(&home, &store, &["import-snapshot", repo.to_str().unwrap()]);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("allow-executable-loss"));
    assert!(matches!(
        mini_cli::identity::load(&home),
        Err(mini_cli::CliError::NotInitialized)
    ));
    let output = dispatch(
        &home,
        &store,
        &[
            "import-snapshot",
            repo.to_str().unwrap(),
            "--allow-executable-loss",
        ],
    )
    .unwrap();
    assert!(output.contains("\"executable_mode_loss_allowed\":true"));
    fs::write(repo.join("space name.txt"), "unsupported").unwrap();
    git_cmd(&repo, &["add", "--all"]);
    git_cmd(&repo, &["commit", "-m", "unsupported path"]);
    let other_home = temp.path().join("other-home");
    assert!(dispatch(
        &other_home,
        &store,
        &[
            "import-snapshot",
            repo.to_str().unwrap(),
            "--allow-executable-loss"
        ]
    )
    .unwrap_err()
    .to_string()
    .contains("unsupported Git path"));
    assert!(matches!(
        mini_cli::identity::load(&other_home),
        Err(mini_cli::CliError::NotInitialized)
    ));
}

#[test]
fn git_bridge_rejects_revision_expressions_option_injection_and_unsupported_modes() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("source");
    fixture(&repo, "sha1");
    let home = temp.path().join("home");
    let store = temp.path().join("store");
    for reference in [
        "--help",
        "HEAD~1",
        "main",
        "refs/heads/main^{tree}",
        "refs/heads/../main",
    ] {
        assert!(dispatch(
            &home,
            &store,
            &[
                "import-snapshot",
                repo.to_str().unwrap(),
                "--ref",
                reference
            ]
        )
        .is_err());
    }
    assert!(dispatch(
        &home,
        &store,
        &[
            "import-snapshot",
            repo.to_str().unwrap(),
            "--ref",
            "HEAD",
            "--ref",
            "HEAD"
        ]
    )
    .is_err());
    assert!(dispatch(
        &home,
        &store,
        &["import-snapshot", repo.to_str().unwrap(), "--unknown"]
    )
    .is_err());
    // A submodule entry is not silently converted into an empty directory.
    let commit = git_cmd(&repo, &["rev-parse", "HEAD"]);
    git_cmd(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{commit},module"),
        ],
    );
    git_cmd(&repo, &["commit", "-m", "submodule"]);
    assert!(
        dispatch(&home, &store, &["import-snapshot", repo.to_str().unwrap()])
            .unwrap_err()
            .to_string()
            .contains("unsupported Git mode")
    );
    assert!(matches!(
        mini_cli::identity::load(&home),
        Err(mini_cli::CliError::NotInitialized)
    ));
}

#[test]
fn git_bridge_export_bounds_repeated_tree_work_before_creating_mirror() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let store_path = temp.path().join("store");
    let identity = mini_cli::identity::init(&home).unwrap();
    let mut store = mini_cli::store::open_store(&store_path).unwrap();
    let mut target =
        mini_forge::put_file(&mut store, &identity.human_did(), &identity.device, b"x").unwrap();
    for level in 0..16 {
        let entries = ["left", "right"].map(|name| mini_forge::TreeEntry {
            name: name.to_string(),
            is_dir: level != 0,
            target: target.clone(),
        });
        target = mini_forge::put_tree(
            &mut store,
            &identity.human_did(),
            &identity.device,
            &entries,
        )
        .unwrap();
    }
    let commit = mini_forge::commit(
        &mut store,
        &identity.human_did(),
        &identity.device,
        "bounded export",
        &target,
        &[],
        1,
        1,
    )
    .unwrap();
    let mirror = temp.path().join("mirror.git");
    let error = dispatch(
        &home,
        &store_path,
        &[
            "export",
            commit.id().as_str(),
            "--to",
            mirror.to_str().unwrap(),
        ],
    )
    .unwrap_err();
    assert!(error.to_string().contains("traversal budget"));
    assert!(!mirror.exists());
}

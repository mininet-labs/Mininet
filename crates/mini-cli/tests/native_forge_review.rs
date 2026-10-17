//! Exercise the public collaboration CLI without GitHub or human-text scraping.
use mini_forge::{commit, project, put_file, put_tree, Policy, TreeEntry};
use mini_store::{FsBackend, Store};
use std::path::Path;

fn command(home: &Path, store: &Path, args: &[&str]) -> Result<String, String> {
    let mut argv = vec![
        "--home".into(),
        home.to_str().unwrap().into(),
        "--store".into(),
        store.to_str().unwrap().into(),
    ];
    argv.extend(args.iter().map(|s| s.to_string()));
    mini_cli::run(&argv).map_err(|e| e.to_string())
}
fn run(home: &Path, store: &Path, args: &[&str]) -> String {
    command(home, store, args).unwrap_or_else(|e| panic!("{args:?}: {e}"))
}
fn string_field<'a>(json: &'a str, field: &str) -> &'a str {
    let marker = format!("\"{field}\":\"");
    let value = &json[json.find(&marker).unwrap() + marker.len()..];
    &value[..value.find('"').unwrap()]
}

#[test]
fn native_exact_head_review_and_two_maintainer_merge() {
    let temp = tempfile::tempdir().unwrap();
    let store_path = temp.path().join("store");
    let homes: Vec<_> = ["author", "alice", "bob"]
        .iter()
        .map(|name| temp.path().join(name))
        .collect();
    for home in &homes {
        run(home, &store_path, &["identity", "init"]);
    }
    let bundles: Vec<_> = homes
        .iter()
        .map(|h| run(h, &store_path, &["kel", "export"]))
        .collect();
    for home in &homes {
        for bundle in &bundles {
            run(home, &store_path, &["kel", "trust", bundle]);
        }
    }
    let author = mini_cli::identity::load(&homes[0]).unwrap();
    let alice = mini_cli::identity::load(&homes[1]).unwrap();
    let bob = mini_cli::identity::load(&homes[2]).unwrap();
    let mut store = Store::new(FsBackend::open(&store_path).unwrap());
    let project_id = project(
        &mut store,
        &alice.human_did(),
        &alice.device,
        "native",
        &Policy {
            min_approvals: 2,
            maintainers: vec![alice.human_did(), bob.human_did()],
        },
    )
    .unwrap()
    .id()
    .clone();
    let file = put_file(
        &mut store,
        &author.human_did(),
        &author.device,
        b"fn main() {}\n",
    )
    .unwrap();
    let tree = put_tree(
        &mut store,
        &author.human_did(),
        &author.device,
        &[TreeEntry {
            name: "main.rs".into(),
            is_dir: false,
            target: file,
        }],
    )
    .unwrap();
    // Fixture objects use the library default sequence zero; subsequent CLI
    // writes allocate durable author sequences beginning at one.
    let head = commit(
        &mut store,
        &author.human_did(),
        &author.device,
        "native code",
        &tree,
        &[],
        0,
        0,
    )
    .unwrap();
    let other_head = commit(
        &mut store,
        &author.human_did(),
        &author.device,
        "other code",
        &tree,
        &[],
        0,
        0,
    )
    .unwrap();
    drop(store);
    let project = project_id.as_str();
    let head = head.id().as_str();
    let proposal = run(
        &homes[0],
        &store_path,
        &[
            "--json",
            "pr",
            "propose",
            project,
            "--branch",
            "main",
            "--title",
            "native review",
            "--head",
            head,
        ],
    );
    assert!(proposal.contains("\"ok\":true"));
    assert_eq!(string_field(&proposal, "head"), head);
    let pr = string_field(&proposal, "pr_id");
    let listing = run(&homes[1], &store_path, &["--json", "pr", "list", project]);
    assert!(listing.contains(pr));
    let diff = run(&homes[1], &store_path, &["--json", "pr", "diff", pr]);
    assert!(diff.contains("\"status\":\"added\""));
    assert!(diff.contains("fn main() {}\\n"));
    let review = run(
        &homes[1],
        &store_path,
        &[
            "--json",
            "pr",
            "review",
            pr,
            "--head",
            head,
            "--findings",
            "AI evidence only",
        ],
    );
    assert!(review.contains("\"findings_id\":"));
    let show = run(&homes[1], &store_path, &["--json", "pr", "show", pr]);
    assert!(show.contains("AI evidence only"));
    assert!(show.contains("\"approvals\":0"));
    assert!(show.contains("\"reviews\":[]"));
    assert!(command(&homes[1], &store_path, &["pr", "merge", project, pr]).is_err());
    assert!(command(
        &homes[1],
        &store_path,
        &["pr", "approve", pr, "--head", other_head.id().as_str()]
    )
    .is_err());
    assert!(command(
        &homes[1],
        &store_path,
        &[
            "pr",
            "review",
            pr,
            "--head",
            other_head.id().as_str(),
            "--findings",
            "wrong head"
        ]
    )
    .is_err());
    assert!(command(
        &homes[1],
        &store_path,
        &["pr", "approve", pr, "--head", head, "--unknown"]
    )
    .is_err());
    for home in [&homes[1], &homes[2]] {
        let verdict = run(
            home,
            &store_path,
            &["--json", "pr", "approve", pr, "--head", head],
        );
        assert!(verdict.contains("\"approved\":true"));
    }
    let ready = run(&homes[1], &store_path, &["--json", "pr", "show", pr]);
    assert!(ready.contains("\"approvals\":2"));
    assert!(ready.contains("\"eligible_approval\":true"));
    let merged = run(
        &homes[1],
        &store_path,
        &["--json", "pr", "merge", project, pr],
    );
    assert!(merged.contains("\"applied\":true"));
    let stale = run(&homes[1], &store_path, &["--json", "pr", "show", pr]);
    assert!(stale.contains("\"stale\":true"));
    assert!(command(&homes[1], &store_path, &["pr", "merge", project, pr]).is_err());
}

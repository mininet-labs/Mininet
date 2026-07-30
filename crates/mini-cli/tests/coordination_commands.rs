//! Exercises the Forge-native contributor flow through the real `mini` CLI:
//! charter -> task -> explicit claim -> exact-state review handoff.
//!
//! This test uses two persisted identities and one shared object store, which
//! is the same store/sync boundary the CLI already supports. It proves the
//! coordination graph is usable without GitHub; it does not claim a live
//! multi-machine sync or any governance authority for these objects.

use std::fs;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-cli-coordination-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|error| panic!("command {args:?} failed: {error}"))
}

fn did_of(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find(|line| line.starts_with(prefix))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

fn last_word(output: &str) -> String {
    output.split_whitespace().last().unwrap().to_string()
}

fn value_after(output: &str, marker: &str) -> String {
    output
        .split(marker)
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string()
}

#[test]
fn two_homes_can_drive_charter_task_claim_and_review() {
    let store = tempdir("store");
    let alice = tempdir("alice");
    let bob = tempdir("bob");
    let store_arg = store.to_str().unwrap().to_string();

    run(&["--home", alice.to_str().unwrap(), "identity", "init"]);
    run(&["--home", bob.to_str().unwrap(), "identity", "init"]);
    let alice_did = did_of(
        &run(&["--home", alice.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let bob_kel = run(&["--home", bob.to_str().unwrap(), "kel", "export"]);
    let alice_kel = run(&["--home", alice.to_str().unwrap(), "kel", "export"]);

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_arg,
        "repo",
        "init",
        "coordination-demo",
        "--maintainer",
        &alice_did,
        "--min-approvals",
        "1",
    ]);
    run(&["--home", bob.to_str().unwrap(), "kel", "trust", &alice_kel]);
    run(&["--home", alice.to_str().unwrap(), "kel", "trust", &bob_kel]);

    let charter_output = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_arg,
        "team",
        "propose",
        "coordination-demo",
        "--group-id",
        "wg-forge-coordination",
        "--name",
        "Forge coordination",
        "--purpose",
        "route bounded contributor work",
        "--path",
        "crates/mini-forge/**",
        "--autonomous",
        "ordinary implementation review",
        "--reserved",
        "invariant amendment",
        "--term-policy",
        "expiring terms",
        "--appeal-policy",
        "cross-group appeal",
    ]);
    let charter = value_after(&charter_output, "working-group charter proposed: ");
    assert!(run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_arg,
        "team",
        "show",
        &charter,
    ])
    .contains("lifecycle: proposed"));

    let task = last_word(&run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_arg,
        "task",
        "create",
        "coordination-demo",
        "--team",
        &charter,
        "--route",
        "rust",
        "--risk",
        "routine",
        "--title",
        "route Forge work",
        "--description",
        "connect a contributor to a bounded task",
        "--path",
        "crates/mini-forge/**",
        "--evidence",
        "cargo test -p mini-forge",
        "--acceptance",
        "another home can inspect the signed graph",
        "--non-goal",
        "no governance authority",
    ]));
    assert!(run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_arg,
        "task",
        "suggest",
        "--route",
        "rust",
        "--path",
        "crates/mini-forge/src/lib.rs",
    ])
    .contains(&task));

    let claim = last_word(&run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_arg,
        "task",
        "claim",
        &task,
        "--role",
        "rust contributor",
        "--path",
        "crates/mini-forge/**",
        "--expires-ms",
        "4102444800000",
        "--notes",
        "will return exact test evidence",
    ]));
    assert!(!claim.is_empty());

    let review = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_arg,
        "task",
        "review",
        &task,
        "--claim",
        &claim,
        "--head",
        &task,
        "--kind",
        "peer",
        "--disposition",
        "observations",
        "--findings",
        "the task is bounded",
        "--evidence",
        "cargo test -p mini-forge",
        "--limitations",
        "no external audit and no approval recorded",
    ]);
    assert!(review.contains("no approval recorded"));

    let show_json = run(&[
        "--json",
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_arg,
        "task",
        "show",
        &task,
    ]);
    assert!(show_json.contains(&format!("\"reviewed_head\":\"{task}\"")));
    assert!(show_json.contains(&format!("\"id\":\"{claim}\"")));

    let _ = fs::remove_dir_all(store);
    let _ = fs::remove_dir_all(alice);
    let _ = fs::remove_dir_all(bob);
}

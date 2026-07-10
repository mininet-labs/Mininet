//! The actual claim this crate exists to prove (Batch 1's exit condition,
//! `docs/design/self-hosted-forge-spine.md`): two developers, each with
//! their own independent `mini` home, can exchange a signed proposed
//! commit, review the exact commit, and reach a governed canonical branch
//! head — with no GitHub, no daemon, no networking code, just a shared
//! filesystem path standing in for "any medium that copies files."
//!
//! Calls `mini_cli::run` directly (not the compiled binary) so this runs
//! as an ordinary, fast `cargo test`.

use std::fs;
use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-e2e-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn run(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned).unwrap_or_else(|e| panic!("command {args:?} failed: {e}"))
}

fn run_err(args: &[&str]) -> mini_cli::CliError {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned).expect_err("expected this command to fail")
}

fn did_of(identity_show_output: &str, which: &str) -> String {
    identity_show_output
        .lines()
        .find(|l| l.starts_with(which))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string()
}

fn last_word(s: &str) -> String {
    s.split_whitespace().last().unwrap().to_string()
}

#[test]
fn two_independent_homes_reach_a_governed_merge_via_a_shared_store_path() {
    let store = tempdir("store");
    let alice = tempdir("alice");
    let bob = tempdir("bob");
    let carol = tempdir("carol");
    let store_flag = store.to_str().unwrap().to_string();

    for home in [&alice, &bob, &carol] {
        run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            &store_flag,
            "identity",
            "init",
        ]);
    }

    let alice_did = did_of(
        &run(&["--home", alice.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let bob_did = did_of(
        &run(&["--home", bob.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let carol_did = did_of(
        &run(&["--home", carol.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

    // Every pair explicitly trusts the other's human+device KELs -- the
    // honest "trust-on-first-use, no witnesses yet" limitation this crate
    // documents (crate::identity's module docs).
    let alice_kel = run(&["--home", alice.to_str().unwrap(), "kel", "export"]);
    let bob_kel = run(&["--home", bob.to_str().unwrap(), "kel", "export"]);
    let carol_kel = run(&["--home", carol.to_str().unwrap(), "kel", "export"]);
    for (home, other_kel) in [
        (&bob, &alice_kel),
        (&bob, &carol_kel),
        (&carol, &alice_kel),
        (&carol, &bob_kel),
        (&alice, &bob_kel),
        (&alice, &carol_kel),
    ] {
        run(&["--home", home.to_str().unwrap(), "kel", "trust", other_kel]);
    }

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        "demo",
        "--maintainer",
        &alice_did,
        "--maintainer",
        &bob_did,
        "--maintainer",
        &carol_did,
        "--min-approvals",
        "2",
    ]);
    let project_id = fs::read_to_string(alice.join("projects").join("demo")).unwrap();

    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "track",
        "demo",
        &project_id,
    ]);
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "track",
        "demo",
        &project_id,
    ]);

    let src_dir = tempdir("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("lib.rs");
    fs::write(&file, "pub fn hello() {}").unwrap();

    let commit_out = run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "commit",
        "demo",
        "--branch",
        "main",
        "--message",
        "add hello",
        file.to_str().unwrap(),
    ]);
    let commit_id = last_word(&commit_out);

    let pr_out = run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "propose",
        "demo",
        "--branch",
        "main",
        "--title",
        "add hello",
        "--head",
        &commit_id,
    ]);
    let pr_id = last_word(&pr_out);

    // Outside contributor's own PR is not yet mergeable with zero reviews.
    let status_before = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "status",
        "demo",
    ]);
    assert!(status_before.contains("0 entries applied"));

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id,
        "--findings",
        "looks fine",
    ]);

    // One approval alone must not be enough for a 2-of-3 policy.
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "merge",
        "demo",
        &pr_id,
    ]);
    let status_one_approval = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "status",
        "demo",
    ]);
    assert!(
        status_one_approval.contains("0 entries applied"),
        "a single approval must not satisfy a 2-of-3 policy: {status_one_approval}"
    );

    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id,
    ]);
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "merge",
        "demo",
        &pr_id,
    ]);

    // The merge is visible from a THIRD, fully independent home (bob's) --
    // this is the actual claim: canonical state reached with no shared
    // process, no daemon, no network stack, just the shared store path.
    let status_after = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "status",
        "demo",
    ]);
    assert!(status_after.contains("1 entries applied"));
    assert!(status_after.contains(&format!("main -> {commit_id}")));

    let findings = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "findings",
        &pr_id,
    ]);
    assert!(findings.contains("looks fine"));

    // The reviewed commit is checkable out from any home, byte-identical.
    let dest = tempdir("checkout");
    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "checkout",
        &commit_id,
        dest.to_str().unwrap(),
    ]);
    assert_eq!(
        fs::read_to_string(dest.join("lib.rs")).unwrap(),
        "pub fn hello() {}"
    );
}

#[test]
fn an_untrusted_authors_project_cannot_be_resolved() {
    // Without the KEL-trust step, a project genesis authored by an
    // identity this home has never vouched for must not resolve --
    // exactly the failure this crate's own smoke-testing caught: skipping
    // `kel trust` is not silently ignored, it is a hard refusal.
    let store = tempdir("store2");
    let alice = tempdir("alice2");
    let bob = tempdir("bob2");
    let store_flag = store.to_str().unwrap().to_string();

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "identity",
        "init",
    ]);
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "identity",
        "init",
    ]);
    let alice_did = did_of(
        &run(&["--home", alice.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        "demo",
        "--maintainer",
        &alice_did,
    ]);
    let project_id = fs::read_to_string(alice.join("projects").join("demo")).unwrap();
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "track",
        "demo",
        &project_id,
    ]);

    // Bob never trusted alice's KEL.
    let err = run_err(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "status",
        "demo",
    ]);
    assert!(matches!(err, mini_cli::CliError::Forge(_)));
}

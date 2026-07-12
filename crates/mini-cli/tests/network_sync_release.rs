//! Batch 5 (#114): the developer-change -> review -> governed-merge ->
//! release -> install pipeline `self_hosted_spine_e2e.rs` already proves
//! over a *shared* `--store` path, reached again with **no shared
//! filesystem at all** -- purely over a real TCP `mini sync` connection,
//! extending `network_sync.rs`'s existing governed-merge-over-the-wire
//! claim through release/attestation/install.
//!
//! `mini_sync::sync_bidirectional` replicates the *entire* object store
//! (`store.all_ids()`), type-agnostic -- proposal, review, merge, release,
//! and attestation objects are all just signed objects in the same
//! content-addressed store, so no new wire protocol or sync-side code is
//! needed for this (the same composition insight D-0062 already proved
//! for KELs and commits). This file exists to actually demonstrate that,
//! not to add new replication logic.

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use mini_forge::ADOPTION_MIN_TIMELOCK_MS;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-netsync-release-{tag}-{}-{}",
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

fn run_with_retry(args: &[&str]) -> String {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    for attempt in 0..50 {
        match mini_cli::run(&owned) {
            Ok(out) => return out,
            Err(e) if attempt < 49 => {
                thread::sleep(Duration::from_millis(20));
                let _ = e;
            }
            Err(e) => panic!("command {args:?} failed after retries: {e}"),
        }
    }
    unreachable!()
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

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn real_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn past_timelock_now_ms() -> u64 {
    real_now_ms() + ADOPTION_MIN_TIMELOCK_MS + 60_000
}

fn free_loopback_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr.to_string()
}

#[test]
fn a_release_reaches_install_on_a_peer_that_never_shared_a_filesystem() {
    // Alice, Carol, and Dave do every authoring/review/attestation step
    // in Alice's *local* store -- exactly `network_sync.rs`'s existing
    // pattern (a realistic "same room" working group). Bob is the one
    // peer with a completely independent store that has never touched
    // Alice's filesystem; everything Bob ends up with arrives only over
    // a real TCP `mini sync` connection.
    let alice_home = tempdir("alice-home");
    let alice_store = tempdir("alice-store");
    let carol_home = tempdir("carol-home");
    let dave_home = tempdir("dave-home");
    let bob_home = tempdir("bob-home");
    let bob_store = tempdir("bob-store");

    for home in [&alice_home, &carol_home, &dave_home, &bob_home] {
        run(&["--home", home.to_str().unwrap(), "identity", "init"]);
    }
    let alice_did = did_of(
        &run(&["--home", alice_home.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let carol_did = did_of(
        &run(&["--home", carol_home.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    // Dave never needs to be named as a maintainer -- attesting doesn't
    // require it, only being a distinct verified identity root does.

    // Alice, Carol, and Dave mutually trust each other (needed for local
    // review/attestation, same pairwise exchange every other multi-
    // identity test in this crate uses). Bob only needs to trust the
    // three of them, before he ever connects -- sync's trust boundary is
    // a separate out-of-band step -- since he never authors anything
    // they need to verify.
    let alice_kel = run(&["--home", alice_home.to_str().unwrap(), "kel", "export"]);
    let carol_kel = run(&["--home", carol_home.to_str().unwrap(), "kel", "export"]);
    let dave_kel = run(&["--home", dave_home.to_str().unwrap(), "kel", "export"]);
    for (home, other_kel) in [
        (&carol_home, &alice_kel),
        (&dave_home, &alice_kel),
        (&alice_home, &carol_kel),
        (&dave_home, &carol_kel),
        (&alice_home, &dave_kel),
        (&carol_home, &dave_kel),
    ] {
        run(&["--home", home.to_str().unwrap(), "kel", "trust", other_kel]);
    }
    for kel in [&alice_kel, &carol_kel, &dave_kel] {
        run(&["--home", bob_home.to_str().unwrap(), "kel", "trust", kel]);
    }

    // ---- governance, entirely local to Alice's store ------------------
    run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "repo",
        "init",
        "spine-over-sync",
        "--maintainer",
        &alice_did,
        "--maintainer",
        &carol_did,
        "--min-approvals",
        "1",
    ]);
    let project_id =
        fs::read_to_string(alice_home.join("projects").join("spine-over-sync")).unwrap();

    let src_dir = tempdir("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("lib.rs");
    fs::write(&file, "pub fn hello() {}").unwrap();
    let commit_out = run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "repo",
        "commit",
        "spine-over-sync",
        "--branch",
        "main",
        "--message",
        "add hello",
        file.to_str().unwrap(),
    ]);
    let commit_id = last_word(&commit_out);

    let pr_out = run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "pr",
        "propose",
        "spine-over-sync",
        "--branch",
        "main",
        "--title",
        "add hello",
        "--head",
        &commit_id,
    ]);
    let pr_id = last_word(&pr_out);
    run(&[
        "--home",
        carol_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id,
    ]);
    run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "pr",
        "merge",
        "spine-over-sync",
        &pr_id,
    ]);

    // ---- release + two independent attestations, still local ----------
    let artifact_dir = tempdir("artifact");
    fs::create_dir_all(&artifact_dir).unwrap();
    let artifact_path = artifact_dir.join("release.bin");
    let artifact_bytes = b"release replicated purely over the wire".to_vec();
    fs::write(&artifact_path, &artifact_bytes).unwrap();
    let artifact_digest = hex(&blake3::hash(&artifact_bytes).into());
    let recipe_digest = hex(&blake3::hash(b"recipe").into());

    let create_out = run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "release",
        "create",
        "spine-over-sync",
        "--branch",
        "main",
        "--version",
        "1.0.0",
        "--commit",
        &commit_id,
        "--artifact",
        artifact_path.to_str().unwrap(),
        "--recipe-digest",
        &recipe_digest,
    ]);
    let release_id = last_word(&create_out);

    for home in [&carol_home, &dave_home] {
        run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            alice_store.to_str().unwrap(),
            "release",
            "attest",
            &release_id,
            "--artifact-digest",
            &artifact_digest,
        ]);
    }

    // ---- Bob tracks the project (a public id, safe to announce before
    // his store has anything in it) and connects purely over TCP -------
    run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "repo",
        "track",
        "spine-over-sync",
        &project_id,
    ]);

    let addr = free_loopback_addr();
    let bob_home_str = bob_home.to_str().unwrap().to_string();
    let bob_store_str = bob_store.to_str().unwrap().to_string();
    let listen_addr = addr.clone();
    let server = thread::spawn(move || {
        mini_cli::run(&[
            "--home".to_string(),
            bob_home_str,
            "--store".to_string(),
            bob_store_str,
            "sync".to_string(),
            "listen".to_string(),
            "--addr".to_string(),
            listen_addr,
        ])
        .unwrap()
    });
    let sync_out = run_with_retry(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "sync",
        "connect",
        &addr,
    ]);
    let server_report = server.join().unwrap();
    assert!(sync_out.contains("accepted"), "client report: {sync_out}");
    assert!(
        server_report.contains("accepted"),
        "server: {server_report}"
    );

    // ---- everything from here reads and writes ONLY Bob's independent
    // store, populated by nothing but what arrived over the network ----
    let now_ms = past_timelock_now_ms().to_string();

    let verify_out = run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "release",
        "verify",
        &release_id,
        "spine-over-sync",
        "--branch",
        "main",
        "--now-ms",
        &now_ms,
    ]);
    assert!(
        verify_out.contains("2 independent attester(s)"),
        "release verify must succeed from purely-synced data: {verify_out}"
    );

    let device_root = tempdir("bob-device");
    let device_root_str = device_root.to_str().unwrap().to_string();

    let stage_out = run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "installer",
        "stage",
        "--device-root",
        &device_root_str,
        &release_id,
        "spine-over-sync",
        "--branch",
        "main",
        "--now-ms",
        &now_ms,
        "--timestamp-ms",
        &now_ms,
    ]);
    assert!(stage_out.contains("staged:"), "{stage_out}");

    run(&[
        "installer",
        "preflight",
        "--device-root",
        &device_root_str,
        &release_id,
        "--timestamp-ms",
        &now_ms,
    ]);
    let activate_out = run(&[
        "installer",
        "activate",
        "--device-root",
        &device_root_str,
        &release_id,
        "--approved-at-ms",
        &now_ms,
    ]);
    assert!(activate_out.contains("activated:"), "{activate_out}");

    let health_out = run(&[
        "installer",
        "health-check",
        "--device-root",
        &device_root_str,
        &release_id,
        "--healthy",
        "--timestamp-ms",
        &now_ms,
    ]);
    assert!(
        health_out.contains("stays active"),
        "the release replicated purely over TCP must genuinely install: {health_out}"
    );

    let status_out = run(&["installer", "status", "--device-root", &device_root_str]);
    assert!(status_out.contains(&release_id), "{status_out}");
}

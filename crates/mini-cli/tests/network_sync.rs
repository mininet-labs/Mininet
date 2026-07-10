//! Batch 5's first concrete demonstration
//! (`docs/design/self-hosted-forge-spine.md`): two `mini` homes with
//! completely independent, *unshared* stores reach the same governed
//! state purely over a real TCP `mini sync` connection. Contrast
//! `tests/two_developers.rs`, which proves the same governed-merge claim
//! over a shared `--store` filesystem path (Batch 1's exit condition) --
//! this test proves it again with no shared filesystem at all, only the
//! network, reusing `mini_bearer`/`mini_sync` exactly as `mini-bootstrap`'s
//! live demo already proved (D-0062).

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-netsync-{tag}-{}-{}",
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

/// Retries the connect side against a listener that may not have bound yet
/// -- avoids a fixed sleep race between spawning the server thread and the
/// client dialing it.
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

/// Bind an ephemeral loopback port, read its address, then release it --
/// the server thread rebinds the same port a moment later. A real race
/// exists in principle; acceptable for this sandboxed test (the client
/// side retries its connect regardless, see `run_with_retry`).
fn free_loopback_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr.to_string()
}

#[test]
fn two_homes_with_independent_stores_converge_over_a_real_tcp_sync() {
    let alice_home = tempdir("alice-home");
    let alice_store = tempdir("alice-store");
    let bob_home = tempdir("bob-home");
    let bob_store = tempdir("bob-store");

    let carol_home = tempdir("carol-home");

    run(&["--home", alice_home.to_str().unwrap(), "identity", "init"]);
    run(&["--home", bob_home.to_str().unwrap(), "identity", "init"]);
    run(&["--home", carol_home.to_str().unwrap(), "identity", "init"]);

    let alice_did = did_of(
        &run(&["--home", alice_home.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let carol_did = did_of(
        &run(&["--home", carol_home.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

    // Out-of-band KEL trust exchange -- unchanged from the shared-store
    // flow; sync's trust boundary is a separate step from object
    // transport, and this crate has no witness/discovery layer yet.
    // Carol collaborates with Alice over the same local store (a realistic
    // "same room" pairing); Bob is the one who is remote and must reach
    // the same state purely over `mini sync`.
    let alice_kel = run(&["--home", alice_home.to_str().unwrap(), "kel", "export"]);
    let bob_kel = run(&["--home", bob_home.to_str().unwrap(), "kel", "export"]);
    let carol_kel = run(&["--home", carol_home.to_str().unwrap(), "kel", "export"]);
    run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "kel",
        "trust",
        &alice_kel,
    ]);
    run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "kel",
        "trust",
        &bob_kel,
    ]);
    run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "kel",
        "trust",
        &carol_kel,
    ]);
    run(&[
        "--home",
        carol_home.to_str().unwrap(),
        "kel",
        "trust",
        &alice_kel,
    ]);
    run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "kel",
        "trust",
        &carol_kel,
    ]);

    // Alice and Carol do all the governance work inside Alice's local
    // store -- `mini-forge` correctly excludes a PR's own author from its
    // approval quorum (D-0067's independent-review floor), so a second,
    // distinct identity is required to actually reach a governed merge --
    // Bob's store does not exist yet at all.
    run(&[
        "--home",
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
        "repo",
        "init",
        "demo",
        "--maintainer",
        &alice_did,
        "--maintainer",
        &carol_did,
        "--min-approvals",
        "1",
    ]);
    let project_id = fs::read_to_string(alice_home.join("projects").join("demo")).unwrap();

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
        alice_home.to_str().unwrap(),
        "--store",
        alice_store.to_str().unwrap(),
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
        "demo",
        &pr_id,
    ]);

    // Bob tracks the same project id, announced out of band (the id itself
    // is not a secret) -- `track` is a local alias only, it never touches
    // the store, so this is safe before Bob's store has anything in it.
    run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "repo",
        "track",
        "demo",
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
        "server report: {server_report}"
    );

    // Bob's completely independent store now resolves the exact same
    // governed merge -- reached with no shared filesystem, only the
    // network. This is the actual claim.
    let status = run(&[
        "--home",
        bob_home.to_str().unwrap(),
        "--store",
        bob_store.to_str().unwrap(),
        "repo",
        "status",
        "demo",
    ]);
    assert!(status.contains("1 entries applied"), "status: {status}");
    assert!(status.contains(&format!("main -> {commit_id}")));
}

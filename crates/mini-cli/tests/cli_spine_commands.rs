//! Proves the `mini build`/`release`/`provenance`/`installer` CLI
//! subcommands added for #111 actually work end to end, driven the way a
//! real developer would type them -- through [`mini_cli::run`], never a
//! direct `mini_forge`/`mini_installer`/`mini_provenance` library call.
//!
//! `self_hosted_spine_e2e.rs` already proves the full developer-change ->
//! governed-merge -> sandboxed-build -> release -> install -> rollback
//! pipeline composes correctly at the *library* level; its own module docs
//! say plainly that build/release/install had no CLI subcommand yet. This
//! file is the follow-up that closes exactly that gap for the commands
//! this batch adds. It does not restate that harness's adversarial
//! rollback scenario -- `installer.rs`'s own logic and event-log grammar
//! are already covered by `mini-installer`'s adversarial test suite; this
//! file only needs to prove the CLI plumbing calls the right library
//! functions with the right arguments and gets the right real-world
//! result back.
//!
//! This file predates `--json` output (#112, `cli_json_output.rs`) and
//! keeps using `last_word`/`did_of` text scraping to thread values between
//! commands, the same approach `two_developers.rs` and
//! `self_hosted_spine_e2e.rs` already use -- left as-is rather than
//! rewritten, since it was already passing and `--json` coverage lives in
//! its own dedicated test file.

mod common;

use std::fs;
use std::path::PathBuf;

use mini_forge::ADOPTION_MIN_TIMELOCK_MS;
use mini_pipeline_protocol::ExitStatus;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-spine-cmds-{tag}-{}-{}",
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

/// A `--now-ms`/`--timestamp-ms` far enough past a release recorded "just
/// now" (real wall-clock time, since `mini release create` stamps releases
/// with the CLI's own real clock, unlike `self_hosted_spine_e2e.rs`'s
/// simulated timeline of direct library calls) to clear the frozen
/// adoption timelock floor.
fn past_timelock_now_ms() -> u64 {
    real_now_ms() + ADOPTION_MIN_TIMELOCK_MS + 60_000
}

/// `mini build run`'s production binary locator (`crate::build::
/// runner_binary_path`, distinct from `common::runner_binary_path`'s
/// test-only `cargo build`-and-resolve trick) looks next to `mini`'s own
/// executable first -- the expected real install layout, both binaries
/// side by side -- before falling back to bare `PATH` resolution. A test
/// binary is not `mini`, so this places a real copy of the compiled
/// runner next to *this test binary's* own executable, satisfying that
/// same lookup rather than mutating the process-wide `PATH` (which would
/// race against any other test spawning subprocesses in this same
/// `cargo test` process).
fn install_runner_next_to_this_test_binary() {
    let built = common::runner_binary_path();
    let sibling = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join(built.file_name().unwrap());
    if !sibling.exists() {
        fs::copy(&built, &sibling).unwrap();
    }
}

#[test]
fn build_run_drives_the_real_wasmtime_runner_subprocess() {
    install_runner_next_to_this_test_binary();
    let source = r#"fn main() { std::fs::write("/artifacts/output.txt", b"hello from mini build run").unwrap(); }"#;
    let component = common::compile_guest("mini_build_run_smoke", source);
    let component_path = tempdir("component");
    fs::create_dir_all(&component_path).unwrap();
    let component_file = component_path.join("guest.wasm");
    fs::write(&component_file, &component).unwrap();

    let store_dir = tempdir("build-store");
    let scratch_dir = tempdir("build-scratch");
    let artifacts_dir = tempdir("build-artifacts");

    let out = run(&[
        "build",
        "run",
        "--component",
        component_file.to_str().unwrap(),
        "--store-dir",
        store_dir.to_str().unwrap(),
        "--scratch-dir",
        scratch_dir.to_str().unwrap(),
        "--artifacts-dir",
        artifacts_dir.to_str().unwrap(),
        "--capability",
        "artifacts-write",
    ]);
    assert!(
        out.contains(&format!("exit_status: {:?}", ExitStatus::Success)),
        "sandboxed build via the CLI must report success: {out}"
    );

    let output = fs::read(artifacts_dir.join("output.txt"))
        .expect("mini build run must have written the real artifact to --artifacts-dir");
    assert_eq!(output, b"hello from mini build run");
    let expected_digest = hex(&blake3::hash(&output).into());
    assert!(
        out.contains(&format!("output_digest: {expected_digest}")),
        "the runner's own attested output digest must match the real artifact bytes: {out}"
    );
}

#[test]
fn release_provenance_and_installer_commands_drive_a_real_install() {
    // ---- three independent identities, mutually trusted ----------------
    let store = tempdir("store");
    let alice = tempdir("alice"); // release author
    let bob = tempdir("bob"); // reviewer + attester
    let carol = tempdir("carol"); // reviewer + attester + independent builder + device owner
    let store_flag = store.to_str().unwrap().to_string();

    for home in [&alice, &bob, &carol] {
        run(&["--home", home.to_str().unwrap(), "identity", "init"]);
    }
    let alice_did = did_of(
        &run(&["--home", alice.to_str().unwrap(), "identity", "show"]),
        "human:",
    );
    let bob_did = did_of(
        &run(&["--home", bob.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

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

    // ---- a real governed merge, exactly as a developer would type it ---
    // Two maintainers (min-approvals 1): a lone maintainer's own PR could
    // never merge, since quorum counting excludes the PR's own author by
    // identity (the same author-exclusion pattern release attestations
    // and provenance agreement use).
    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        "cli-spine-demo",
        "--maintainer",
        &alice_did,
        "--maintainer",
        &bob_did,
        "--min-approvals",
        "1",
    ]);
    let project_id = fs::read_to_string(alice.join("projects").join("cli-spine-demo")).unwrap();
    // Bob and Carol both need the project tracked locally: bob to approve
    // and to run `release verify`/`release list`, carol because `installer
    // stage` re-verifies governed release trust itself and needs to
    // resolve the same project alias.
    for home in [&bob, &carol] {
        run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            &store_flag,
            "repo",
            "track",
            "cli-spine-demo",
            &project_id,
        ]);
    }

    let src_dir = tempdir("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("lib.rs");
    fs::write(&file, "pub fn hello() -> &'static str { \"cli-spine\" }").unwrap();
    let commit_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "commit",
        "cli-spine-demo",
        "--branch",
        "main",
        "--message",
        "add hello",
        file.to_str().unwrap(),
    ]);
    let commit_id = last_word(&commit_out);
    let pr_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "propose",
        "cli-spine-demo",
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
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id,
        "--findings",
        "lgtm from bob",
    ]);
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "merge",
        "cli-spine-demo",
        &pr_id,
    ]);

    // ---- `mini release create` --------------------------------------
    let artifact_dir = tempdir("artifact");
    fs::create_dir_all(&artifact_dir).unwrap();
    let artifact_path = artifact_dir.join("release.bin");
    let artifact_bytes = b"a real release artifact, cli-driven".to_vec();
    fs::write(&artifact_path, &artifact_bytes).unwrap();
    let artifact_digest_hex = hex(&blake3::hash(&artifact_bytes).into());
    let recipe_digest_hex = hex(&blake3::hash(b"recipe: cargo build --release").into());

    let create_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "release",
        "create",
        "cli-spine-demo",
        "--branch",
        "main",
        "--version",
        "1.0.0",
        "--commit",
        &commit_id,
        "--artifact",
        artifact_path.to_str().unwrap(),
        "--recipe-digest",
        &recipe_digest_hex,
    ]);
    let release_id = last_word(&create_out);

    // ---- `mini release attest`, two independent identity roots -------
    for home in [&bob, &carol] {
        let out = run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            &store_flag,
            "release",
            "attest",
            &release_id,
            "--artifact-digest",
            &artifact_digest_hex,
        ]);
        assert!(out.contains("attestation recorded"), "{out}");
    }

    let now_ms = past_timelock_now_ms().to_string();

    // ---- `mini release verify` ----------------------------------------
    let verify_out = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "release",
        "verify",
        &release_id,
        "cli-spine-demo",
        "--branch",
        "main",
        "--now-ms",
        &now_ms,
    ]);
    assert!(
        verify_out.contains("2 independent attester(s)"),
        "{verify_out}"
    );

    // ---- `mini release list` -------------------------------------------
    let list_out = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "release",
        "list",
        "cli-spine-demo",
        "--branch",
        "main",
    ]);
    assert!(list_out.contains(&release_id), "{list_out}");

    // ---- `mini provenance record` + `mini provenance verify` ----------
    // Carol independently claims she rebuilt the exact same artifact
    // digest -- one distinct identity root beyond the release author.
    let env_digest_hex = hex(&blake3::hash(b"linux-x86_64-rustc1.83").into());
    let commands_digest_hex = hex(&blake3::hash(b"cargo build --release").into());
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "provenance",
        "record",
        &release_id,
        "--environment-digest",
        &env_digest_hex,
        "--commands-digest",
        &commands_digest_hex,
        "--output",
        &artifact_digest_hex,
        "--group",
        "linux-x86_64-rustc1.83",
        "--started-ms",
        "1000",
        "--finished-ms",
        "2000",
    ]);
    let provenance_verify_out = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "provenance",
        "verify",
        &release_id,
        "--output",
        &artifact_digest_hex,
        "--min-agreement",
        "1",
    ]);
    assert!(
        provenance_verify_out.contains("1 independent identity root(s) agree"),
        "{provenance_verify_out}"
    );

    // ---- `mini installer stage/preflight/activate/health-check` -------
    // Carol is the device owner running this install.
    let device_root = tempdir("device");
    let device_root_str = device_root.to_str().unwrap().to_string();

    let stage_out = run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "installer",
        "stage",
        "--device-root",
        &device_root_str,
        &release_id,
        "cli-spine-demo",
        "--branch",
        "main",
        "--now-ms",
        &now_ms,
        "--timestamp-ms",
        &now_ms,
    ]);
    assert!(stage_out.contains("staged:"), "{stage_out}");

    let preflight_out = run(&[
        "installer",
        "preflight",
        "--device-root",
        &device_root_str,
        &release_id,
        "--timestamp-ms",
        &now_ms,
    ]);
    assert!(
        preflight_out.contains("awaiting owner approval"),
        "{preflight_out}"
    );

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

    let status_out = run(&["installer", "status", "--device-root", &device_root_str]);
    assert!(status_out.contains(&release_id), "{status_out}");

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
        "a passing health check must not roll anything back: {health_out}"
    );

    // ---- `mini installer verify-log` + `history` -----------------------
    let verify_log_out = run(&["installer", "verify-log", "--device-root", &device_root_str]);
    assert!(
        verify_log_out.contains("verified clean"),
        "{verify_log_out}"
    );

    let history_out = run(&[
        "installer",
        "history",
        "--device-root",
        &device_root_str,
        "--release",
        &release_id,
    ]);
    for expected in [
        "Discovered",
        "Verified",
        "Staged",
        "PreflightPassed",
        "AwaitingOwnerApproval",
        "OwnerApproved",
        "Activating",
        "HealthCheckStarted",
        "HealthCheckPassed",
    ] {
        assert!(
            history_out.contains(expected),
            "history must show {expected}: {history_out}"
        );
    }

    // ---- `mini installer rollback` -------------------------------------
    // A standalone, owner-initiated rollback (not health-check-triggered)
    // -- there is nothing prior to restore to, so this must fail cleanly
    // rather than silently succeed.
    let owned: Vec<String> = [
        "installer",
        "rollback",
        "--device-root",
        &device_root_str,
        "--timestamp-ms",
        &now_ms,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let err = mini_cli::run(&owned).expect_err("rollback with nothing prior must fail cleanly");
    assert!(matches!(err, mini_cli::CliError::Installer(_)));
}

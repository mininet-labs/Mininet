//! Adversarial CLI fixtures for `release`/`installer` (#113): bad
//! behavior must fail safely, driven through the real text-based CLI
//! (`mini_cli::run`), not direct `mini_forge`/`mini_installer` calls.
//!
//! `mini-forge`'s and `mini-installer`'s own test suites already cover
//! most of this logic at the library level -- this file exists to prove
//! the *CLI plumbing* doesn't accidentally weaken any of it: that a
//! self-attestation, a duplicate attestation, a wrong-digest attestation,
//! an early verify, a wrong-branch verify, or an out-of-order installer
//! step all still fail through `mini release`/`mini installer` exactly as
//! they fail through the library, with no silent CLI-level bypass.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mini_forge::ADOPTION_MIN_TIMELOCK_MS;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-adversarial-{tag}-{}-{}",
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

/// A real governed project with a merged commit and a real release,
/// ready for adversarial attestation/verification/install fixtures.
/// Alice authors and is a maintainer; Bob is a maintainer and attester;
/// Carol is an attester only.
struct Setup {
    store_flag: String,
    alice: PathBuf,
    bob: PathBuf,
    carol: PathBuf,
    project: String,
    release_id: String,
    artifact_digest: String,
}

fn governed_release(tag: &str) -> Setup {
    let store = tempdir(&format!("{tag}-store"));
    let alice = tempdir(&format!("{tag}-alice"));
    let bob = tempdir(&format!("{tag}-bob"));
    let carol = tempdir(&format!("{tag}-carol"));
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

    let project = format!("adversarial-{tag}");
    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        &project,
        "--maintainer",
        &alice_did,
        "--maintainer",
        &bob_did,
        "--min-approvals",
        "1",
    ]);
    let project_id = fs::read_to_string(alice.join("projects").join(&project)).unwrap();
    for home in [&bob, &carol] {
        run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            &store_flag,
            "repo",
            "track",
            &project,
            &project_id,
        ]);
    }

    let src_dir = tempdir(&format!("{tag}-src"));
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("lib.rs");
    fs::write(&file, "pub fn hello() {}").unwrap();
    let commit_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "commit",
        &project,
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
        &project,
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
    ]);
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "merge",
        &project,
        &pr_id,
    ]);

    let artifact_dir = tempdir(&format!("{tag}-artifact"));
    fs::create_dir_all(&artifact_dir).unwrap();
    let artifact_path = artifact_dir.join("release.bin");
    let artifact_bytes = format!("adversarial release artifact {tag}").into_bytes();
    fs::write(&artifact_path, &artifact_bytes).unwrap();
    let artifact_digest = hex(&blake3::hash(&artifact_bytes).into());
    let recipe_digest = hex(&blake3::hash(b"recipe").into());

    let create_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "release",
        "create",
        &project,
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

    Setup {
        store_flag,
        alice,
        bob,
        carol,
        project,
        release_id,
        artifact_digest,
    }
}

fn attest(s: &Setup, home: &Path, digest: &str) {
    run(&[
        "--home",
        home.to_str().unwrap(),
        "--store",
        &s.store_flag,
        "release",
        "attest",
        &s.release_id,
        "--artifact-digest",
        digest,
    ]);
}

fn verify_err(s: &Setup, home: &Path, branch: &str, now_ms: u64) -> mini_cli::CliError {
    run_err(&[
        "--home",
        home.to_str().unwrap(),
        "--store",
        &s.store_flag,
        "release",
        "verify",
        &s.release_id,
        &s.project,
        "--branch",
        branch,
        "--now-ms",
        &now_ms.to_string(),
    ])
}

#[test]
fn release_verify_fails_with_only_one_real_attester() {
    let s = governed_release("insufficient");
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let err = verify_err(&s, &s.bob.clone(), "main", past_timelock_now_ms());
    assert!(matches!(err, mini_cli::CliError::Forge(_)));
    assert!(
        err.to_string()
            .contains("need 2 independent attestations, got 1"),
        "{err}"
    );
}

#[test]
fn self_attestation_never_counts_toward_quorum() {
    let s = governed_release("self-attest");
    // Alice, the release's own author, attests her own release --
    // must not count toward the quorum at all.
    attest(&s, &s.alice.clone(), &s.artifact_digest.clone());
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let err = verify_err(&s, &s.bob.clone(), "main", past_timelock_now_ms());
    assert!(
        err.to_string()
            .contains("need 2 independent attestations, got 1"),
        "author self-attestation must not count: {err}"
    );
}

#[test]
fn duplicate_attestations_from_the_same_identity_only_count_once() {
    let s = governed_release("dup-attest");
    // Bob attests twice; no other real attester exists. Two calls from
    // one identity must still only count as one toward the quorum.
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    let err = verify_err(&s, &s.carol.clone(), "main", past_timelock_now_ms());
    assert!(
        err.to_string()
            .contains("need 2 independent attestations, got 1"),
        "a repeated attestation from the same identity must not double-count: {err}"
    );
}

#[test]
fn an_attestation_with_the_wrong_digest_does_not_count() {
    let s = governed_release("wrong-digest");
    let wrong_digest = "00".repeat(32);
    attest(&s, &s.bob.clone(), &wrong_digest);
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let err = verify_err(&s, &s.alice.clone(), "main", past_timelock_now_ms());
    assert!(
        err.to_string()
            .contains("need 2 independent attestations, got 1"),
        "an attestation claiming the wrong digest must not count: {err}"
    );
}

#[test]
fn release_verify_rejects_before_the_timelock_elapses() {
    let s = governed_release("early-verify");
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    // Real attestations, real quorum -- but --now-ms is still inside the
    // inspection window.
    let err = verify_err(&s, &s.alice.clone(), "main", real_now_ms());
    assert!(matches!(err, mini_cli::CliError::Forge(_)));
    assert!(
        err.to_string().contains("timelock has not elapsed"),
        "{err}"
    );
}

#[test]
fn release_verify_rejects_a_branch_the_release_did_not_claim() {
    let s = governed_release("wrong-branch");
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let err = verify_err(&s, &s.alice.clone(), "not-main", past_timelock_now_ms());
    assert!(matches!(err, mini_cli::CliError::Forge(_)));
    assert!(
        err.to_string().contains("not the canonical governed head")
            || err.to_string().contains("malformed forge object"),
        "{err}"
    );
}

#[test]
fn release_verify_succeeds_only_once_every_real_condition_is_met() {
    // Sanity anchor: the same setup that fails in every test above must
    // genuinely succeed once attestations are real, distinct, digest-
    // matched, and the timelock has elapsed -- otherwise the failures
    // above would be meaningless (failing for the wrong reason).
    let s = governed_release("happy-path");
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let out = run(&[
        "--home",
        s.bob.to_str().unwrap(),
        "--store",
        &s.store_flag,
        "release",
        "verify",
        &s.release_id,
        &s.project,
        "--branch",
        "main",
        "--now-ms",
        &past_timelock_now_ms().to_string(),
    ]);
    assert!(out.contains("2 independent attester(s)"), "{out}");
}

#[test]
fn installer_activate_before_preflight_fails_cleanly() {
    let s = governed_release("activate-too-early");
    attest(&s, &s.bob.clone(), &s.artifact_digest.clone());
    attest(&s, &s.carol.clone(), &s.artifact_digest.clone());
    let now_ms = past_timelock_now_ms().to_string();
    let device_root = tempdir("activate-too-early-device");
    let device_root_str = device_root.to_str().unwrap().to_string();

    run(&[
        "--home",
        s.carol.to_str().unwrap(),
        "--store",
        &s.store_flag,
        "installer",
        "stage",
        "--device-root",
        &device_root_str,
        &s.release_id,
        &s.project,
        "--branch",
        "main",
        "--now-ms",
        &now_ms,
        "--timestamp-ms",
        &now_ms,
    ]);

    // Skip `installer preflight` entirely and try to activate directly.
    let err = run_err(&[
        "installer",
        "activate",
        "--device-root",
        &device_root_str,
        &s.release_id,
        "--approved-at-ms",
        &now_ms,
    ]);
    assert!(matches!(err, mini_cli::CliError::Installer(_)));
    assert!(err.to_string().contains("AwaitingOwnerApproval"), "{err}");
}

#[test]
fn installer_preflight_on_a_release_that_was_never_staged_fails_cleanly() {
    let s = governed_release("preflight-never-staged");
    let device_root = tempdir("preflight-never-staged-device");
    let installer_err = run_err(&[
        "installer",
        "preflight",
        "--device-root",
        device_root.to_str().unwrap(),
        &s.release_id,
        "--timestamp-ms",
        "1000",
    ]);
    assert!(matches!(installer_err, mini_cli::CliError::Installer(_)));
    assert!(
        installer_err.to_string().contains("no recorded events"),
        "{installer_err}"
    );
}

#[test]
fn installer_health_check_requires_exactly_one_of_healthy_or_unhealthy() {
    let s = governed_release("health-check-flags");
    let device_root = tempdir("health-check-flags-device");

    let neither = run_err(&[
        "installer",
        "health-check",
        "--device-root",
        device_root.to_str().unwrap(),
        &s.release_id,
        "--timestamp-ms",
        "1000",
    ]);
    assert!(matches!(neither, mini_cli::CliError::Usage(_)));

    let both = run_err(&[
        "installer",
        "health-check",
        "--device-root",
        device_root.to_str().unwrap(),
        &s.release_id,
        "--healthy",
        "--unhealthy",
        "--timestamp-ms",
        "1000",
    ]);
    assert!(matches!(both, mini_cli::CliError::Usage(_)));
}

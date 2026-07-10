//! The self-hosted forge spine, proven as one system (#102's stated exit
//! condition): developer change -> review -> governed merge -> reproducible
//! sandboxed build -> build provenance -> governed release -> owner-approved
//! install -> health check -> and, when the deployed release turns out
//! broken, automatic rollback -- three independent identities, real signed
//! objects, a real Wasmtime-isolated subprocess build, real files on real
//! disk. No GitHub, no daemon, no mocked library calls.
//!
//! ## Where this drives the real `mini` binary's logic vs. calls libraries directly
//!
//! `mini-cli` today wires `identity`/`kel`/`repo`/`pr`/`sync` (see
//! `cli.rs`) -- everything through the governed merge goes through
//! [`mini_cli::run`], exactly as a developer would type it. There is
//! **no CLI subcommand yet** for build/provenance/release/install, so
//! everything from "build release in sandbox" onward calls `mini_forge`,
//! `mini_media`, `mini_provenance`, and `mini_installer` directly in-process
//! against the *same* on-disk `--store` the CLI calls wrote to, and drives
//! `mini-build-runner-wasmtime` as a genuine subprocess (never linked
//! in-process, per that crate's own D-0069 boundary rule). **This is
//! exactly the gap this harness exists to expose**: the pieces compose
//! correctly at the library level today; CLI/`--json` wiring for
//! release/install is real, separate follow-up work, not yet done.
//!
//! ## What this does not (yet) prove
//!
//! - No persisted, queryable installer event log exists yet (`mini_installer`
//!   is a type-state pipeline: each function's return type *is* the proof
//!   a step happened, checked at compile time, but there's no `Vec<Event>`
//!   a caller can inspect after the fact). This test asserts on the real
//!   typed return values (`ActivationRecord`, `HealthCheckOutcome`) as the
//!   closest existing analogue -- adding a real event log is separate,
//!   necessary follow-up work.
//! - Runs against `FsBackend` on one machine with three home directories
//!   sharing one store path -- not yet a live, concurrent, multi-machine
//!   network (that's Batch 5).

mod common;

use std::fs;
use std::path::PathBuf;

use did_mini::Did;
use mini_forge::{
    attest, check_no_rollback, release, verify_governed_release, ReleasePolicy, Version,
    ADOPTION_MIN_ATTESTATIONS, ADOPTION_MIN_TIMELOCK_MS,
};
use mini_installer::{HealthCheckOutcome, Installer, OwnerApproval};
use mini_objects::ObjectId;
use mini_pipeline::{Capability, ResourceLimits};
use mini_provenance::{independent_agreement, record_provenance, BuildProvenance};

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-spine-e2e-{tag}-{}-{}",
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

fn tight_limits() -> ResourceLimits {
    ResourceLimits {
        max_fuel: 50_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        max_wall_clock_ms: 5_000,
        max_output_bytes: 16 * 1024 * 1024,
        max_stdout_bytes: 1024 * 1024,
        max_stderr_bytes: 1024 * 1024,
        max_open_files: 16,
    }
}

/// Loads `home`'s reconstructed identity and the trust-directory oracle
/// governance decisions against this store are checked with -- the same
/// plumbing `mini_cli::cli`'s dispatch uses internally for every command,
/// now `pub` specifically so this harness (and any future `mini release`/
/// `mini installer` CLI command) can reuse it instead of re-deriving it.
fn oracle_for(home: &std::path::Path) -> (mini_cli::identity::Identity, mini_forge::KelDirectory) {
    let identity = mini_cli::identity::load(home).unwrap();
    let oracle = mini_cli::store::build_oracle(home, &identity).unwrap();
    (identity, oracle)
}

/// Builds one tiny WASI Preview 2 component in the real Wasmtime-isolated
/// sandbox (a genuine child process, real `mini-pipeline-protocol` framing
/// over real stdin/stdout) and returns the artifact bytes it wrote plus the
/// runner's own attested `ExecutionResult`.
fn build_in_sandbox(
    guest_name: &str,
    output_marker: &str,
) -> (Vec<u8>, mini_pipeline_protocol::ExecutionResult) {
    let source = format!(
        r#"fn main() {{ std::fs::write("/artifacts/output.txt", b"{output_marker}").unwrap(); }}"#
    );
    let component = common::compile_guest(guest_name, &source);
    let run = common::run_in_sandbox(common::SandboxRequest {
        component,
        workspace: vec![],
        capabilities: vec![Capability::ArtifactsWrite],
        limits: tight_limits(),
    });
    assert_eq!(
        run.result.exit_status,
        mini_pipeline_protocol::ExitStatus::Success,
        "sandboxed build of {guest_name} did not succeed"
    );
    let output = fs::read(run.artifacts_dir.join("output.txt")).unwrap_or_else(|e| {
        panic!("sandbox for {guest_name} produced no /artifacts/output.txt: {e}")
    });
    let expected_digest: [u8; 32] = blake3::hash(&output).into();
    assert_eq!(
        run.result.output_digests,
        vec![expected_digest],
        "runner-reported output digest must match the actual artifact bytes"
    );
    (output, run.result)
}

#[test]
fn self_hosted_spine_survives_broken_release_and_rolls_back() {
    // ---- Phase 1: three independent identities -----------------------
    let store = tempdir("store");
    let alice = tempdir("alice");
    let bob = tempdir("bob");
    let carol = tempdir("carol");
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
    let carol_did = did_of(
        &run(&["--home", carol.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

    // ---- Phase 2: KEL export + trust verify, all pairs ----------------
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
        let out = run(&["--home", home.to_str().unwrap(), "kel", "trust", other_kel]);
        assert!(
            out.contains("now trusting"),
            "kel trust must confirm what it trusted: {out}"
        );
    }

    // ---- Phase 3: repo init (2-of-3), commit, propose, review, merge --
    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        "spine-demo",
        "--maintainer",
        &alice_did,
        "--maintainer",
        &bob_did,
        "--maintainer",
        &carol_did,
        "--min-approvals",
        "2",
    ]);
    let project_id_str = fs::read_to_string(alice.join("projects").join("spine-demo")).unwrap();
    let project_id = ObjectId::parse(project_id_str.trim()).unwrap();

    for home in [&bob, &carol] {
        run(&[
            "--home",
            home.to_str().unwrap(),
            "--store",
            &store_flag,
            "repo",
            "track",
            "spine-demo",
            &project_id_str,
        ]);
    }

    let src_dir = tempdir("src");
    fs::create_dir_all(&src_dir).unwrap();
    let file = src_dir.join("lib.rs");
    fs::write(&file, "pub fn hello() -> &'static str { \"spine\" }").unwrap();

    let commit_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "commit",
        "spine-demo",
        "--branch",
        "main",
        "--message",
        "add hello",
        file.to_str().unwrap(),
    ]);
    let commit_id_str = last_word(&commit_out);
    let commit_id = ObjectId::parse(&commit_id_str).unwrap();

    let pr_out = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "propose",
        "spine-demo",
        "--branch",
        "main",
        "--title",
        "add hello",
        "--head",
        &commit_id_str,
    ]);
    let pr_id = last_word(&pr_out);

    // Bob reviews the exact commit.
    run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id_str,
        "--findings",
        "lgtm from bob",
    ]);
    // Carol reviews the exact commit.
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "approve",
        &pr_id,
        "--head",
        &commit_id_str,
        "--findings",
        "lgtm from carol",
    ]);
    // Alice, the (excluded) author, cannot merge this into existence alone --
    // the 2-of-3 quorum is satisfied by Bob + Carol, neither of whom is the author.
    run(&[
        "--home",
        carol.to_str().unwrap(),
        "--store",
        &store_flag,
        "pr",
        "merge",
        "spine-demo",
        &pr_id,
    ]);

    let status = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "status",
        "spine-demo",
    ]);
    assert!(
        status.contains("1 entries applied"),
        "governed merge must be visible from a third, independent home: {status}"
    );
    assert!(status.contains(&format!("main -> {commit_id_str}")));

    // ---- Phase 4: trust verify -- the whole governance graph resolves --
    let (alice_identity, _alice_oracle) = oracle_for(&alice);
    let (_carol_identity, carol_oracle) = oracle_for(&carol);
    let project_state = mini_forge::resolve_project(
        &mini_cli::store::open_store(&store).unwrap(),
        &carol_oracle,
        &project_id,
    )
    .unwrap();
    assert!(
        !project_state.forks_detected,
        "no governance fork should exist on a single-branch merge"
    );
    assert_eq!(
        project_state.branches,
        vec![("main".to_string(), commit_id.clone())],
        "the resolved governance graph's canonical head must match the CLI-reported merge"
    );

    // ---- Phase 5: build release 1.0.0 in the real Wasmtime sandbox ----
    let (artifact_v1, exec_v1) = build_in_sandbox("spine_v1", "mini spine v1.0.0 build output");

    let mut store_handle = mini_cli::store::open_store(&store).unwrap();
    let manifest_v1 = mini_media::publish_media(
        &mut store_handle,
        &Did::parse(&alice_did).unwrap(),
        &alice_identity.device,
        "application/octet-stream",
        &artifact_v1,
        9000,
        9000,
    )
    .unwrap();
    let recipe_digest_v1: [u8; 32] = blake3::hash(b"spine_v1 build recipe").into();

    let release_v1 = release(
        &mut store_handle,
        &Did::parse(&alice_did).unwrap(),
        &alice_identity.device,
        "1.0.0",
        &project_id,
        "main",
        &commit_id,
        &manifest_v1.id,
        manifest_v1.digest,
        recipe_digest_v1,
        9100,
        9001,
    )
    .unwrap();
    let release_v1_id = release_v1.id().clone();

    // ---- Phase 6: record build provenance, then independent agreement --
    let provenance_v1 = BuildProvenance {
        environment_digest: blake3::hash(exec_v1.wasmtime_version.as_bytes()).into(),
        commands_digest: blake3::hash(b"rustc --target wasm32-wasip2 -O").into(),
        output_digests: exec_v1.output_digests.clone(),
        reproducibility_group: "spine-e2e".to_string(),
        network_enabled: false,
        started_ms: 9000,
        finished_ms: 9050,
    };
    record_provenance(
        &mut store_handle,
        &Did::parse(&alice_did).unwrap(),
        &alice_identity.device,
        &release_v1_id,
        &provenance_v1,
        9101,
        9002,
    )
    .unwrap();
    // Bob and Carol independently confirm they reproduced the same output --
    // Alice's own record (as the release author) never counts toward this.
    let (bob_identity, _bob_oracle) = oracle_for(&bob);
    record_provenance(
        &mut store_handle,
        &Did::parse(&bob_did).unwrap(),
        &bob_identity.device,
        &release_v1_id,
        &provenance_v1,
        9102,
        9001,
    )
    .unwrap();
    let (carol_identity, _) = oracle_for(&carol);
    record_provenance(
        &mut store_handle,
        &Did::parse(&carol_did).unwrap(),
        &carol_identity.device,
        &release_v1_id,
        &provenance_v1,
        9103,
        9001,
    )
    .unwrap();
    let agreement = independent_agreement(
        &store_handle,
        &carol_oracle,
        &release_v1_id,
        manifest_v1.digest,
    )
    .unwrap();
    assert_eq!(
        agreement, 2,
        "author's own provenance record must not count toward independent agreement"
    );

    // ---- Phase 7: attest (2 independent roots) + verify the release ---
    attest(
        &mut store_handle,
        &Did::parse(&bob_did).unwrap(),
        &bob_identity.device,
        &release_v1_id,
        manifest_v1.digest,
        9200,
        9002,
    )
    .unwrap();
    attest(
        &mut store_handle,
        &Did::parse(&carol_did).unwrap(),
        &carol_identity.device,
        &release_v1_id,
        manifest_v1.digest,
        9201,
        9002,
    )
    .unwrap();

    let policy_v1 = ReleasePolicy {
        min_attestations: ADOPTION_MIN_ATTESTATIONS,
        timelock_ms: ADOPTION_MIN_TIMELOCK_MS,
        now_ms: 9100 + ADOPTION_MIN_TIMELOCK_MS + 1_000,
    };
    let verified_v1 = verify_governed_release(
        &store_handle,
        &carol_oracle,
        &release_v1_id,
        &project_id,
        "main",
        &policy_v1,
    )
    .unwrap();
    assert_eq!(verified_v1.version, "1.0.0");
    assert_eq!(verified_v1.attesters, 2);

    // ---- Phase 8: release transparency + rollback protection ----------
    let v1_version = Version::parse("1.0.0").unwrap();
    check_no_rollback(None, &v1_version).expect("first release of a project is never a rollback");
    let releases = mini_forge::list_releases(&store_handle, &project_id, "main").unwrap();
    assert_eq!(
        releases.len(),
        1,
        "the transparency log is the object store itself -- one release so far"
    );
    let equivocations =
        mini_forge::detect_equivocation(&store_handle, &project_id, "main").unwrap();
    assert!(
        equivocations.is_empty(),
        "a single honest release must show no equivocation"
    );

    // ---- Phase 9: install into a temp device root, owner-approved -----
    let device_root = tempdir("device-root");
    let installer = Installer::new(&device_root).unwrap();
    assert!(installer.current().unwrap().is_none());

    let staged_v1 = installer.stage(&store_handle, &verified_v1).unwrap();
    let passed_v1 = installer.preflight(&staged_v1).unwrap();
    let activation_v1 = installer
        .activate(&passed_v1, &OwnerApproval::new(release_v1_id.clone(), 9300))
        .unwrap();
    assert_eq!(
        activation_v1.previous, None,
        "first-ever activation has nothing to roll back to"
    );
    let outcome_v1 = installer.health_check(activation_v1, || true).unwrap();
    assert_eq!(
        outcome_v1,
        HealthCheckOutcome::Active(release_v1_id.clone())
    );
    assert_eq!(installer.current().unwrap(), Some(release_v1_id.clone()));

    // ==================== THE BROKEN RELEASE PATH =======================
    // ---- Phase 10: build, release, attest, and verify 2.0.0 ------------
    // Every governance/build/attestation step below is genuinely honest --
    // v2.0.0 is a real, validly governed, validly attested release. Its
    // brokenness is deliberately confined to the *deployed software*
    // failing its post-activation health probe, exactly the failure mode
    // the installer's rollback exists to catch (a defect no amount of
    // governance or build-provenance checking can see in advance).
    let (artifact_v2, _exec_v2) = build_in_sandbox(
        "spine_v2",
        "mini spine v2.0.0 build output (broken at runtime)",
    );
    let manifest_v2 = mini_media::publish_media(
        &mut store_handle,
        &Did::parse(&alice_did).unwrap(),
        &alice_identity.device,
        "application/octet-stream",
        &artifact_v2,
        9400,
        9003,
    )
    .unwrap();
    let recipe_digest_v2: [u8; 32] = blake3::hash(b"spine_v2 build recipe").into();
    let release_v2 = release(
        &mut store_handle,
        &Did::parse(&alice_did).unwrap(),
        &alice_identity.device,
        "2.0.0",
        &project_id,
        "main",
        &commit_id,
        &manifest_v2.id,
        manifest_v2.digest,
        recipe_digest_v2,
        9500,
        9004,
    )
    .unwrap();
    let release_v2_id = release_v2.id().clone();

    attest(
        &mut store_handle,
        &Did::parse(&bob_did).unwrap(),
        &bob_identity.device,
        &release_v2_id,
        manifest_v2.digest,
        9600,
        9003,
    )
    .unwrap();
    attest(
        &mut store_handle,
        &Did::parse(&carol_did).unwrap(),
        &carol_identity.device,
        &release_v2_id,
        manifest_v2.digest,
        9601,
        9003,
    )
    .unwrap();

    let policy_v2 = ReleasePolicy {
        min_attestations: ADOPTION_MIN_ATTESTATIONS,
        timelock_ms: ADOPTION_MIN_TIMELOCK_MS,
        now_ms: 9500 + ADOPTION_MIN_TIMELOCK_MS + 1_000,
    };
    let verified_v2 = verify_governed_release(
        &store_handle,
        &carol_oracle,
        &release_v2_id,
        &project_id,
        "main",
        &policy_v2,
    )
    .unwrap();
    assert_eq!(verified_v2.version, "2.0.0");

    // Forge-level rollback protection: adopting v1 *after* v2 is already
    // running is exactly the attack `check_no_rollback` exists to reject --
    // distinct from the installer's own health-check-triggered rollback below.
    let v2_version = Version::parse("2.0.0").unwrap();
    check_no_rollback(Some(&v1_version), &v2_version)
        .expect("2.0.0 after 1.0.0 is forward progress");
    let rollback_attempt = check_no_rollback(Some(&v2_version), &v1_version);
    assert!(
        rollback_attempt.is_err(),
        "adopting 1.0.0 after 2.0.0 is already running must be rejected"
    );

    // ---- Phase 11: install the broken release -> health check fails ---
    let staged_v2 = installer.stage(&store_handle, &verified_v2).unwrap();
    let passed_v2 = installer.preflight(&staged_v2).unwrap();
    let activation_v2 = installer
        .activate(&passed_v2, &OwnerApproval::new(release_v2_id.clone(), 9700))
        .unwrap();
    assert_eq!(
        activation_v2.previous,
        Some(release_v1_id.clone()),
        "activation must record what it's replacing"
    );
    assert_eq!(
        installer.current().unwrap(),
        Some(release_v2_id.clone()),
        "v2 is live, pending its health check"
    );

    // ---- Phase 12: rollback, and the evidence trail that it happened --
    let outcome_v2 = installer.health_check(activation_v2, || false).unwrap();
    assert_eq!(
        outcome_v2,
        HealthCheckOutcome::RolledBack {
            failed: release_v2_id.clone(),
            restored: release_v1_id.clone()
        },
        "a failed health check must roll back to exactly the previous release, named explicitly"
    );
    assert_eq!(
        installer.current().unwrap(),
        Some(release_v1_id.clone()),
        "after rollback the device must be running the last-known-good release, not nothing and not the broken one"
    );

    // The observable evidence trail this test can assert on today (no
    // persisted event-log type exists yet -- see this file's module docs):
    // activation_v1.previous == None, activation_v2.previous == Some(v1),
    // and outcome_v2 naming both the failed and restored release ids by
    // value, not just a boolean. Together they reconstruct exactly the
    // sequence the founder directive asks this harness to prove:
    // stage -> preflight -> owner-approved activate -> health check ->
    // (fail) -> automatic rollback to the prior known-good release.

    for dir in [&store, &alice, &bob, &carol, &device_root, &src_dir] {
        fs::remove_dir_all(dir).ok();
    }
}

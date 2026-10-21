//! `--json` output (#112): the machine-readable envelope
//! (`{"ok":true,"kind":...,...}` / `{"ok":false,"kind":...,
//! "error_code":...,"message":...}`) that replaces `last_word`-style
//! text-scraping for chaining commands. Two kinds of coverage:
//!
//! - `mini_cli::run` in-process, for the success envelope and real field
//!   extraction chained into a following command (`release create`'s
//!   `release_id` field feeding `release attest` directly, no text
//!   parsing).
//! - the actual compiled `mini` binary as a real subprocess, for the
//!   error envelope, since that path lives in `main.rs`
//!   (`mini_cli::json_error_envelope`/`command_kind`), outside anything
//!   `mini_cli::run` itself returns.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-json-{tag}-{}-{}",
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

/// A minimal, hand-rolled extractor for one `"key":"value"` or
/// `"key":number` pair out of the single-line JSON this crate emits --
/// not a general JSON parser (this crate never needs to consume its own
/// JSON output for real, only a test needs to pull one field back out to
/// prove it round-trips as real structured data, not text).
fn json_field<'a>(doc: &'a str, key: &str) -> &'a str {
    let needle = format!("\"{key}\":");
    let start = doc
        .find(&needle)
        .unwrap_or_else(|| panic!("field {key:?} not found in {doc}"))
        + needle.len();
    let rest = &doc[start..];
    if let Some(after_quote) = rest.strip_prefix('"') {
        let end = after_quote.find('"').unwrap();
        &after_quote[..end]
    } else {
        let end = rest.find([',', '}']).unwrap_or(rest.len());
        rest[..end].trim()
    }
}

#[test]
fn release_create_json_output_chains_directly_into_attest_without_text_scraping() {
    let store = tempdir("store");
    let alice = tempdir("alice");
    let bob = tempdir("bob");
    let store_flag = store.to_str().unwrap().to_string();

    for home in [&alice, &bob] {
        run(&["--home", home.to_str().unwrap(), "identity", "init"]);
    }
    let alice_did = did_of(
        &run(&["--home", alice.to_str().unwrap(), "identity", "show"]),
        "human:",
    );

    let alice_kel = run(&["--home", alice.to_str().unwrap(), "kel", "export"]);
    let bob_kel = run(&["--home", bob.to_str().unwrap(), "kel", "export"]);
    run(&["--home", bob.to_str().unwrap(), "kel", "trust", &alice_kel]);
    run(&["--home", alice.to_str().unwrap(), "kel", "trust", &bob_kel]);

    run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "repo",
        "init",
        "json-demo",
        "--maintainer",
        &alice_did,
        "--min-approvals",
        "1",
    ]);

    let src_dir = tempdir("src");
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
        "json-demo",
        "--branch",
        "main",
        "--message",
        "add hello",
        file.to_str().unwrap(),
    ]);
    let commit_id = commit_out.split_whitespace().last().unwrap().to_string();

    let artifact_dir = tempdir("artifact");
    fs::create_dir_all(&artifact_dir).unwrap();
    let artifact_path = artifact_dir.join("release.bin");
    fs::write(&artifact_path, b"json output test artifact").unwrap();
    let recipe_digest = "00".repeat(32);

    // `release create` with --json returns a real single-line JSON
    // envelope, not the human sentence.
    let create_json = run(&[
        "--home",
        alice.to_str().unwrap(),
        "--store",
        &store_flag,
        "--json",
        "release",
        "create",
        "json-demo",
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
    assert!(create_json.starts_with('{') && create_json.ends_with('}'));
    assert_eq!(json_field(&create_json, "ok"), "true");
    assert_eq!(json_field(&create_json, "kind"), "release.create");
    assert_eq!(json_field(&create_json, "version"), "1.0.0");
    let release_id = json_field(&create_json, "release_id").to_string();
    let artifact_digest = json_field(&create_json, "artifact_digest").to_string();
    assert!(
        release_id.starts_with('z') && !release_id.is_empty(),
        "release_id must be a real base58btc object id: {create_json}"
    );
    assert_eq!(
        artifact_digest.len(),
        64,
        "artifact_digest must be a real 32-byte hex digest: {create_json}"
    );

    // The extracted release_id feeds `release attest` directly -- no
    // `last_word`/text scraping anywhere in this chain.
    let attest_json = run(&[
        "--home",
        bob.to_str().unwrap(),
        "--store",
        &store_flag,
        "--json",
        "release",
        "attest",
        &release_id,
        "--artifact-digest",
        &artifact_digest,
    ]);
    assert_eq!(json_field(&attest_json, "ok"), "true");
    assert_eq!(json_field(&attest_json, "kind"), "release.attest");
    assert_eq!(json_field(&attest_json, "release_id"), release_id);
}

#[test]
fn json_is_rejected_for_commands_that_do_not_support_it_yet() {
    let home = tempdir("no-json-support");
    let owned: Vec<String> = ["--home", home.to_str().unwrap(), "--json", "intake", "show"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let err = mini_cli::run(&owned).expect_err("intake has no --json support yet");
    assert!(matches!(err, mini_cli::CliError::Usage(_)));
}

fn mini_binary() -> &'static str {
    env!("CARGO_BIN_EXE_mini")
}

/// The error envelope only exists in `main.rs` (`mini_cli::run` itself
/// keeps returning a real `Err(CliError)` to every Rust caller, never a
/// JSON string on failure -- see `mini_cli::json_error_envelope`'s doc
/// comment for why) -- so this drives the actual compiled binary as a
/// real subprocess rather than calling `mini_cli::run` in-process.
#[test]
fn compiled_binary_prints_a_json_error_envelope_on_failure() {
    let home = tempdir("binary-error");
    let bad_output = Command::new(mini_binary())
        .args([
            "--home",
            home.to_str().unwrap(),
            "--json",
            "release",
            "verify",
            "not-a-real-release-id",
            "json-demo",
            "--branch",
            "main",
        ])
        .output()
        .expect("failed to spawn the real mini binary");

    assert!(
        !bad_output.status.success(),
        "a bad release id must fail the process"
    );
    let stdout = String::from_utf8_lossy(&bad_output.stdout);
    assert!(
        stdout.trim_end().starts_with('{') && stdout.trim_end().ends_with('}'),
        "stdout must be the JSON error envelope, not free text: {stdout:?}"
    );
    assert_eq!(json_field(&stdout, "ok"), "false");
    assert_eq!(json_field(&stdout, "kind"), "release.verify");
    assert!(!json_field(&stdout, "error_code").is_empty());
    assert!(!json_field(&stdout, "message").is_empty());
}

#[test]
fn kel_inspect_and_failed_trust_leave_no_partial_trust_state() {
    let temp = tempfile::tempdir().unwrap();
    let owner = temp.path().join("owner");
    let recipient = temp.path().join("recipient");
    let init = run(&[
        "--home",
        owner.to_str().unwrap(),
        "--json",
        "identity",
        "init",
    ]);
    let exported = run(&["--home", owner.to_str().unwrap(), "--json", "kel", "export"]);
    assert_eq!(
        json_field(&init, "human_did"),
        json_field(&exported, "human_did")
    );
    let hex = json_field(&exported, "hex");
    let inspected = run(&[
        "--home",
        recipient.to_str().unwrap(),
        "--json",
        "kel",
        "inspect",
        hex,
    ]);
    assert_eq!(
        json_field(&inspected, "bundle_id"),
        json_field(&exported, "bundle_id")
    );
    assert_eq!(json_field(&inspected, "trusted"), "false");
    assert!(!recipient.join("trusted_kels").exists());
    let wrong: Vec<String> = [
        "--home",
        recipient.to_str().unwrap(),
        "kel",
        "trust",
        hex,
        "--confirm",
        "wrong-id",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(mini_cli::run(&wrong).is_err());
    assert!(!recipient.join("trusted_kels").exists());
    let mut bytes: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    // Corrupt the device's final signature, leaving the human log valid and
    // both binary KEL containers structurally decodable. The old sequential
    // implementation wrote the human KEL before discovering this invalidity.
    *bytes.last_mut().unwrap() ^= 1;
    let human_len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    let device_start = 4 + human_len + 4;
    let human = did_mini::Kel::from_bytes(&bytes[4..4 + human_len]).unwrap();
    let device = did_mini::Kel::from_bytes(&bytes[device_start..]).unwrap();
    human.verify().unwrap();
    assert!(device.verify().is_err());
    let malformed = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert!(mini_cli::store::cmd_trust_kel(&recipient, &malformed).is_err());
    assert!(!recipient.join("trusted_kels").exists());
    // Invalid Unicode is a usage error, not a panic from slicing a UTF-8
    // character at an arbitrary byte boundary.
    let unicode: Vec<String> = [
        "--home",
        recipient.to_str().unwrap(),
        "kel",
        "inspect",
        "€a",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert!(mini_cli::run(&unicode).is_err());
    let confirmed = run(&[
        "--home",
        recipient.to_str().unwrap(),
        "--json",
        "kel",
        "trust",
        hex,
        "--confirm",
        json_field(&exported, "bundle_id"),
    ]);
    assert_eq!(json_field(&confirmed, "trusted"), "true");
    assert_eq!(
        fs::read_dir(recipient.join("trusted_kels"))
            .unwrap()
            .count(),
        2
    );
}

#[test]
fn trusted_kel_updates_reject_rollback_and_forks_without_replacing_history() {
    let temp = tempfile::tempdir().unwrap();
    let mut root = did_mini::Controller::incept_single_from_seeds(&[21; 32], &[22; 32]).unwrap();
    let initial = root.kel();
    root.interact(vec![[1; 32]]).unwrap();
    let extended = root.kel();
    mini_cli::store::trust_kel(temp.path(), extended.clone()).unwrap();
    let file = temp
        .path()
        .join("trusted_kels")
        .join(format!("{}.kel", extended.scid()));
    let expected = fs::read(&file).unwrap();
    assert!(mini_cli::store::trust_kel(temp.path(), initial).is_err());
    assert_eq!(fs::read(&file).unwrap(), expected);
    let mut fork = did_mini::Controller::incept_single_from_seeds(&[21; 32], &[22; 32]).unwrap();
    fork.interact(vec![[2; 32]]).unwrap();
    assert!(mini_cli::store::trust_kel(temp.path(), fork.kel()).is_err());
    assert_eq!(fs::read(&file).unwrap(), expected);
    root.interact(vec![[3; 32]]).unwrap();
    mini_cli::store::trust_kel(temp.path(), root.kel()).unwrap();
    assert_ne!(fs::read(file).unwrap(), expected);
}

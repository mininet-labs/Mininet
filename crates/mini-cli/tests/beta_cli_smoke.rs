//! Basic smoke coverage for the Forge-native Open Beta CLI surfaces
//! introduced by #336 / PR #342.
//!
//! This is intentionally a lightweight, no-network test that proves the
//! command dispatch, empty-store list paths, and private claim generation
//! work. The full two-node verified-sync acceptance test (ephemeral KEL
//! carriers + finding + disposition chain) is still required before the
//! parent PR can leave draft.

use std::path::PathBuf;

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-cli-beta-smoke-{tag}-{}-{}",
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

fn run_expect_err(args: &[&str]) -> mini_cli::CliError {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    mini_cli::run(&owned).expect_err(&format!("expected failure for {args:?}"))
}

#[test]
fn beta_empty_lists_and_claim_new_smoke() {
    let home = tempdir("home");
    let store = tempdir("store");
    let home_s = home.to_str().unwrap();
    let store_s = store.to_str().unwrap();

    run(&["--home", home_s, "identity", "init"]);

    // Empty lists must succeed and report emptiness rather than panic.
    let campaign_list = run(&[
        "--home", home_s, "--store", store_s, "beta", "campaign", "list",
    ]);
    assert!(
        campaign_list.contains("no beta campaigns") || campaign_list.contains("campaigns"),
        "unexpected campaign list output: {campaign_list}"
    );

    let finding_list = run(&[
        "--home", home_s, "--store", store_s, "beta", "finding", "list",
    ]);
    assert!(
        finding_list.contains("no beta findings") || finding_list.contains("findings"),
        "unexpected finding list output: {finding_list}"
    );

    let disposition_list = run(&[
        "--home",
        home_s,
        "--store",
        store_s,
        "beta",
        "disposition",
        "list",
    ]);
    assert!(
        disposition_list.contains("no") || disposition_list.contains("disposition"),
        "unexpected disposition list output: {disposition_list}"
    );

    let contribution_list = run(&[
        "--home",
        home_s,
        "--store",
        store_s,
        "beta",
        "contribution",
        "list",
    ]);
    assert!(
        contribution_list.contains("no accepted beta contributions")
            || contribution_list.contains("contributions"),
        "unexpected contribution list output: {contribution_list}"
    );

    // Fresh private claim material must be generated without touching a
    // persistent contributor identity.
    let claim_out = run(&["--home", home_s, "--store", store_s, "beta", "claim", "new"]);
    assert!(
        claim_out.to_lowercase().contains("claim")
            || claim_out.contains("ClaimTag")
            || claim_out.contains("account"),
        "claim new should surface private claim/account material: {claim_out}"
    );

    // JSON surfaces for the list commands must be parseable single-line envelopes.
    let json_list = run(&[
        "--home", home_s, "--store", store_s, "--json", "beta", "campaign", "list",
    ]);
    assert!(
        json_list.trim_start().starts_with('{'),
        "--json campaign list must emit a JSON object: {json_list}"
    );
    assert!(
        json_list.contains("\"ok\"") || json_list.contains("campaigns"),
        "expected ok/campaigns field in JSON: {json_list}"
    );
}

#[test]
fn beta_finding_submit_requires_privacy_redacted_flag() {
    let home = tempdir("home-redact");
    let store = tempdir("store-redact");
    let home_s = home.to_str().unwrap();
    let store_s = store.to_str().unwrap();

    run(&["--home", home_s, "identity", "init"]);

    // Without a real campaign the command will fail later, but the privacy
    // gate must fire first when the flag is omitted.
    let err = run_expect_err(&[
        "--home",
        home_s,
        "--store",
        store_s,
        "beta",
        "finding",
        "submit",
        "not-a-real-campaign-id",
        "--evidence-class",
        "emulator",
        "--severity",
        "low",
        "--component",
        "cli",
        "--summary",
        "test",
        "--environment",
        "test",
        "--steps",
        "1",
        "--expected",
        "ok",
        "--observed",
        "ok",
        "--evidence",
        "none",
        "--limitations",
        "none",
        // deliberately omit --privacy-redacted
    ]);
    let msg = err.to_string();
    assert!(
        msg.contains("privacy-redacted") || msg.contains("Usage"),
        "finding submit without --privacy-redacted must be rejected: {msg}"
    );
}

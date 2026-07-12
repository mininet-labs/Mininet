//! Runs the no-GitHub outage demo (D-0081, `tools/
//! no_github_outage_demo.sh`) as a real subprocess against the real
//! compiled `mini` binary, so it stays exercised by `cargo test
//! --workspace` and a broken demo script fails CI the same way any other
//! regression would -- not just something a human has to remember to run
//! by hand.
//!
//! The script itself is the actual deliverable (a narrated, runnable
//! artifact a developer or auditor can read and execute without touching
//! Rust); this test is a thin wrapper proving it still works, not a
//! restatement of its logic.

use std::process::Command;

#[test]
fn the_full_no_github_outage_demo_script_runs_clean() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("mini-cli is expected to live at <repo>/crates/mini-cli");
    let script = repo_root.join("tools").join("no_github_outage_demo.sh");
    assert!(script.is_file(), "expected the demo script at {script:?}");

    let mini_bin = env!("CARGO_BIN_EXE_mini");

    let output = Command::new("bash")
        .arg(&script)
        .arg(mini_bin)
        .output()
        .expect("failed to spawn the demo script");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "no-GitHub outage demo failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    for marker in [
        "governed merge reached, verified from a third independent identity",
        "2 independent attester(s)",
        "release 1.0.0 installed and healthy",
        "rolled back to",
        "device is back on the known-good 1.0.0 release",
        "event log verified clean",
        "No-GitHub outage demo complete",
    ] {
        assert!(
            stdout.contains(marker),
            "expected demo output to contain {marker:?}:\n{stdout}"
        );
    }
}

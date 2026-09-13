//! End-to-end tests that run the real `mininet-setup` executable.
//!
//! These drive the shipped binary as a user or a deployment script would ---
//! process boundary, exit codes, stdout, `--json` envelopes --- rather than
//! calling its functions in-process. That is the difference between "the
//! install logic is tested" and "the installer works".
//!
//! The Windows-only halves (`.lnk`, `HKCU`) are skipped by the binary itself
//! on other platforms, with a note in its output, which these tests assert.
//! Everything else --- package verification, file placement, read-back
//! verification, activation, upgrade, rollback, uninstall --- is the same
//! code on every platform and is exercised here in full.

use mini_windows_setup::container;
use mini_windows_setup::manifest::{ManifestHeader, PackageManifest, PackageShortcut};
use mini_windows_setup::PackageFile;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SETUP_EXE: &str = env!("CARGO_BIN_EXE_mininet-setup");

const DESKTOP_V1: &[u8] = b"#!/bin/sh\necho desktop-0.1.0\n";
const DESKTOP_V2: &[u8] = b"#!/bin/sh\necho desktop-0.2.0\n";
const CLI: &[u8] = b"#!/bin/sh\necho cli\n";
// Uninstall registration requires the package to carry its own setup
// executable (or an explicit `options.setup_exe`), matching what the real
// release scripts stage. This fixture registers uninstall by default, so it
// needs one too.
const SETUP: &[u8] = b"#!/bin/sh\necho setup\n";

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-setup-cli-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write_package(dir: &Path, version: &str, desktop: &[u8]) -> PathBuf {
    let manifest = PackageManifest::new(
        ManifestHeader {
            package: "mininet-windows-client",
            version,
            target: "x86_64-pc-windows-msvc",
            product: "Mininet",
            launch: "mininet-desktop.exe",
            built_at_ms: 1_757_635_200_000,
        },
        vec![
            PackageFile::describe("mininet-desktop.exe", desktop).unwrap(),
            PackageFile::describe("mini.exe", CLI).unwrap(),
            PackageFile::describe("mininet-setup.exe", SETUP).unwrap(),
        ],
        vec![PackageShortcut {
            target: "mininet-desktop.exe".to_string(),
            name: "Mininet".to_string(),
        }],
    )
    .unwrap();
    let bytes = container::write(&manifest, |path| {
        Ok(match path {
            "mininet-desktop.exe" => desktop.to_vec(),
            "mini.exe" => CLI.to_vec(),
            "mininet-setup.exe" => SETUP.to_vec(),
            other => panic!("unexpected {other}"),
        })
    })
    .unwrap();
    let path = dir.join(format!("client-{version}.mnpkg"));
    std::fs::write(&path, bytes).unwrap();
    path
}

struct Env {
    base: PathBuf,
    install_root: PathBuf,
    user_data: PathBuf,
}

impl Env {
    fn new(tag: &str) -> Self {
        let base = tempdir(tag);
        Self {
            install_root: base.join("Programs").join("Mininet"),
            user_data: base.join("UserData"),
            base,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut command = Command::new(SETUP_EXE);
        command
            .args(args)
            .arg("--install-root")
            .arg(&self.install_root)
            .arg("--user-data-root")
            .arg(&self.user_data);
        // A stray .mnpkg beside the test binary must never be picked up.
        command.env("MININET_SETUP_PAYLOAD", "");
        command.output().expect("running mininet-setup")
    }

    fn json(&self, args: &[&str]) -> String {
        let output = self.run(args);
        String::from_utf8(output.stdout).expect("stdout is UTF-8")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn field(line: &str, key: &str) -> String {
    // A deliberately tiny extractor: these tests assert on the envelope's
    // exact single-line shape, so a real JSON parser would only hide a
    // formatting regression this is meant to catch.
    let needle = format!("\"{key}\":");
    let start = line.find(&needle).unwrap_or_else(|| {
        panic!("no {key} in {line}");
    }) + needle.len();
    let rest = &line[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    rest[..end].trim_matches('"').to_string()
}

#[test]
fn help_and_version_work_without_a_package() {
    let env = Env::new("help");
    let help = env.run(&["--help"]);
    assert!(help.status.success());
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("mininet-setup"));
    assert!(text.contains("--silent"));
    assert!(text.contains("never contacts the network"));

    let version = env.json(&["--version", "--json"]);
    assert_eq!(field(&version, "ok"), "true");
    assert_eq!(field(&version, "kind"), "setup.version");
}

#[test]
fn an_unrecognized_flag_fails_with_usage_and_exit_code_two() {
    let env = Env::new("badflag");
    let output = env.run(&["--not-a-flag"]);
    assert_eq!(output.status.code(), Some(2));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(text.contains("unrecognized argument"));
    assert!(text.contains("USAGE"));
}

#[test]
fn a_usage_error_is_reported_as_json_when_json_was_requested() {
    let env = Env::new("badflag-json");
    let output = env.run(&["--not-a-flag", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let line = String::from_utf8_lossy(&output.stdout);
    assert_eq!(field(&line, "ok"), "false");
    assert_eq!(field(&line, "error_code"), "bad_arguments");
}

#[test]
fn status_on_a_clean_machine_says_nothing_is_installed() {
    let env = Env::new("status-clean");
    let line = env.json(&["--status", "--json"]);
    assert_eq!(field(&line, "ok"), "true");
    assert_eq!(field(&line, "active_version"), "null");
    assert_eq!(field(&line, "installed_versions"), "[]");
}

#[test]
fn a_dry_run_reports_the_plan_and_writes_nothing() {
    let env = Env::new("dry-run");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let line = env.json(&[
        "--dry-run",
        "--json",
        "--payload",
        package.to_str().unwrap(),
    ]);
    assert_eq!(field(&line, "kind"), "setup.plan");
    assert_eq!(field(&line, "plan"), "first_install");
    assert_eq!(field(&line, "files"), "3");
    assert!(!env.install_root.exists());

    let human = env.run(&["--dry-run", "--payload", package.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(text.contains("write "));
    assert!(text.contains("Untouched:"));
}

#[test]
fn verifying_a_package_file_checks_every_byte_without_installing() {
    let env = Env::new("verify-package");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let line = env.json(&["--verify", "--json", "--payload", package.to_str().unwrap()]);
    assert_eq!(field(&line, "intact"), "true");
    assert_eq!(field(&line, "files_checked"), "3");
    assert!(!env.install_root.exists());
}

#[test]
fn verifying_a_damaged_package_fails_loudly_and_exits_non_zero() {
    let env = Env::new("verify-damaged");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let mut bytes = std::fs::read(&package).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&package, bytes).unwrap();

    let output = env.run(&["--verify", "--json", "--payload", package.to_str().unwrap()]);
    assert!(!output.status.success());
    let line = String::from_utf8_lossy(&output.stdout);
    assert_eq!(field(&line, "ok"), "false");
    assert_eq!(field(&line, "error_code"), "digest_mismatch");
}

#[test]
fn a_silent_install_places_the_files_and_reports_where_to_run_them() {
    let env = Env::new("silent");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let line = env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    assert_eq!(field(&line, "ok"), "true");
    assert_eq!(field(&line, "kind"), "setup.install");
    assert_eq!(field(&line, "version"), "0.1.0");
    assert_eq!(field(&line, "files_written"), "3");

    let launch = PathBuf::from(field(&line, "launch_path"));
    assert!(launch.is_file());
    assert_eq!(std::fs::read(&launch).unwrap(), DESKTOP_V1);

    // And the thing it installed runs.
    #[cfg(unix)]
    {
        let output = Command::new(&launch).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "desktop-0.1.0"
        );
    }

    let status = env.json(&["--status", "--json"]);
    assert_eq!(field(&status, "active_version"), "0.1.0");
    assert_eq!(field(&status, "user_data_present"), "false");
}

#[test]
fn the_human_output_of_an_install_names_the_platform_limit_it_hit() {
    let env = Env::new("note");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let output = env.run(&["--silent", "--payload", package.to_str().unwrap()]);
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("re-verified"));
    #[cfg(not(windows))]
    assert!(text.contains("Shell integration skipped"));
    #[cfg(windows)]
    assert!(!text.contains("Shell integration skipped"));
}

#[test]
fn an_upgrade_then_a_rollback_returns_to_the_earlier_build() {
    let env = Env::new("upgrade-rollback");
    let first = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let second = write_package(&env.base, "0.2.0", DESKTOP_V2);
    env.json(&["--silent", "--json", "--payload", first.to_str().unwrap()]);
    let upgraded = env.json(&["--silent", "--json", "--payload", second.to_str().unwrap()]);
    assert_eq!(field(&upgraded, "version"), "0.2.0");
    assert_eq!(field(&upgraded, "previous_version"), "0.1.0");
    assert_eq!(
        std::fs::read(PathBuf::from(field(&upgraded, "launch_path"))).unwrap(),
        DESKTOP_V2
    );

    let rolled = env.json(&["--rollback", "--json"]);
    assert_eq!(field(&rolled, "ok"), "true");
    assert_eq!(field(&rolled, "active_version"), "0.1.0");
    assert_eq!(field(&rolled, "previous_version"), "null");
    assert_eq!(
        std::fs::read(PathBuf::from(field(&rolled, "launch_path"))).unwrap(),
        DESKTOP_V1
    );
}

#[test]
fn installing_an_older_build_is_refused_until_downgrade_is_allowed() {
    let env = Env::new("downgrade");
    let first = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let second = write_package(&env.base, "0.2.0", DESKTOP_V2);
    env.json(&["--silent", "--json", "--payload", second.to_str().unwrap()]);

    let refused = env.run(&["--silent", "--json", "--payload", first.to_str().unwrap()]);
    assert!(!refused.status.success());
    let line = String::from_utf8_lossy(&refused.stdout);
    assert_eq!(field(&line, "error_code"), "would_downgrade");

    let allowed = env.json(&[
        "--silent",
        "--json",
        "--allow-downgrade",
        "--payload",
        first.to_str().unwrap(),
    ]);
    assert_eq!(field(&allowed, "version"), "0.1.0");
}

#[test]
fn verifying_an_installation_detects_a_file_changed_after_install() {
    let env = Env::new("verify-installed");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let installed = env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    let launch = PathBuf::from(field(&installed, "launch_path"));

    let clean = env.json(&["--verify", "--json"]);
    assert_eq!(field(&clean, "intact"), "true");

    // Exactly the same length, different contents: the case a size check
    // alone would miss, and the one a patched binary actually looks like.
    let swapped = b"#!/bin/sh\necho desktop-9.9.9\n";
    assert_eq!(swapped.len(), DESKTOP_V1.len());
    std::fs::write(&launch, swapped).unwrap();
    let output = env.run(&["--verify", "--json"]);
    // A tampered installation is a failing exit code, not a success with a
    // warning buried in the output.
    assert!(!output.status.success());
    let line = String::from_utf8_lossy(&output.stdout);
    assert_eq!(field(&line, "intact"), "false");
    assert!(line.contains("digest:mininet-desktop.exe"), "{line}");

    // A truncated file is reported as a length problem, which names the two
    // numbers a user can act on rather than only "wrong".
    std::fs::write(&launch, b"short").unwrap();
    let truncated = String::from_utf8_lossy(&env.run(&["--verify", "--json"]).stdout).to_string();
    assert!(
        truncated.contains("length:mininet-desktop.exe:expected=29:found=5"),
        "{truncated}"
    );
}

#[test]
fn uninstall_removes_the_program_and_keeps_identities() {
    let env = Env::new("uninstall");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    std::fs::create_dir_all(&env.user_data).unwrap();
    std::fs::write(env.user_data.join("identity.dpapi"), b"keep me").unwrap();

    let line = env.json(&["--uninstall", "--json"]);
    assert_eq!(field(&line, "ok"), "true");
    assert_eq!(field(&line, "identities_destroyed"), "false");
    assert!(!env.install_root.exists());
    assert_eq!(
        std::fs::read(env.user_data.join("identity.dpapi")).unwrap(),
        b"keep me"
    );
}

#[test]
fn uninstall_destroys_identities_only_when_that_flag_is_given() {
    let env = Env::new("uninstall-destroy");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    std::fs::create_dir_all(&env.user_data).unwrap();
    std::fs::write(env.user_data.join("identity.dpapi"), b"goodbye").unwrap();

    let line = env.json(&["--uninstall", "--destroy-identities", "--json"]);
    assert_eq!(field(&line, "identities_destroyed"), "true");
    assert!(!env.user_data.exists());
}

#[test]
fn destroying_identities_without_uninstalling_is_rejected_before_anything_runs() {
    let env = Env::new("destroy-guard");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    std::fs::create_dir_all(&env.user_data).unwrap();
    std::fs::write(env.user_data.join("identity.dpapi"), b"safe").unwrap();

    let output = env.run(&[
        "--silent",
        "--destroy-identities",
        "--payload",
        package.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(env.user_data.join("identity.dpapi").is_file());
    assert!(!env.install_root.exists());
}

#[test]
fn a_missing_package_is_a_clear_error_rather_than_a_crash() {
    let env = Env::new("no-package");
    let output = env.run(&["--silent", "--json", "--payload", "/nope/missing.mnpkg"]);
    assert!(!output.status.success());
    let line = String::from_utf8_lossy(&output.stdout);
    assert_eq!(field(&line, "error_code"), "no_payload");
}

#[test]
fn rolling_back_with_nothing_to_roll_back_to_says_so() {
    let env = Env::new("rollback-empty");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    let output = env.run(&["--rollback", "--json"]);
    assert!(!output.status.success());
    let line = String::from_utf8_lossy(&output.stdout);
    assert_eq!(field(&line, "error_code"), "no_previous_version");
}

#[test]
fn a_portable_install_can_be_asked_for_no_shell_integration_at_all() {
    let env = Env::new("portable");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    let line = env.json(&[
        "--silent",
        "--json",
        "--no-start-menu",
        "--no-register",
        "--payload",
        package.to_str().unwrap(),
    ]);
    assert_eq!(field(&line, "shell_actions"), "[]");
    assert!(PathBuf::from(field(&line, "launch_path")).is_file());
}

#[test]
fn the_setup_log_is_readable_text_after_an_install() {
    let env = Env::new("log");
    let package = write_package(&env.base, "0.1.0", DESKTOP_V1);
    env.json(&["--silent", "--json", "--payload", package.to_str().unwrap()]);
    let log = std::fs::read_to_string(env.install_root.join("setup-log.txt")).unwrap();
    assert!(log.contains("install-started version=0.1.0"));
    assert!(log.contains("activated version=0.1.0"));
}

//! The install lifecycle, end to end, on whatever platform runs the tests.
//!
//! Every test here drives the real engine against a real temporary
//! directory: files are written, read back, hashed, activated, rolled back,
//! and removed. Only the two genuinely Windows-specific steps --- the `.lnk`
//! and the `HKCU` entry --- are captured by `RecordingShell` instead of
//! performed, and the tests assert on exactly what would have been done.

use mini_windows_setup::container::{self, Container};
use mini_windows_setup::manifest::{ManifestHeader, PackageManifest, PackageShortcut};
use mini_windows_setup::{
    InstallApproval, InstallOptions, PackageFile, PlanKind, RecordingShell, Setup, ShellAction,
    UninstallApproval, VerifyProblem,
};
use std::path::PathBuf;

const DESKTOP_V1: &[u8] = b"#!/bin/sh\necho mininet-desktop 0.1.0\n";
const DESKTOP_V2: &[u8] = b"#!/bin/sh\necho mininet-desktop 0.2.0\n";
const CLI: &[u8] = b"#!/bin/sh\necho mini cli\n";
const SETUP: &[u8] = b"#!/bin/sh\necho mininet-setup\n";

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-windows-setup-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn package(version: &str, desktop: &[u8]) -> PackageManifest {
    PackageManifest::new(
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
    .unwrap()
}

fn bytes_for(manifest: &PackageManifest, desktop: &[u8]) -> Vec<u8> {
    container::write(manifest, |path| {
        Ok(match path {
            "mininet-desktop.exe" => desktop.to_vec(),
            "mini.exe" => CLI.to_vec(),
            "mininet-setup.exe" => SETUP.to_vec(),
            other => panic!("unexpected file {other}"),
        })
    })
    .unwrap()
}

/// Options whose shell paths stay inside the test directory, so a test run
/// can never touch a real Start Menu.
fn options(root: &std::path::Path) -> InstallOptions {
    InstallOptions {
        start_menu_dir: Some(root.join("start-menu")),
        desktop_dir: Some(root.join("desktop")),
        ..InstallOptions::default()
    }
}

struct Fixture {
    base: PathBuf,
    setup: Setup,
    options: InstallOptions,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let base = tempdir(tag);
        let install_root = base.join("Programs").join("Mininet");
        let setup = Setup::new(&install_root).with_user_data_root(base.join("UserData"));
        let options = options(&base);
        Self {
            base,
            setup,
            options,
        }
    }

    fn install(
        &self,
        version: &str,
        desktop: &[u8],
        now_ms: u64,
        shell: &mut RecordingShell,
    ) -> Result<mini_windows_setup::InstallReport, mini_windows_setup::SetupError> {
        let manifest = package(version, desktop);
        let bytes = bytes_for(&manifest, desktop);
        let container = Container::open(&bytes).unwrap();
        let approval = InstallApproval::new(container.manifest(), now_ms);
        self.setup
            .install(&container, &approval, &self.options, shell, now_ms)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[test]
fn a_first_install_writes_every_file_activates_it_and_registers_one_shortcut() {
    let fixture = Fixture::new("first");
    let mut shell = RecordingShell::default();
    let report = fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();

    assert_eq!(report.files_written, 3);
    assert_eq!(
        report.bytes_written,
        (DESKTOP_V1.len() + CLI.len() + SETUP.len()) as u64
    );
    assert!(report.previous.is_none());
    assert!(report.launch_path.is_file());
    assert_eq!(std::fs::read(&report.launch_path).unwrap(), DESKTOP_V1);

    let status = fixture.setup.status().unwrap();
    let active = status.active.expect("a version is active");
    assert_eq!(active.version_text, "0.1.0");
    assert_eq!(status.installed_versions, vec!["0.1.0".to_string()]);
    assert!(status.previous.is_none());

    // Exactly one Start Menu shortcut and one Apps & features entry, and no
    // desktop shortcut: an installer that adds one without being asked is
    // the behaviour this project exists not to have.
    assert_eq!(shell.actions.len(), 2);
    match &shell.actions[0] {
        ShellAction::CreateShortcut(request) => {
            assert!(request.link_path.ends_with("Mininet.lnk"));
            assert_eq!(request.target, report.launch_path);
        }
        other => panic!("expected a shortcut, got {other:?}"),
    }
    match &shell.actions[1] {
        ShellAction::RegisterUninstall(registration) => {
            assert_eq!(registration.display_version, "0.1.0");
            assert!(registration.uninstall_command.contains("--uninstall"));
            assert!(registration
                .uninstall_command
                .contains("mininet-setup.exe"));
        }
        other => panic!("expected an uninstall registration, got {other:?}"),
    }
}

#[test]
fn the_installed_client_is_the_package_bytes_and_actually_runs() {
    // The point of an installer is that what it put on disk works. On a
    // Unix host the engine marks `.exe` entries executable, so the installed
    // artifact can be executed here rather than merely compared.
    let fixture = Fixture::new("runs");
    let mut shell = RecordingShell::default();
    let report = fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    #[cfg(unix)]
    {
        let output = std::process::Command::new(&report.launch_path)
            .output()
            .expect("the installed executable should run");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "mininet-desktop 0.1.0"
        );
    }
    #[cfg(not(unix))]
    assert!(report.launch_path.is_file());
}

#[test]
fn an_approval_for_one_build_cannot_install_a_different_build() {
    let fixture = Fixture::new("approval");
    let mut shell = RecordingShell::default();
    let approved = package("0.1.0", DESKTOP_V1);
    let offered = package("0.1.0", DESKTOP_V2);
    let bytes = bytes_for(&offered, DESKTOP_V2);
    let container = Container::open(&bytes).unwrap();
    // Same package name, same version, different contents.
    assert_ne!(approved.digest_hex(), offered.digest_hex());
    let approval = InstallApproval::new(&approved, 1_000);
    let error = fixture
        .setup
        .install(&container, &approval, &fixture.options, &mut shell, 1_000)
        .unwrap_err();
    assert_eq!(error.code(), "approval_mismatch");
    // Nothing was written and nothing was registered.
    assert!(fixture.setup.status().unwrap().active.is_none());
    assert!(shell.actions.is_empty());
}

#[test]
fn an_upgrade_records_the_older_version_as_the_rollback_target() {
    let fixture = Fixture::new("upgrade");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let report = fixture.install("0.2.0", DESKTOP_V2, 2_000, &mut shell).unwrap();

    assert_eq!(report.active.version_text, "0.2.0");
    assert_eq!(report.previous.unwrap().version_text, "0.1.0");
    let status = fixture.setup.status().unwrap();
    assert_eq!(status.active.unwrap().version_text, "0.2.0");
    assert_eq!(status.previous.unwrap().version_text, "0.1.0");
    // The old version's files are still on disk: that is what makes the
    // rollback real rather than a re-download.
    assert_eq!(
        status.installed_versions,
        vec!["0.1.0".to_string(), "0.2.0".to_string()]
    );
    assert_eq!(std::fs::read(&report.launch_path).unwrap(), DESKTOP_V2);
}

#[test]
fn installing_an_older_version_over_a_newer_one_is_refused_by_default() {
    let fixture = Fixture::new("downgrade");
    let mut shell = RecordingShell::default();
    fixture.install("0.2.0", DESKTOP_V2, 1_000, &mut shell).unwrap();
    let error = fixture
        .install("0.1.0", DESKTOP_V1, 2_000, &mut shell)
        .unwrap_err();
    assert_eq!(error.code(), "would_downgrade");
    // Still on the newer version: a refused downgrade changes nothing.
    assert_eq!(
        fixture.setup.status().unwrap().active.unwrap().version_text,
        "0.2.0"
    );
}

#[test]
fn a_downgrade_is_possible_when_the_caller_declares_one() {
    let mut fixture = Fixture::new("downgrade-ok");
    let mut shell = RecordingShell::default();
    fixture.install("0.2.0", DESKTOP_V2, 1_000, &mut shell).unwrap();
    fixture.options.allow_downgrade = true;
    let report = fixture.install("0.1.0", DESKTOP_V1, 2_000, &mut shell).unwrap();
    assert_eq!(report.active.version_text, "0.1.0");
    assert_eq!(std::fs::read(&report.launch_path).unwrap(), DESKTOP_V1);
}

#[test]
fn planning_reports_the_relationship_to_what_is_already_installed() {
    let fixture = Fixture::new("plan");
    let mut shell = RecordingShell::default();
    let first = package("0.1.0", DESKTOP_V1);
    assert_eq!(
        fixture.setup.plan(&first, &fixture.options).unwrap().kind,
        PlanKind::FirstInstall
    );
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    assert_eq!(
        fixture.setup.plan(&first, &fixture.options).unwrap().kind,
        PlanKind::Reinstall
    );
    assert_eq!(
        fixture
            .setup
            .plan(&package("0.2.0", DESKTOP_V2), &fixture.options)
            .unwrap()
            .kind,
        PlanKind::Upgrade
    );
    assert_eq!(
        fixture
            .setup
            .plan(&package("0.0.9", DESKTOP_V1), &fixture.options)
            .unwrap()
            .kind,
        PlanKind::Downgrade
    );
}

#[test]
fn a_plan_lists_every_change_before_anything_is_written() {
    let fixture = Fixture::new("plan-detail");
    let manifest = package("0.1.0", DESKTOP_V1);
    let plan = fixture.setup.plan(&manifest, &fixture.options).unwrap();
    assert_eq!(plan.files.len(), 3);
    assert_eq!(plan.total_bytes, manifest.total_bytes());
    assert_eq!(plan.package_digest, manifest.digest_hex());
    assert_eq!(plan.shell_actions.len(), 2);
    assert!(plan.user_data_root.ends_with("UserData"));
    for file in &plan.files {
        assert!(file.destination.starts_with(&plan.version_dir));
        assert!(!file.destination.exists());
    }
    assert!(!plan.install_root.join("current.txt").exists());
}

#[test]
fn a_reinstall_of_the_active_version_keeps_the_existing_rollback_target() {
    let fixture = Fixture::new("reinstall");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    fixture.install("0.2.0", DESKTOP_V2, 2_000, &mut shell).unwrap();
    let report = fixture.install("0.2.0", DESKTOP_V2, 3_000, &mut shell).unwrap();
    assert_eq!(report.previous.unwrap().version_text, "0.1.0");
    assert_eq!(
        fixture.setup.status().unwrap().previous.unwrap().version_text,
        "0.1.0"
    );
}

#[test]
fn verification_reads_the_installed_files_back_and_finds_them_intact() {
    let fixture = Fixture::new("verify");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let report = fixture.setup.verify_installed("0.1.0").unwrap();
    assert!(report.is_intact());
    assert_eq!(report.files_checked, 3);
    assert_eq!(
        report.bytes_checked,
        (DESKTOP_V1.len() + CLI.len() + SETUP.len()) as u64
    );
}

#[test]
fn verification_reports_every_problem_rather_than_only_the_first() {
    let fixture = Fixture::new("verify-bad");
    let mut shell = RecordingShell::default();
    let report = fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let version_dir = report.launch_path.parent().unwrap().to_path_buf();

    // One file tampered with, one truncated, one deleted, one added.
    std::fs::write(version_dir.join("mininet-desktop.exe"), DESKTOP_V2).unwrap();
    std::fs::write(version_dir.join("mini.exe"), b"short").unwrap();
    std::fs::remove_file(version_dir.join("mininet-setup.exe")).unwrap();
    std::fs::write(version_dir.join("extra.dll"), b"hijack").unwrap();

    let verify = fixture.setup.verify_installed("0.1.0").unwrap();
    assert!(!verify.is_intact());
    assert!(verify.problems.iter().any(|problem| matches!(
        problem,
        VerifyProblem::Digest { path } if path == "mininet-desktop.exe"
    )));
    assert!(verify.problems.iter().any(|problem| matches!(
        problem,
        VerifyProblem::Length { path, .. } if path == "mini.exe"
    )));
    assert!(verify.problems.iter().any(|problem| matches!(
        problem,
        VerifyProblem::Missing { path } if path == "mininet-setup.exe"
    )));
    assert!(verify.problems.iter().any(|problem| matches!(
        problem,
        VerifyProblem::Unexpected { path } if path == "extra.dll"
    )));
}

#[test]
fn rollback_returns_to_the_previous_version_and_repoints_the_shortcut() {
    let fixture = Fixture::new("rollback");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    fixture.install("0.2.0", DESKTOP_V2, 2_000, &mut shell).unwrap();

    let mut rollback_shell = RecordingShell::default();
    let restored = fixture
        .setup
        .rollback(&fixture.options, &mut rollback_shell, 3_000)
        .unwrap();
    assert_eq!(restored.version_text, "0.1.0");

    let status = fixture.setup.status().unwrap();
    assert_eq!(status.active.unwrap().version_text, "0.1.0");
    // The rollback target is consumed: rolling back twice in a row would be
    // a downgrade nobody asked for.
    assert!(status.previous.is_none());
    let launch = status.launch_path.unwrap();
    assert_eq!(std::fs::read(&launch).unwrap(), DESKTOP_V1);

    match &rollback_shell.actions[0] {
        ShellAction::CreateShortcut(request) => assert_eq!(request.target, launch),
        other => panic!("expected the shortcut to be repointed, got {other:?}"),
    }
    let error = fixture
        .setup
        .rollback(&fixture.options, &mut rollback_shell, 4_000)
        .unwrap_err();
    assert_eq!(error.code(), "no_previous_version");
}

#[test]
fn rollback_refuses_a_previous_version_whose_files_are_damaged() {
    let fixture = Fixture::new("rollback-bad");
    let mut shell = RecordingShell::default();
    let first = fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    fixture.install("0.2.0", DESKTOP_V2, 2_000, &mut shell).unwrap();
    std::fs::write(&first.launch_path, b"corrupted").unwrap();

    let error = fixture
        .setup
        .rollback(&fixture.options, &mut shell, 3_000)
        .unwrap_err();
    assert!(matches!(
        error.code(),
        "digest_mismatch" | "length_mismatch"
    ));
    // Still on the newer version rather than on a broken older one.
    assert_eq!(
        fixture.setup.status().unwrap().active.unwrap().version_text,
        "0.2.0"
    );
}

#[test]
fn uninstall_removes_program_files_and_keeps_identities_by_default() {
    let fixture = Fixture::new("uninstall");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let user_data = fixture.setup.user_data_root().to_path_buf();
    std::fs::create_dir_all(&user_data).unwrap();
    std::fs::write(user_data.join("identity.dpapi"), b"irreplaceable").unwrap();

    let mut uninstall_shell = RecordingShell::default();
    let approval =
        UninstallApproval::keeping_identities(fixture.setup.layout().root(), 2_000);
    let report = fixture
        .setup
        .uninstall(&approval, &fixture.options, &mut uninstall_shell, 2_000)
        .unwrap();

    assert_eq!(report.versions_removed, vec!["0.1.0".to_string()]);
    assert!(!fixture.setup.layout().root().exists());
    assert_eq!(report.user_data_kept.as_deref(), Some(user_data.as_path()));
    assert!(report.user_data_destroyed.is_none());
    // The one thing a person cannot recreate is still there.
    assert_eq!(
        std::fs::read(user_data.join("identity.dpapi")).unwrap(),
        b"irreplaceable"
    );

    assert!(uninstall_shell
        .actions
        .iter()
        .any(|action| matches!(action, ShellAction::RemoveShortcut { .. })));
    assert!(uninstall_shell
        .actions
        .iter()
        .any(|action| matches!(action, ShellAction::DeregisterUninstall { .. })));
}

#[test]
fn uninstall_destroys_identities_only_when_that_exact_path_was_approved() {
    let fixture = Fixture::new("uninstall-destroy");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let user_data = fixture.setup.user_data_root().to_path_buf();
    std::fs::create_dir_all(&user_data).unwrap();
    std::fs::write(user_data.join("identity.dpapi"), b"gone").unwrap();

    let approval = UninstallApproval::destroying_identities(
        fixture.setup.layout().root(),
        &user_data,
        2_000,
    );
    let report = fixture
        .setup
        .uninstall(&approval, &fixture.options, &mut shell, 2_000)
        .unwrap();
    assert_eq!(
        report.user_data_destroyed.as_deref(),
        Some(user_data.as_path())
    );
    assert!(report.user_data_kept.is_none());
    assert!(!user_data.exists());
}

#[test]
fn an_uninstall_approval_for_another_install_root_is_refused() {
    let fixture = Fixture::new("uninstall-wrong-root");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let approval = UninstallApproval::keeping_identities("/somewhere/else", 2_000);
    let error = fixture
        .setup
        .uninstall(&approval, &fixture.options, &mut shell, 2_000)
        .unwrap_err();
    assert_eq!(error.code(), "approval_mismatch");
    assert!(fixture.setup.layout().root().exists());
}

#[test]
fn the_setup_log_records_each_step_and_outlives_the_uninstall() {
    let fixture = Fixture::new("log");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    fixture.install("0.2.0", DESKTOP_V2, 2_000, &mut shell).unwrap();
    fixture
        .setup
        .rollback(&fixture.options, &mut shell, 3_000)
        .unwrap();

    let log = mini_windows_setup::SetupLog::new(fixture.setup.layout().log_path());
    let lines = log.read().unwrap();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("install-started"))
            .count(),
        2
    );
    assert!(lines.iter().any(|line| line.contains("rolled-back version=0.1.0")));
    // No path from the user's profile leaks into a log people are asked to
    // paste into bug reports.
    for line in &lines {
        assert!(!line.contains("mini-windows-setup-log"));
    }

    let approval =
        UninstallApproval::keeping_identities(fixture.setup.layout().root(), 4_000);
    fixture
        .setup
        .uninstall(&approval, &fixture.options, &mut shell, 4_000)
        .unwrap();
    let preserved = fixture
        .setup
        .layout()
        .root()
        .with_extension("uninstalled.log.txt");
    let after = mini_windows_setup::SetupLog::new(&preserved).read().unwrap();
    assert!(after.len() > lines.len());
    assert!(after.last().unwrap().contains("uninstalled"));
}

#[test]
fn a_failed_install_leaves_the_running_version_active() {
    let fixture = Fixture::new("failed");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();

    // A container whose payload was damaged after it was built: the manifest
    // is internally consistent, so this is only caught when the bytes are
    // read on their way to disk.
    let manifest = package("0.2.0", DESKTOP_V2);
    let mut bytes = bytes_for(&manifest, DESKTOP_V2);
    let length = bytes.len();
    bytes[length - 1] ^= 0xff;
    let container = Container::open(&bytes).unwrap();
    let approval = InstallApproval::new(container.manifest(), 2_000);
    let error = fixture
        .setup
        .install(&container, &approval, &fixture.options, &mut shell, 2_000)
        .unwrap_err();
    assert_eq!(error.code(), "digest_mismatch");

    let status = fixture.setup.status().unwrap();
    assert_eq!(status.active.unwrap().version_text, "0.1.0");
    assert_eq!(status.installed_versions, vec!["0.1.0".to_string()]);
    assert_eq!(
        std::fs::read(status.launch_path.unwrap()).unwrap(),
        DESKTOP_V1
    );
}

#[test]
fn status_on_an_empty_root_is_an_answer_not_an_error() {
    let base = tempdir("empty");
    let setup = Setup::new(base.join("never-installed"));
    let status = setup.status().unwrap();
    assert!(status.active.is_none());
    assert!(status.previous.is_none());
    assert!(status.installed_versions.is_empty());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_corrupt_pointer_file_is_reported_rather_than_guessed_through() {
    let fixture = Fixture::new("corrupt-pointer");
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    std::fs::write(fixture.setup.layout().current_path(), b"garbage\n").unwrap();
    let error = fixture.setup.status().unwrap_err();
    assert_eq!(error.code(), "corrupt_pointer");
}

#[test]
fn a_desktop_shortcut_is_only_created_when_the_user_asks_for_one() {
    let mut fixture = Fixture::new("desktop");
    fixture.options.desktop_shortcut = true;
    let mut shell = RecordingShell::default();
    fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    let shortcuts = shell
        .actions
        .iter()
        .filter(|action| matches!(action, ShellAction::CreateShortcut(_)))
        .count();
    assert_eq!(shortcuts, 2);
}

#[test]
fn a_portable_install_can_skip_every_shell_change() {
    let mut fixture = Fixture::new("portable");
    fixture.options.start_menu_shortcut = false;
    fixture.options.register_uninstall = false;
    let mut shell = RecordingShell::default();
    let report = fixture.install("0.1.0", DESKTOP_V1, 1_000, &mut shell).unwrap();
    assert!(shell.actions.is_empty());
    assert!(report.launch_path.is_file());
}

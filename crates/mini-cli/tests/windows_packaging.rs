//! `mini windows ...` through the real CLI text interface.
//!
//! The engine's own suite covers install semantics; this file exists to
//! prove the CLI plumbing does not weaken any of it, and that `pack` is
//! genuinely reproducible --- which is the property the whole
//! independent-builder-agreement idea rests on, and the one a build script
//! silently stamping the clock would destroy.

use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "mini-cli-windows-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn source_tree(base: &Path, desktop: &[u8]) -> PathBuf {
    let source = base.join("src");
    std::fs::create_dir_all(source.join("docs")).unwrap();
    std::fs::write(source.join("mininet-desktop.exe"), desktop).unwrap();
    std::fs::write(source.join("mini.exe"), b"cli\n").unwrap();
    std::fs::write(source.join("docs/README.txt"), b"read me\n").unwrap();
    source
}

fn run(args: &[&str]) -> mini_cli::Result<String> {
    mini_cli::run(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
}

fn pack(base: &Path, source: &Path, version: &str, built_at_ms: &str) -> PathBuf {
    let out = base.join(format!("client-{version}.mnpkg"));
    let home = base.join("home");
    run(&[
        "--home",
        home.to_str().unwrap(),
        "windows",
        "pack",
        "--source",
        source.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--version",
        version,
        "--built-at-ms",
        built_at_ms,
        "--shortcut",
        "mininet-desktop.exe=Mininet",
    ])
    .unwrap();
    out
}

#[test]
fn packing_the_same_bytes_twice_produces_identical_containers() {
    let base = tempdir("reproducible");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let first = pack(&base, &source, "0.1.0", "1757635200000");
    let first_bytes = std::fs::read(&first).unwrap();
    std::fs::remove_file(&first).unwrap();
    let second = pack(&base, &source, "0.1.0", "1757635200000");
    assert_eq!(std::fs::read(&second).unwrap(), first_bytes);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_different_build_timestamp_is_a_different_package() {
    let base = tempdir("timestamp");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let first = std::fs::read(pack(&base, &source, "0.1.0", "1757635200000")).unwrap();
    std::fs::remove_file(base.join("client-0.1.0.mnpkg")).unwrap();
    let second = std::fs::read(pack(&base, &source, "0.1.0", "1757635200001")).unwrap();
    assert_ne!(first, second);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn pack_without_a_build_timestamp_is_a_usage_error() {
    let base = tempdir("no-timestamp");
    let source = source_tree(&base, b"desktop\n");
    let out = base.join("client.mnpkg");
    let error = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "windows",
        "pack",
        "--source",
        source.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--version",
        "0.1.0",
    ])
    .unwrap_err();
    assert_eq!(error.error_code(), "usage");
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn pack_writes_a_readable_manifest_beside_the_container() {
    let base = tempdir("manifest");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let manifest = container.with_extension("manifest.txt");
    let text = std::fs::read_to_string(&manifest).unwrap();
    assert!(text.starts_with("MNWINPKG1\n"));
    assert!(text.contains("product Mininet\n"));
    assert!(text.contains("shortcut mininet-desktop.exe Mininet\n"));
    // Both digests per file, so the SHA-256 column can be checked with
    // Get-FileHash by someone who trusts nothing we shipped.
    let file_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("file "))
        .collect();
    assert_eq!(file_lines.len(), 3);
    for line in file_lines {
        let parts: Vec<&str> = line.split(' ').collect();
        assert_eq!(parts[2].len(), 64, "blake3 column");
        assert_eq!(parts[3].len(), 64, "sha256 column");
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_sha256_column_matches_what_an_outside_tool_would_compute() {
    let base = tempdir("sha256");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let text = std::fs::read_to_string(container.with_extension("manifest.txt")).unwrap();
    let readme_line = text
        .lines()
        .find(|line| line.ends_with("docs/README.txt"))
        .unwrap();
    let recorded = readme_line.split(' ').nth(3).unwrap();
    let mut expected = String::new();
    for byte in mini_crypto::hash::sha2_256(b"read me\n") {
        expected.push_str(&format!("{byte:02x}"));
    }
    assert_eq!(recorded, expected);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn inspect_works_on_a_container_and_on_the_bare_manifest() {
    let base = tempdir("inspect");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let home = base.join("home");

    let from_container = run(&[
        "--home",
        home.to_str().unwrap(),
        "windows",
        "inspect",
        container.to_str().unwrap(),
    ])
    .unwrap();
    assert!(from_container.contains("every file matches"));
    assert!(from_container.contains("mininet-desktop.exe"));

    let from_manifest = run(&[
        "--home",
        home.to_str().unwrap(),
        "windows",
        "inspect",
        container.with_extension("manifest.txt").to_str().unwrap(),
    ])
    .unwrap();
    assert!(from_manifest.contains("manifest)"));
    assert!(!from_manifest.contains("every file matches"));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn inspecting_a_damaged_container_reports_the_engines_own_error_code() {
    let base = tempdir("inspect-damaged");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let mut bytes = std::fs::read(&container).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(&container, bytes).unwrap();

    let error = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "windows",
        "inspect",
        container.to_str().unwrap(),
    ])
    .unwrap_err();
    // Not flattened to one "windows_setup" code: a script can branch on the
    // same value `mininet-setup --json` would report.
    assert_eq!(error.error_code(), "digest_mismatch");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_file_that_cannot_be_a_windows_path_stops_the_pack() {
    let base = tempdir("bad-path");
    let source = source_tree(&base, b"desktop\n");
    // `aux` is a DOS device name, not a file name, on every Windows.
    std::fs::write(source.join("aux.dll"), b"x").unwrap();
    let out = base.join("client.mnpkg");
    let error = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "windows",
        "pack",
        "--source",
        source.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--version",
        "0.1.0",
        "--built-at-ms",
        "1",
    ])
    .unwrap_err();
    assert_eq!(error.error_code(), "unsafe_path");
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn plan_changes_nothing_and_names_every_file_and_shell_action() {
    let base = tempdir("plan");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let install_root = base.join("Programs");
    let text = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "windows",
        "plan",
        container.to_str().unwrap(),
        "--install-root",
        install_root.to_str().unwrap(),
        "--user-data-root",
        base.join("UserData").to_str().unwrap(),
        "--start-menu-dir",
        base.join("menu").to_str().unwrap(),
    ])
    .unwrap();
    assert!(text.contains("first_install"));
    assert!(text.contains("mininet-desktop.exe"));
    assert!(text.contains("create-shortcut:"));
    assert!(text.contains("register-uninstall:Mininet"));
    assert!(text.contains("Run mininet-setup to install."));
    assert!(!install_root.exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn status_and_verify_report_an_empty_install_root_without_pretending() {
    let base = tempdir("status");
    let home = base.join("home");
    let install_root = base.join("Programs");
    let status = run(&[
        "--home",
        home.to_str().unwrap(),
        "windows",
        "status",
        "--install-root",
        install_root.to_str().unwrap(),
        "--user-data-root",
        base.join("UserData").to_str().unwrap(),
    ])
    .unwrap();
    assert!(status.contains("Nothing installed"));

    let error = run(&[
        "--home",
        home.to_str().unwrap(),
        "windows",
        "verify",
        "--install-root",
        install_root.to_str().unwrap(),
    ])
    .unwrap_err();
    assert_eq!(error.error_code(), "usage");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn json_output_carries_the_same_field_names_the_setup_binary_uses() {
    let base = tempdir("json");
    let source = source_tree(&base, b"desktop-0.1.0\n");
    let container = pack(&base, &source, "0.1.0", "1757635200000");
    let line = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "--json",
        "windows",
        "plan",
        container.to_str().unwrap(),
        "--install-root",
        base.join("Programs").to_str().unwrap(),
        "--user-data-root",
        base.join("UserData").to_str().unwrap(),
    ])
    .unwrap();
    assert!(line.starts_with("{\"ok\":true,\"kind\":\"windows.plan\""));
    for key in [
        "package_digest",
        "plan",
        "install_root",
        "version_dir",
        "launch_path",
        "files",
        "total_bytes",
        "shell_actions",
        "user_data_root",
    ] {
        assert!(
            line.contains(&format!("\"{key}\":")),
            "missing {key} in {line}"
        );
    }
    assert!(!line.contains('\n'));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn an_unknown_windows_subcommand_is_a_usage_error() {
    let base = tempdir("unknown");
    let error = run(&[
        "--home",
        base.join("home").to_str().unwrap(),
        "windows",
        "install",
    ])
    .unwrap_err();
    assert_eq!(error.error_code(), "usage");
    assert!(error.to_string().contains("install"));
    let _ = std::fs::remove_dir_all(base);
}

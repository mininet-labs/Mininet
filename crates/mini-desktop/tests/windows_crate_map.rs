//! The Windows product integration map must describe the workspace that actually exists.
//!
//! This is deliberately separate from `mini-selftest`'s diagnostic coverage table.
//! A crate can be useful to the Windows product without being linked into the GUI process,
//! and several crates must stay behind process boundaries for security and governance.

use std::collections::{BTreeMap, BTreeSet};

const WORKSPACE: &str = include_str!("../../../Cargo.toml");
const DESKTOP_MANIFEST: &str = include_str!("../Cargo.toml");
const MAP: &str = include_str!("../windows-crate-map.tsv");

#[derive(Debug)]
struct Row<'a> {
    current: &'a str,
    target_process: &'a str,
    surface: &'a str,
    wave: &'a str,
    gate: &'a str,
}

fn workspace_members() -> BTreeSet<String> {
    let members = WORKSPACE
        .split_once("members = [")
        .expect("workspace Cargo.toml has a members list")
        .1
        .split_once(']')
        .expect("workspace members list is closed")
        .0;
    members
        .lines()
        .filter_map(|line| {
            line.trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .strip_prefix("crates/")
                .map(str::to_owned)
        })
        .collect()
}

fn map_rows() -> BTreeMap<&'static str, Row<'static>> {
    let mut rows = BTreeMap::new();
    for (line_no, line) in MAP.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') || line.starts_with("crate\t") {
            continue;
        }
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(
            columns.len(),
            6,
            "windows-crate-map.tsv line {} must have exactly six tab-separated columns",
            line_no + 1
        );
        let old = rows.insert(
            columns[0],
            Row {
                current: columns[1],
                target_process: columns[2],
                surface: columns[3],
                wave: columns[4],
                gate: columns[5],
            },
        );
        assert!(old.is_none(), "{} is listed twice in the Windows map", columns[0]);
    }
    rows
}

fn direct_mininet_dependencies() -> BTreeSet<String> {
    DESKTOP_MANIFEST
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.contains("path = \"../") {
                return None;
            }
            line.split_once('=')
                .map(|(name, _)| name.trim().to_owned())
        })
        .collect()
}

#[test]
fn every_workspace_crate_has_exactly_one_windows_product_decision() {
    let members = workspace_members();
    let rows = map_rows();
    let mapped: BTreeSet<String> = rows.keys().map(|name| (*name).to_owned()).collect();

    let missing: Vec<_> = members.difference(&mapped).cloned().collect();
    let stale: Vec<_> = mapped.difference(&members).cloned().collect();

    assert!(
        missing.is_empty(),
        "workspace crates missing from the Windows integration map: {missing:?}"
    );
    assert!(
        stale.is_empty(),
        "Windows integration rows that no longer name workspace crates: {stale:?}"
    );
    assert_eq!(members.len(), rows.len());
}

#[test]
fn direct_desktop_dependencies_are_declared_as_direct_and_only_them() {
    let direct = direct_mininet_dependencies();
    let rows = map_rows();
    let declared: BTreeSet<String> = rows
        .iter()
        .filter_map(|(name, row)| (row.current == "direct").then(|| (*name).to_owned()))
        .collect();

    assert_eq!(
        direct, declared,
        "the map's current=direct rows must match mini-desktop path dependencies exactly"
    );
}

#[test]
fn every_row_uses_a_known_process_wave_and_nonempty_product_contract() {
    const PROCESSES: &[&str] = &[
        "ui",
        "core-service",
        "node-service",
        "wallet-service",
        "forge-service",
        "worker",
        "setup-tool",
        "diagnostics-tool",
        "tooling",
        "platform-excluded",
    ];
    const WAVES: &[&str] = &["W0", "W1", "W2", "W3", "W4", "W5", "W6", "W7"];
    const CURRENT: &[&str] = &["host", "direct", "none"];

    for (name, row) in map_rows() {
        assert!(
            CURRENT.contains(&row.current),
            "{name} has unknown current status {:?}",
            row.current
        );
        assert!(
            PROCESSES.contains(&row.target_process),
            "{name} has unknown target process {:?}",
            row.target_process
        );
        assert!(
            WAVES.contains(&row.wave),
            "{name} has unknown implementation wave {:?}",
            row.wave
        );
        assert!(!row.surface.trim().is_empty(), "{name} has no product surface");
        assert!(
            row.gate.trim().len() >= 20,
            "{name} has no meaningful integration gate: {:?}",
            row.gate
        );
    }
}

#[test]
fn sensitive_domains_do_not_target_the_gui_process() {
    let rows = map_rows();
    for name in [
        "mini-forge",
        "mini-value",
        "mini-treasury",
        "mini-custody",
        "mini-private-payment",
        "mini-bounty",
        "mini-settlement",
        "mini-consensus",
        "mini-execution",
        "mini-build-runner-wasmtime",
    ] {
        assert_ne!(
            rows[name].target_process, "ui",
            "{name} must not be moved into the desktop rendering/security boundary"
        );
    }
}

#[test]
fn value_and_forge_are_assigned_to_separate_process_domains() {
    let rows = map_rows();
    assert_eq!(rows["mini-value"].target_process, "wallet-service");
    assert_eq!(rows["mini-treasury"].target_process, "wallet-service");
    assert_eq!(rows["mini-forge"].target_process, "forge-service");
    assert_eq!(
        rows["mini-build-runner-wasmtime"].target_process,
        "worker"
    );
}

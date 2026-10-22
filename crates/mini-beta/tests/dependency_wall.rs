//! Structural regression tests for PR #334's hardest safety boundary.
//!
//! `mini-beta` is deliberately a test-value leaf. If a future change adds a
//! production value, settlement, treasury, chain, consensus, Forge-governance,
//! or personhood dependency here, Beta MINI has stopped being cleanly isolated
//! and the change must be redesigned rather than waved through as convenience.

#[test]
fn beta_test_currency_has_no_production_value_or_authority_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    let forbidden = [
        "mini-value",
        "mini-private-payment",
        "mini-settlement",
        "mini-treasury",
        "mini-chain",
        "mini-consensus",
        "mini-forge",
        "mini-governance",
        "mini-uniqueness",
        "mini-economy",
        "mini-airdrop",
        "mini-airdrop-treasury",
    ];

    for dependency in forbidden {
        assert!(
            !manifest.contains(dependency),
            "mini-beta must remain isolated from production value/authority; forbidden dependency: {dependency}"
        );
    }
}

#[test]
fn beta_manifest_contains_only_the_expected_runtime_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    let dependency_section = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("mini-beta must have a dependency section");
    let dependency_section = dependency_section
        .split('[')
        .next()
        .unwrap_or(dependency_section);

    let names: Vec<&str> = dependency_section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split('=').next().map(str::trim))
        .collect();

    assert_eq!(names, ["did-mini", "mini-objects", "mini-store"]);
}

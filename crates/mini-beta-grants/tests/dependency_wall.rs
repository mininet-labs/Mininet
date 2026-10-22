//! Structural safety wall for decentralized Beta grant acceptance.
//!
//! This crate may coordinate Beta-only operational approvals, but it must not
//! grow dependencies on production value, treasury, consensus, personhood, or
//! governance systems. Those dependencies would turn test issuance into a
//! privileged cross-domain control point.

#[test]
fn grant_acceptance_has_no_production_value_or_governance_dependencies() {
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
        "reqwest",
        "octocrab",
    ];

    for dependency in forbidden {
        assert!(
            !manifest.contains(dependency),
            "mini-beta-grants must remain Beta-only; forbidden dependency: {dependency}"
        );
    }
}

#[test]
fn manifest_contains_only_the_expected_runtime_dependencies() {
    let manifest = include_str!("../Cargo.toml");
    let dependency_section = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("mini-beta-grants must have a dependency section");
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

    assert_eq!(
        names,
        ["did-mini", "mini-beta", "mini-objects", "mini-store"]
    );
}

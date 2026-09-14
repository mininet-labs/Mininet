//! Structural wall: durable Beta execution remains test-value infrastructure.

#[test]
fn durable_beta_execution_has_no_production_authority_dependencies() {
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
            "mini-beta-exec must remain Beta-only; forbidden dependency: {dependency}"
        );
    }
}

#[test]
fn runtime_dependencies_are_explicit_and_bounded() {
    let manifest = include_str!("../Cargo.toml");
    let section = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("dependency section")
        .split('[')
        .next()
        .unwrap();
    let names: Vec<&str> = section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split('=').next().map(str::trim))
        .collect();
    assert_eq!(
        names,
        [
            "blake3",
            "did-mini",
            "mini-beta",
            "mini-beta-grants",
            "mini-objects",
            "mini-store"
        ]
    );
}

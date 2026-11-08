//! P0-3 (research PR #429): the hand-rolled FROST DKG/signing in `mini-treasury`
//! must stay unreachable from production builds. This scans every workspace
//! manifest, so enabling a legacy feature anywhere else, or giving a binary
//! crate a way to switch it on, fails here first.

use std::fs;
use std::path::Path;

fn manifests() -> Vec<(String, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for entry in fs::read_dir(crates).unwrap() {
        let path = entry.unwrap().path().join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&path) {
            let name = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            out.push((name, text));
        }
    }
    out
}

#[test]
fn no_manifest_outside_mini_treasury_enables_a_legacy_hand_rolled_feature() {
    for (name, text) in manifests() {
        if name == "mini-treasury" {
            continue;
        }
        assert!(
            !text.contains("legacy-hand-rolled"),
            "{name} must not name a legacy hand-rolled FROST feature: production signing is mini-custody"
        );
    }
}

#[test]
fn the_legacy_features_are_off_by_default_in_mini_treasury() {
    let treasury = manifests()
        .into_iter()
        .find(|(name, _)| name == "mini-treasury")
        .unwrap()
        .1;
    let features = treasury.split("[features]").nth(1).unwrap();
    let section = features.split("\n[[").next().unwrap();
    assert!(
        !section.contains("default ="),
        "mini-treasury must not declare default features that could enable a legacy path"
    );
}

#[test]
fn mini_custody_does_not_depend_on_the_legacy_signer() {
    let custody = manifests()
        .into_iter()
        .find(|(name, _)| name == "mini-custody")
        .unwrap()
        .1;
    let deps = custody.split("[dependencies]").nth(1).unwrap();
    let deps = deps.split("\n[").next().unwrap();
    assert!(
        !deps.contains("mini-treasury"),
        "the production custody path must not link the hand-rolled treasury signer"
    );
}

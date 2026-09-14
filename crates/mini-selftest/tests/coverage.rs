//! The coverage table must describe the workspace that actually exists.
//!
//! Without this, the table is a claim that decays silently: someone adds a
//! crate, nobody classifies it, and a user reading "everything is covered"
//! is misled by a document that was true once. With it, adding a crate
//! without deciding whether a user can exercise it fails CI.

use mini_selftest::coverage::{Coverage, COVERAGE};
use std::collections::BTreeSet;

/// Workspace members, read from the real `Cargo.toml`.
fn workspace_members() -> BTreeSet<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("the crate sits two levels under the workspace root")
        .join("Cargo.toml");
    let text = std::fs::read_to_string(&root)
        .unwrap_or_else(|error| panic!("reading {}: {error}", root.display()));
    let members = text
        .split_once("members = [")
        .expect("the workspace table has a members list")
        .1
        .split_once(']')
        .expect("the members list is closed")
        .0;
    members
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_end_matches(',').trim_matches('"');
            line.strip_prefix("crates/").map(str::to_string)
        })
        .collect()
}

fn classified() -> BTreeSet<String> {
    COVERAGE.iter().map(|(name, _)| name.to_string()).collect()
}

#[test]
fn every_workspace_crate_is_classified() {
    let members = workspace_members();
    let classified = classified();
    let unclassified: Vec<&String> = members.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "these crates exist but the coverage table does not mention them, so a user \
         reading the diagnostics would not know whether they can exercise them: {unclassified:?}"
    );
}

#[test]
fn the_table_does_not_describe_crates_that_no_longer_exist() {
    let members = workspace_members();
    let classified = classified();
    let stale: Vec<&String> = classified.difference(&members).collect();
    assert!(
        stale.is_empty(),
        "the coverage table lists crates that are not workspace members: {stale:?}"
    );
}

#[test]
fn no_crate_is_listed_twice() {
    let mut seen = BTreeSet::new();
    for (name, _) in COVERAGE {
        assert!(
            seen.insert(*name),
            "{name} appears twice in the coverage table"
        );
    }
    assert_eq!(seen.len(), COVERAGE.len());
}

#[test]
fn every_gap_states_a_reason_and_every_area_named_is_real() {
    for (name, coverage) in COVERAGE {
        match coverage {
            Coverage::Gap { reason } => {
                assert!(
                    reason.len() > 20,
                    "{name} is marked as a gap without a usable reason: {reason:?}"
                );
            }
            Coverage::Exercised { area } => {
                assert!(
                    mini_selftest::AREAS.contains(area),
                    "{name} claims coverage from area {area:?}, which is not one of the \
                     suite's areas {:?}",
                    mini_selftest::AREAS
                );
            }
            Coverage::Transitive { via } => assert!(!via.is_empty()),
            Coverage::SeparateBinary {
                area,
                binary,
                reason,
            } => {
                assert!(!binary.is_empty());
                assert!(reason.len() > 20, "{name}: {reason:?}");
                assert!(
                    mini_selftest::AREAS.contains(area),
                    "{name} reports under area {area:?}, which the suite does not advertise"
                );
            }
        }
    }
}

#[test]
fn every_area_the_suite_advertises_covers_at_least_one_crate() {
    for area in mini_selftest::AREAS {
        let covers = COVERAGE.iter().any(|(_, coverage)| {
            matches!(coverage, Coverage::Exercised { area: a } if a == area)
                || matches!(coverage, Coverage::SeparateBinary { area: a, .. } if a == area)
        });
        assert!(
            covers,
            "area {area} runs checks but the coverage table credits it with no crate, so \
             the two views of the suite disagree"
        );
    }
}

#[test]
fn the_summary_counts_match_the_table() {
    let summary = mini_selftest::coverage::summary();
    assert!(summary.contains(&format!("of {} crates", COVERAGE.len())));
    assert!(summary.contains(&format!("{} of", mini_selftest::coverage::runnable_count())));
}

//! The self-test suite must itself pass.
//!
//! A diagnostics feature that a user can run and that nobody checks is a
//! diagnostics feature that will one day report a confident green on a broken
//! build. So CI runs every check the client's Diagnostics page runs, against
//! the same code, and fails if any of them fails.

use mini_selftest::{run_all, run_area, Outcome, AREAS};

fn scratch(tag: &str) -> std::path::PathBuf {
    let path = mini_selftest::default_scratch().join(tag);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn every_check_in_the_suite_passes_or_explains_why_it_cannot_run() {
    let root = scratch("all");
    let report = run_all(&root);
    let failures: Vec<String> = report
        .checks
        .iter()
        .filter(|check| check.outcome.is_failure())
        .map(|check| format!("{}/{}: {}", check.area, check.name, check.outcome.detail()))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(report.is_clean());
    assert_eq!(
        report.checks.len(),
        mini_selftest::all_checks().len(),
        "the suite skipped constructing a check entirely"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_suite_is_not_only_happy_paths() {
    let root = scratch("negative");
    let report = run_all(&root);
    // Refusal checks are what establish the guarantees are load-bearing, so a
    // suite that lost them would still look green while proving much less.
    assert!(
        report.negative_checks() >= 9,
        "only {} refusal checks ran: {}",
        report.negative_checks(),
        report.summary()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn every_area_is_covered_and_addressable_on_its_own() {
    for area in AREAS {
        let root = scratch(area);
        let report = run_area(&root, area);
        assert!(
            !report.checks.is_empty(),
            "area {area} has no checks, but is advertised"
        );
        assert!(
            report.checks.iter().all(|check| check.area == *area),
            "running one area ran checks from another"
        );
        assert!(report.is_clean(), "{area}: {}", report.summary());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[test]
fn a_skip_is_reported_as_a_skip_and_never_as_a_pass() {
    let root = scratch("skips");
    let report = run_all(&root);
    for check in &report.checks {
        if let Outcome::Skipped { reason } = &check.outcome {
            assert!(
                !reason.is_empty(),
                "{}/{} skipped without saying why",
                check.area,
                check.name
            );
        }
    }
    assert_eq!(
        report.passed() + report.failed() + report.skipped(),
        report.checks.len()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_suite_leaves_nothing_behind_in_its_scratch_directory() {
    let root = scratch("cleanup");
    run_all(&root);
    let leftovers: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the suite left {leftovers:?} behind; a diagnostics run must not accumulate state"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_summary_line_reports_the_same_numbers_the_checks_do() {
    let root = scratch("summary");
    let report = run_all(&root);
    let summary = report.summary();
    assert!(summary.contains(&format!("{} passed", report.passed())));
    assert!(summary.contains(&format!("{} failed", report.failed())));
    assert!(summary.contains(&format!("{} skipped", report.skipped())));
    let _ = std::fs::remove_dir_all(root);
}

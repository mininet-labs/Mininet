//! `mini selftest` --- run the diagnostics suite from a terminal.
//!
//! The same checks the client's Diagnostics page runs, over the same
//! `mini-selftest` crate. Two front ends, one suite: a green result in one
//! place means the same thing as a green result in the other, which it would
//! not if each had its own list.
//!
//! Exit status matters here. A failed check exits non-zero, so this is usable
//! as a post-install smoke test in a script or a deployment, where a report
//! nobody reads is the normal case.

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};
use mini_selftest::{run_all, run_area, Outcome, Report, AREAS};

use std::path::Path;

/// Run every check, or only one area.
///
/// Returns the rendered result and whether the run was clean. The caller uses
/// that flag to make a failing run exit non-zero: a smoke test whose process
/// succeeds while printing FAIL lines is a smoke test that will be trusted
/// exactly once.
pub fn run(scratch: Option<&Path>, area: Option<&str>) -> Result<(CommandResult, bool)> {
    if let Some(area) = area {
        if !AREAS.contains(&area) {
            return Err(CliError::Usage(format!(
                "unknown area {area:?}; known areas are {}",
                AREAS.join(", ")
            )));
        }
    }
    let owned = match scratch {
        Some(path) => path.to_path_buf(),
        None => mini_selftest::default_scratch(),
    };
    std::fs::create_dir_all(&owned)
        .map_err(|error| CliError::Io(format!("{}: {error}", owned.display())))?;
    let report = match area {
        Some(area) => run_area(&owned, area),
        None => run_all(&owned),
    };
    // The scratch root itself is this command's to clean up; each check
    // already removes its own subdirectory.
    let _ = std::fs::remove_dir_all(&owned);
    let clean = report.is_clean();
    Ok((render(&report), clean))
}

/// List what would run, without running it.
pub fn list() -> CommandResult {
    let checks = mini_selftest::all_checks();
    let mut human = format!("{} checks across {} areas:\n", checks.len(), AREAS.len());
    for (area, name, negative, _) in &checks {
        human.push_str(&format!(
            "  [{area}]{} {name}\n",
            if *negative { " (refusal)" } else { "" }
        ));
    }
    let refusals = checks
        .iter()
        .filter(|(_, _, negative, _)| *negative)
        .count();
    human.push_str(&format!(
        "{refusals} of them check that something is refused, not that it works.\n"
    ));
    CommandResult::new(human)
        .field("checks", JsonValue::num(checks.len() as u64))
        .field("refusal_checks", JsonValue::num(refusals as u64))
        .field("areas", JsonValue::strs(AREAS.iter().copied()))
        .field(
            "names",
            JsonValue::strs(
                checks
                    .iter()
                    .map(|(area, name, _, _)| format!("{area}: {name}")),
            ),
        )
}

fn render(report: &Report) -> CommandResult {
    let mut human = String::new();
    let mut current_area = "";
    for check in &report.checks {
        if check.area != current_area {
            human.push_str(&format!("\n{}\n", check.area));
            current_area = check.area;
        }
        let mark = match &check.outcome {
            Outcome::Passed { .. } => "pass",
            Outcome::Failed { .. } => "FAIL",
            Outcome::Skipped { .. } => "skip",
        };
        human.push_str(&format!(
            "  {mark}  {}{}\n        {}\n",
            check.name,
            if check.negative { " [refusal]" } else { "" },
            check.outcome.detail()
        ));
    }
    human.push_str(&format!("\n{}\n", report.summary()));
    if !report.is_clean() {
        human.push_str("At least one check failed. This build should not be treated as working.\n");
    }
    CommandResult::new(human)
        .field("passed", JsonValue::num(report.passed() as u64))
        .field("failed", JsonValue::num(report.failed() as u64))
        .field("skipped", JsonValue::num(report.skipped() as u64))
        .field(
            "refusal_checks",
            JsonValue::num(report.negative_checks() as u64),
        )
        .field("clean", JsonValue::Bool(report.is_clean()))
        .field("elapsed_ms", JsonValue::num(report.elapsed_ms))
        .field(
            "failures",
            JsonValue::strs(
                report
                    .checks
                    .iter()
                    .filter(|check| check.outcome.is_failure())
                    .map(|check| format!("{}: {}", check.area, check.name)),
            ),
        )
        .field(
            "skips",
            JsonValue::strs(
                report
                    .checks
                    .iter()
                    .filter(|check| matches!(check.outcome, Outcome::Skipped { .. }))
                    .map(|check| format!("{}: {}", check.area, check.name)),
            ),
        )
}

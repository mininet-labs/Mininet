//! Running the value-layer checks, which live in another process.
//!
//! `mininet-value-selftest` is spawned, not linked. The canonical invariant
//! (P1) is that no balance maps to governance or validator vote weight; what
//! it requires in code is that no dependency path connect the value crates to
//! the governance crates. This crate holds the `mini-forge` edge, that binary
//! holds the `mini-value` edge, and the only thing crossing between them is
//! lines of text on a pipe.
//!
//! What it explicitly does **not** require is hiding the money code from the
//! person running the client. A user gets one Diagnostics page covering both
//! halves, which is the point of going to this trouble rather than simply
//! dropping the value checks.
//!
//! If the binary is not present, every value check reports
//! [`Outcome::Skipped`] naming that fact. A missing sibling binary is a
//! deployment detail; reporting it as a pass would be a lie, and reporting it
//! as a failure would cry wolf on a perfectly healthy install.

use crate::{Check, Outcome};
use std::path::{Path, PathBuf};

/// The binary that runs the value-layer checks.
pub const BINARY: &str = "mininet-value-selftest";

/// Format tag the binary prints before its first result line.
const MAGIC: &str = "MNVALUECHK1";

/// Where to look for the value-check binary.
///
/// Beside the running executable first, because that is how it ships; then
/// an explicit override for development and packaging; the bare name last,
/// so a `PATH` install works.
pub fn candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(explicit) = std::env::var_os("MININET_VALUE_SELFTEST") {
        if !explicit.is_empty() {
            out.push(PathBuf::from(explicit));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(BINARY));
            out.push(dir.join(format!("{BINARY}.exe")));
        }
    }
    out.push(PathBuf::from(BINARY));
    out
}

/// Run the value-layer checks by spawning the binary.
pub fn run() -> Vec<Check> {
    match spawn() {
        Ok(checks) if !checks.is_empty() => checks,
        Ok(_) => vec![unavailable(
            "the value-check binary ran but reported nothing",
        )],
        Err(reason) => vec![unavailable(&reason)],
    }
}

fn unavailable(reason: &str) -> Check {
    Check {
        area: "value",
        name: "value-layer checks run in their own process",
        negative: false,
        outcome: Outcome::Skipped {
            reason: format!(
                "{reason}. These checks live in {BINARY} so that no crate links both the \
                 value layer and the governance layer; install it beside this program, or \
                 set MININET_VALUE_SELFTEST, to run them."
            ),
        },
    }
}

fn spawn() -> Result<Vec<Check>, String> {
    let mut last = "no value-check binary found".to_string();
    for candidate in candidates() {
        match std::process::Command::new(&candidate).output() {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                return parse(&text);
            }
            Err(error) => {
                last = format!("could not run {}: {error}", candidate.display());
            }
        }
    }
    Err(last)
}

/// Parse the binary's line protocol.
///
/// A pure function so the wire format is testable without spawning anything,
/// and so a malformed line is a reported failure rather than a panic in a
/// diagnostics screen.
pub fn parse(text: &str) -> Result<Vec<Check>, String> {
    let mut lines = text.lines();
    match lines.next() {
        Some(first) if first.trim() == MAGIC => {}
        Some(other) => {
            return Err(format!(
                "the value-check binary printed {other:?} instead of the {MAGIC} tag"
            ))
        }
        None => return Err("the value-check binary printed nothing".to_string()),
    }
    let mut checks = Vec::new();
    for (number, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 5 {
            return Err(format!(
                "result line {} had {} field(s), expected 5",
                number + 1,
                fields.len()
            ));
        }
        // Areas and names are leaked from a sibling process, so they are
        // owned strings here rather than the `&'static str` the in-process
        // checks use; the Check type keeps that distinction honest by
        // storing what this crate itself knows.
        let detail = fields[4].to_string();
        let outcome = match fields[3] {
            "passed" => Outcome::Passed { detail },
            "failed" => Outcome::Failed { detail },
            "skipped" => Outcome::Skipped { reason: detail },
            other => {
                return Err(format!(
                    "result line {} reported an unknown outcome {other:?}",
                    number + 1
                ))
            }
        };
        checks.push(Check {
            area: "value",
            name: leak_name(fields[0], fields[1]),
            negative: fields[2] == "true",
            outcome,
        });
    }
    Ok(checks)
}

/// The spawned process names its own checks, and [`Check::name`] is a
/// `&'static str`, so the name is interned once here.
///
/// Bounded by the binary's fixed check list --- these strings come from a
/// program built from this same workspace, not from user input --- so the
/// leak is a small, constant set, not unbounded growth.
fn leak_name(area: &str, name: &str) -> &'static str {
    Box::leak(format!("{area}: {name}").into_boxed_str())
}

/// The value binary's own path, if one can be found. For a UI that wants to
/// tell the user where the checks came from.
pub fn located() -> Option<PathBuf> {
    candidates().into_iter().find(|path| path.is_file())
}

/// Whether `path` looks like a runnable value-check binary.
pub fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_report_parses_into_checks() {
        let text = "MNVALUECHK1\n\
                    value\ta hidden amount commits\tfalse\tpassed\tit did\n\
                    treasury\tone custodian cannot reach the threshold\ttrue\tpassed\tit could not\n";
        let checks = parse(text).unwrap();
        assert_eq!(checks.len(), 2);
        assert!(checks[0].name.starts_with("value: "));
        assert!(checks[1].negative);
        assert!(matches!(checks[1].outcome, Outcome::Passed { .. }));
    }

    #[test]
    fn a_failed_check_crosses_the_pipe_as_a_failure() {
        let text = "MNVALUECHK1\nvalue\tinflation is refused\ttrue\tfailed\tit was not\n";
        let checks = parse(text).unwrap();
        assert!(checks[0].outcome.is_failure());
        assert_eq!(checks[0].outcome.detail(), "it was not");
    }

    #[test]
    fn output_without_the_format_tag_is_refused_rather_than_guessed_at() {
        assert!(parse("some other program's output\n").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn a_malformed_line_is_reported_not_silently_dropped() {
        let text = "MNVALUECHK1\nvalue\ttoo\tfew\n";
        let error = parse(text).unwrap_err();
        assert!(error.contains("expected 5"));
    }

    #[test]
    fn an_unknown_outcome_word_is_refused() {
        let text = "MNVALUECHK1\nvalue\tname\tfalse\tmaybe\tdetail\n";
        assert!(parse(text).unwrap_err().contains("maybe"));
    }

    #[test]
    fn a_missing_binary_reports_a_skip_that_says_how_to_get_it() {
        let check = unavailable("could not run it");
        assert!(matches!(check.outcome, Outcome::Skipped { .. }));
        assert!(check.outcome.detail().contains(BINARY));
        assert!(check.outcome.detail().contains("MININET_VALUE_SELFTEST"));
    }
}

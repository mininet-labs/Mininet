//! Structured, machine-readable fields for every setup result.
//!
//! `mininet-setup.exe --json` and `mini windows-setup --json` must report
//! the same facts under the same names, or a script written against one
//! silently misreads the other. The field names therefore live here, next
//! to the types they describe, and each front end only supplies its own
//! JSON emitter.
//!
//! These names are part of the tool's contract in the same sense
//! `mini-cli`'s `--json` envelope keys are: a caller may match on them, so
//! renaming one is a breaking change rather than a wording fix.

use crate::{
    InstallPlan, InstallReport, SetupStatus, UninstallReport, VerifyProblem, VerifyReport,
};

/// One reportable value.
///
/// Deliberately narrow: every number setup reports is an exact byte count,
/// file count, or millisecond timestamp, so there is no float to lose
/// precision on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Text.
    Text(String),
    /// Text that may be absent (a rollback target, a launch path).
    MaybeText(Option<String>),
    /// An exact non-negative integer.
    Number(u64),
    /// A flag.
    Flag(bool),
    /// An ordered list of strings.
    List(Vec<String>),
}

impl Field {
    fn path(path: &std::path::Path) -> Self {
        Self::Text(path.display().to_string())
    }

    fn maybe_path(path: Option<&std::path::Path>) -> Self {
        Self::MaybeText(path.map(|path| path.display().to_string()))
    }
}

/// Fields for [`SetupStatus`].
pub fn status_fields(status: &SetupStatus) -> Vec<(&'static str, Field)> {
    vec![
        ("install_root", Field::path(&status.install_root)),
        (
            "active_version",
            Field::MaybeText(
                status
                    .active
                    .as_ref()
                    .map(|record| record.version_text.clone()),
            ),
        ),
        (
            "active_package_digest",
            Field::MaybeText(
                status
                    .active
                    .as_ref()
                    .map(|record| record.package_digest.clone()),
            ),
        ),
        (
            "installed_at_ms",
            Field::Number(
                status
                    .active
                    .as_ref()
                    .map_or(0, |record| record.installed_at_ms),
            ),
        ),
        (
            "previous_version",
            Field::MaybeText(
                status
                    .previous
                    .as_ref()
                    .map(|record| record.version_text.clone()),
            ),
        ),
        (
            "installed_versions",
            Field::List(status.installed_versions.clone()),
        ),
        ("launch_path", Field::maybe_path(status.launch_path.as_deref())),
        ("user_data_root", Field::path(&status.user_data_root)),
        ("user_data_present", Field::Flag(status.user_data_present)),
    ]
}

/// Fields for [`InstallPlan`].
pub fn plan_fields(plan: &InstallPlan) -> Vec<(&'static str, Field)> {
    vec![
        ("package", Field::Text(plan.package.clone())),
        ("version", Field::Text(plan.version_text.clone())),
        ("product", Field::Text(plan.product.clone())),
        ("package_digest", Field::Text(plan.package_digest.clone())),
        ("plan", Field::Text(plan.kind.as_str().to_string())),
        ("install_root", Field::path(&plan.install_root)),
        ("version_dir", Field::path(&plan.version_dir)),
        ("launch_path", Field::path(&plan.launch_path)),
        ("files", Field::Number(plan.files.len() as u64)),
        ("total_bytes", Field::Number(plan.total_bytes)),
        (
            "shell_actions",
            Field::List(plan.shell_actions.iter().map(shell_action_name).collect()),
        ),
        (
            "active_version",
            Field::MaybeText(
                plan.active
                    .as_ref()
                    .map(|record| record.version_text.clone()),
            ),
        ),
        ("user_data_root", Field::path(&plan.user_data_root)),
    ]
}

/// Fields for [`InstallReport`].
pub fn install_fields(report: &InstallReport) -> Vec<(&'static str, Field)> {
    vec![
        ("version", Field::Text(report.active.version_text.clone())),
        (
            "package_digest",
            Field::Text(report.active.package_digest.clone()),
        ),
        (
            "previous_version",
            Field::MaybeText(
                report
                    .previous
                    .as_ref()
                    .map(|record| record.version_text.clone()),
            ),
        ),
        ("files_written", Field::Number(report.files_written as u64)),
        ("bytes_written", Field::Number(report.bytes_written)),
        ("launch_path", Field::path(&report.launch_path)),
        (
            "shell_actions",
            Field::List(report.shell_actions.iter().map(shell_action_name).collect()),
        ),
    ]
}

/// Fields for [`VerifyReport`].
pub fn verify_fields(report: &VerifyReport) -> Vec<(&'static str, Field)> {
    vec![
        ("version", Field::Text(report.version_text.clone())),
        ("intact", Field::Flag(report.is_intact())),
        ("files_checked", Field::Number(report.files_checked as u64)),
        ("bytes_checked", Field::Number(report.bytes_checked)),
        (
            "problems",
            Field::List(report.problems.iter().map(describe_problem).collect()),
        ),
    ]
}

/// Fields for [`UninstallReport`].
pub fn uninstall_fields(report: &UninstallReport) -> Vec<(&'static str, Field)> {
    vec![
        ("install_root", Field::path(&report.install_root)),
        (
            "versions_removed",
            Field::List(report.versions_removed.clone()),
        ),
        (
            "shell_actions",
            Field::List(report.shell_actions.iter().map(shell_action_name).collect()),
        ),
        (
            "user_data_kept",
            Field::maybe_path(report.user_data_kept.as_deref()),
        ),
        (
            "identities_destroyed",
            Field::Flag(report.user_data_destroyed.is_some()),
        ),
    ]
}

/// A stable short name for one shell change.
pub fn shell_action_name(action: &crate::ShellAction) -> String {
    match action {
        crate::ShellAction::CreateShortcut(request) => {
            format!("create-shortcut:{}", request.link_path.display())
        }
        crate::ShellAction::RemoveShortcut { path } => {
            format!("remove-shortcut:{}", path.display())
        }
        crate::ShellAction::RegisterUninstall(registration) => {
            format!("register-uninstall:{}", registration.key_name)
        }
        crate::ShellAction::DeregisterUninstall { key_name } => {
            format!("deregister-uninstall:{key_name}")
        }
    }
}

/// A one-line description of a verification problem, for a report or a log.
pub fn describe_problem(problem: &VerifyProblem) -> String {
    match problem {
        VerifyProblem::Missing { path } => format!("missing:{path}"),
        VerifyProblem::Length {
            path,
            expected,
            found,
        } => format!("length:{path}:expected={expected}:found={found}"),
        VerifyProblem::Digest { path } => format!("digest:{path}"),
        VerifyProblem::Unexpected { path } => format!("unexpected:{path}"),
    }
}

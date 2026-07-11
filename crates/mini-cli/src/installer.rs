//! `mini installer ...` — drive the real `mini_installer::Installer`
//! type-state pipeline one step at a time across separate CLI
//! invocations.
//!
//! Each `mini installer <step>` invocation is a fresh process: it cannot
//! hold a Rust value like `StagedRelease`/`PreflightPassed`/
//! `ActivationRecord` in memory the way an in-process caller (this crate's
//! own end-to-end test) can. `mini_installer::Installer::staged_release`/
//! `preflight_passed`/`activation_record` close that gap by reconstructing
//! the minimal typed value from the installer's own disk state and
//! persisted event log, so the process boundary does not weaken the
//! pipeline's guarantees: each reconstruction method fails cleanly
//! ([`mini_installer::InstallerError::WrongState`]) unless the log's own
//! record shows the release genuinely completed the previous step.
//!
//! **Boundary rule (unchanged from `mini-installer`'s own docs):** the
//! event log this module reads is durable evidence, never permission --
//! `activate` below still constructs a real, release-id-scoped
//! `OwnerApproval` itself; nothing here lets a stale or forged log entry
//! stand in for that.

use std::path::Path;

use mini_installer::{verify_install_event_log, HealthCheckOutcome, Installer, OwnerApproval};
use mini_objects::ObjectId;

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};

fn open(device_root: &Path) -> Result<Installer> {
    Installer::new(device_root).map_err(|e| CliError::Installer(e.to_string()))
}

fn parse_release(release_ref: &str) -> Result<ObjectId> {
    ObjectId::parse(release_ref).map_err(|e| CliError::Object(e.to_string()))
}

/// `mini installer stage --device-root <dir> --store <path> --release <id>
/// --project <p> --branch <b> [--min-attestations N] [--timelock-ms N]
/// [--now-ms N] --timestamp-ms N`
///
/// Independently re-verifies governance/attestation/timelock trust the
/// same way `mini release verify` does (this crate never re-derives that
/// trust from the release object alone -- see `mini-installer`'s module
/// docs) before handing the result to `Installer::stage`.
#[allow(clippy::too_many_arguments)]
pub fn stage(
    home: &Path,
    store_path: &Path,
    device_root: &Path,
    release_ref: &str,
    project_ref: &str,
    branch: &str,
    min_attestations: Option<u32>,
    timelock_ms: Option<u64>,
    now_ms: Option<u64>,
    timestamp_ms: u64,
) -> Result<CommandResult> {
    let verified = crate::release::verified_release(
        home,
        store_path,
        release_ref,
        project_ref,
        branch,
        min_attestations,
        timelock_ms,
        now_ms,
    )?;
    let store = crate::store::open_store(store_path)?;
    let installer = open(device_root)?;
    let staged = installer
        .stage(&store, &verified, timestamp_ms)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    Ok(CommandResult::new(format!(
        "staged: release {} version {:?} ({} bytes) at {:?}",
        staged.release_id.as_str(),
        staged.version,
        staged.len,
        staged.path
    ))
    .field("release_id", JsonValue::str(staged.release_id.as_str()))
    .field("version", JsonValue::str(&staged.version))
    .field("digest", JsonValue::str(hex(&staged.digest)))
    .field("len", JsonValue::num(staged.len))
    .field("path", JsonValue::str(staged.path.to_string_lossy())))
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `mini installer preflight --device-root <dir> --release <id>
/// --timestamp-ms N`
pub fn preflight(
    device_root: &Path,
    release_ref: &str,
    timestamp_ms: u64,
) -> Result<CommandResult> {
    let release_id = parse_release(release_ref)?;
    let installer = open(device_root)?;
    let staged = installer
        .staged_release(&release_id)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let passed = installer
        .preflight(&staged, timestamp_ms)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    Ok(CommandResult::new(format!(
        "preflight passed: release {} version {:?} -- awaiting owner approval",
        passed.release_id.as_str(),
        passed.version
    ))
    .field("release_id", JsonValue::str(passed.release_id.as_str()))
    .field("version", JsonValue::str(&passed.version)))
}

/// `mini installer activate --device-root <dir> --release <id>
/// --approved-at-ms N`
///
/// Invoking this command *is* the explicit device-owner action: it
/// constructs a fresh `OwnerApproval` naming exactly this release id right
/// here, never reads one from the log or anywhere else (the typed-domain
/// rule -- see `mini-installer`'s module docs on "no forced update").
pub fn activate(
    device_root: &Path,
    release_ref: &str,
    approved_at_ms: u64,
) -> Result<CommandResult> {
    let release_id = parse_release(release_ref)?;
    let installer = open(device_root)?;
    let passed = installer
        .preflight_passed(&release_id)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let approval = OwnerApproval::new(release_id, approved_at_ms);
    let activation = installer
        .activate(&passed, &approval)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let previous_str = activation
        .previous
        .as_ref()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "none".to_string());
    Ok(CommandResult::new(format!(
        "activated: release {} version {:?} (previous: {previous_str})",
        activation.release_id.as_str(),
        activation.version,
    ))
    .field("release_id", JsonValue::str(activation.release_id.as_str()))
    .field("version", JsonValue::str(&activation.version))
    .field(
        "previous",
        JsonValue::opt_str(activation.previous.as_ref().map(|p| p.as_str().to_string())),
    ))
}

/// `mini installer health-check --device-root <dir> --release <id>
/// (--healthy|--unhealthy) --timestamp-ms N`
///
/// `healthy` is the caller's own verdict (a smoke test, a port check --
/// whatever "healthy" means for the installed software, this crate cannot
/// know, per `mini-installer`'s module docs), passed in already decided
/// rather than this command deciding it.
pub fn health_check(
    device_root: &Path,
    release_ref: &str,
    healthy: bool,
    timestamp_ms: u64,
) -> Result<CommandResult> {
    let release_id = parse_release(release_ref)?;
    let installer = open(device_root)?;
    let activation = installer
        .activation_record(&release_id)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let outcome = installer
        .health_check(activation, || healthy, timestamp_ms)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    Ok(match outcome {
        HealthCheckOutcome::Active(id) => CommandResult::new(format!(
            "healthy: release {} stays active",
            id.as_str()
        ))
        .field("outcome", JsonValue::str("active"))
        .field("release_id", JsonValue::str(id.as_str())),
        HealthCheckOutcome::RolledBack { failed, restored } => CommandResult::new(format!(
            "unhealthy: release {} failed, rolled back to {}",
            failed.as_str(),
            restored.as_str()
        ))
        .field("outcome", JsonValue::str("rolled_back"))
        .field("failed_release_id", JsonValue::str(failed.as_str()))
        .field("restored_release_id", JsonValue::str(restored.as_str())),
        HealthCheckOutcome::FailedWithNoPriorRelease { failed } => CommandResult::new(format!(
            "unhealthy: release {} failed with no prior release to restore -- device left with nothing active",
            failed.as_str()
        ))
        .field("outcome", JsonValue::str("failed_no_prior_release"))
        .field("failed_release_id", JsonValue::str(failed.as_str())),
        other => {
            return Err(CliError::Installer(format!(
                "unrecognized health check outcome variant: {other:?}"
            )))
        }
    })
}

/// `mini installer rollback --device-root <dir> --timestamp-ms N`
///
/// A standalone, caller-initiated rollback -- distinct from the automatic
/// rollback `health-check` triggers on failure, and recorded as such in
/// the event log (`mini-installer`'s `UnexplainedRollback` check).
pub fn rollback(device_root: &Path, timestamp_ms: u64) -> Result<CommandResult> {
    let installer = open(device_root)?;
    let restored = installer
        .rollback(timestamp_ms)
        .map_err(|e| CliError::Installer(e.to_string()))?;
    Ok(CommandResult::new(format!(
        "rolled back: restored release {}",
        restored.as_str()
    ))
    .field("restored_release_id", JsonValue::str(restored.as_str())))
}

/// `mini installer status --device-root <dir>`
pub fn status(device_root: &Path) -> Result<CommandResult> {
    let installer = open(device_root)?;
    let current = installer
        .current()
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let human = match &current {
        Some(id) => format!("active release: {}", id.as_str()),
        None => "no release currently active".to_string(),
    };
    Ok(CommandResult::new(human).field(
        "active_release_id",
        JsonValue::opt_str(current.as_ref().map(|id| id.as_str().to_string())),
    ))
}

/// `mini installer history --device-root <dir> [--release <id>]`
pub fn history(device_root: &Path, release_ref: Option<&str>) -> Result<CommandResult> {
    let installer = open(device_root)?;
    let events = installer
        .event_log()
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let filter = release_ref.map(parse_release).transpose()?;
    let mut out = String::new();
    let mut event_fields = Vec::new();
    for event in &events {
        if let Some(want) = &filter {
            if &event.release_id != want {
                continue;
            }
        }
        out.push_str(&format!(
            "{:>4}  {:?}  release {}\n",
            event.sequence,
            event.kind,
            event.release_id.as_str()
        ));
        event_fields.push(JsonValue::Object(vec![
            ("sequence".to_string(), JsonValue::num(event.sequence)),
            (
                "kind".to_string(),
                JsonValue::str(format!("{:?}", event.kind)),
            ),
            (
                "release_id".to_string(),
                JsonValue::str(event.release_id.as_str()),
            ),
        ]));
    }
    if out.is_empty() {
        out.push_str("no matching events\n");
    }
    Ok(CommandResult::new(out).field("events", JsonValue::Array(event_fields)))
}

/// `mini installer verify-log --device-root <dir>` -- run
/// [`verify_install_event_log`] against the real persisted log and report
/// pass/fail, never itself gating any installer action (see this module's
/// docs on the boundary rule).
pub fn verify_log(device_root: &Path) -> Result<CommandResult> {
    let installer = open(device_root)?;
    let events = installer
        .event_log()
        .map_err(|e| CliError::Installer(e.to_string()))?;
    let history = verify_install_event_log(&events)
        .map_err(|e| CliError::Installer(format!("event log failed verification: {e}")))?;
    let mut releases: Vec<String> = Vec::new();
    for event in &events {
        let id = event.release_id.as_str().to_string();
        if !releases.contains(&id) {
            releases.push(id);
        }
    }
    Ok(CommandResult::new(format!(
        "event log verified clean: {} event(s) across {} release(s)",
        history.events.len(),
        releases.len()
    ))
    .field("event_count", JsonValue::num(history.events.len() as u64))
    .field("release_count", JsonValue::num(releases.len() as u64)))
}

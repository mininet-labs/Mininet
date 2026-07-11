//! Batch 4 of the self-hosted forge spine (D-0066/D-0071,
//! `docs/design/self-hosted-forge-spine.md`): the audit's most
//! safety-critical, most honestly-named gap. `mini_update::AdoptionState::
//! adopt` verifies a release and records a decision, but nothing in that
//! crate executes, fetches, or installs anything -- deliberately, since it
//! stays policy/intent only (the no-forced-update/no-kill-path freeze,
//! `docs/INVARIANTS.md` U1). This crate is the separate, real installation
//! layer the design doc calls for: it takes a `mini_forge::VerifiedRelease`
//! (already verified by `mini-update`/`mini-forge` -- this crate never
//! re-derives governance/attestation/timelock trust, only acts on it) and
//! actually reads the artifact bytes out of the store, stages them on disk,
//! and -- only once the device owner explicitly says so -- atomically
//! activates them, with automatic rollback on a failed health check.
//!
//! ## State machine
//!
//! `Discovered -> Verified -> Downloading -> Staged -> PreflightPassed ->
//! AwaitingOwnerApproval -> Activating -> HealthChecking -> Active` or
//! `RolledBack`, exactly as named in the design doc. `Verified` is already
//! true of the `VerifiedRelease` this crate is handed (mini-update's job);
//! `Downloading` and `Staged` happen together in one real disk write
//! ([`Installer::stage`]); `AwaitingOwnerApproval` is not a blocking call --
//! this crate never waits on anything, mirroring `mini-update`'s stance --
//! it is simply the gap between [`Installer::preflight`] returning and the
//! caller choosing to construct an [`OwnerApproval`] and call
//! [`Installer::activate`]. Each stage's return type is required as the
//! next stage's input type (a type-state pipeline), so the sequence is
//! enforced by the compiler, not just documented; each returned value also
//! carries the named [`InstallState`] for logging/observability.
//!
//! [`event_log::InstallEventKind`]'s persisted names diverge slightly from
//! the design doc's phrasing above, honestly: there is no separate
//! `Downloading` event (folded into `Staged`, since this crate performs
//! both in one disk write), and `HealthChecking`/`Active` become three
//! finer-grained events (`HealthCheckStarted`, then `HealthCheckPassed`
//! or `HealthCheckFailed`) so a caller reading the log back can tell a
//! health check that never finished from one that failed.
//!
//! ## Persisted event log (D-0076)
//!
//! Every step above also appends a record to a durable, hash-chained,
//! append-only log ([`event_log`], re-exported as [`InstallEvent`]/
//! [`InstallEventKind`]/[`verify_install_event_log`]) -- queryable after
//! any process exit, independent of the in-process type-state values
//! above. **Boundary rule:** the log is evidence of what this crate did;
//! it is never permission to do anything. [`Installer::activate`] still
//! refuses to run without a real, matching [`OwnerApproval`] regardless
//! of what the log says -- nothing reads the log to make an install
//! decision, only to record one already made.
//!
//! ## No forced update, still [FREEZE]
//!
//! [`Installer::activate`] requires a caller-constructed [`OwnerApproval`]
//! naming the exact release id it authorizes (the typed-domain rule:
//! authority-exercising functions take a specific named request type, never
//! a generic "approve"/"go ahead"). This crate never constructs one itself
//! and never calls `activate` on a timer, on startup, or in response to
//! anything other than an explicit caller decision -- the actual "no forced
//! update" guarantee is procedural (this crate's own code never
//! self-invokes activation), the same honest limit `mini-update`'s docs
//! already state about `adopt()`. A failed health check triggers an
//! *automatic rollback to whatever was already running* -- returning to a
//! known-good prior state is the opposite of forcing new software, and is
//! itself a device-owner-in-control safety property, not a policy weakening.
//!
//! ## Honest limits
//!
//! - **Unix-only.** Activation is a `symlink`/`rename` swap
//!   (`std::os::unix::fs::symlink`); no Windows support exists yet.
//! - **No process supervision.** This crate stages files and flips a
//!   pointer. It does not start, stop, restart, or supervise any process --
//!   the caller's own health-check predicate ([`Installer::health_check`])
//!   is where "is the newly activated release actually running correctly"
//!   gets answered, since this crate cannot know what that means for
//!   arbitrary software (the same caller-supplied-policy pattern as
//!   `mini_update::FreshnessPolicy`).
//! - **No real package manager / OS integration.** "Activation" here means
//!   the `current` symlink under an installer-owned directory points at the
//!   newly staged release's directory. Wiring that into an actual running
//!   system (restarting a service, swapping a binary on `PATH`, etc.) is
//!   the caller's job, layered on top of this crate's atomic pointer flip.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod event_log;

pub use error::InstallerError;
pub use event_log::{
    verify_install_event_log, EventHash, InstallEvent, InstallEventKind, InstallLogError,
    VerifiedInstallHistory,
};

use std::fs;
use std::path::{Path, PathBuf};

use mini_crypto::HashAlgorithm;
use mini_forge::VerifiedRelease;
use mini_objects::ObjectId;
use mini_store::{Backend, Store};

const CURRENT_LINK: &str = "current";
const PREVIOUS_MARKER: &str = "previous";
const STAGED_DIR: &str = "staged";
const ARTIFACT_FILE: &str = "artifact";
const EVENT_LOG_FILE: &str = "event_log.bin";

/// The named state a value in this crate's pipeline represents -- exactly
/// the sequence the design doc calls for, carried alongside each typed
/// return value for logging/observability (the compiler already enforces
/// the sequence via each stage's input/output types; this is not itself
/// what does the enforcing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallState {
    Staged,
    PreflightPassed,
    Active,
    RolledBack,
}

/// A release's artifact bytes, fetched from the store and written to a real
/// local staging directory. Produced only by [`Installer::stage`], which
/// re-verifies the digest independently of `mini-media`'s own internal
/// check.
#[derive(Debug, Clone)]
pub struct StagedRelease {
    pub release_id: ObjectId,
    pub version: String,
    pub digest: [u8; 32],
    pub len: u64,
    pub path: PathBuf,
    pub state: InstallState,
}

/// A staged release whose on-disk bytes were re-read and re-verified
/// immediately before activation -- catches staging-directory corruption or
/// tampering between [`Installer::stage`] and [`Installer::activate`].
#[derive(Debug, Clone)]
pub struct PreflightPassed {
    pub release_id: ObjectId,
    pub version: String,
    pub state: InstallState,
}

/// Explicit device-owner authorization to activate one specific staged
/// release. Typed rather than a generic "approve" call (CLAUDE.md's
/// typed-domain rule): an `OwnerApproval` names exactly which `release_id`
/// it authorizes, and [`Installer::activate`] refuses to use it for any
/// other release id, so the authority it grants cannot be silently widened
/// by whatever bytes happen to be lying around.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerApproval {
    pub release_id: ObjectId,
    pub approved_at_ms: u64,
}

impl OwnerApproval {
    pub fn new(release_id: ObjectId, approved_at_ms: u64) -> Self {
        OwnerApproval {
            release_id,
            approved_at_ms,
        }
    }
}

/// The result of a successful [`Installer::activate`] call: which release
/// is now active, and what was active immediately before (`None` if this
/// was the first-ever activation), for [`Installer::rollback`] to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationRecord {
    pub release_id: ObjectId,
    pub version: String,
    pub previous: Option<ObjectId>,
    pub state: InstallState,
}

/// The result of [`Installer::health_check`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HealthCheckOutcome {
    /// The health predicate passed; the release stays active.
    Active(ObjectId),
    /// The health predicate failed and a prior release was restored.
    RolledBack {
        failed: ObjectId,
        restored: ObjectId,
    },
    /// The health predicate failed on the very first activation -- there
    /// was nothing to roll back to, so the device is left with no active
    /// release rather than a corrupted or unhealthy one.
    FailedWithNoPriorRelease { failed: ObjectId },
}

/// A real, local, on-disk installation area rooted at one directory. Every
/// method reads/writes real files -- there is no in-memory state to get out
/// of sync with a process restart, unlike `mini_update::AdoptionState`
/// (which is deliberately in-memory bookkeeping one layer up).
#[derive(Debug, Clone)]
pub struct Installer {
    root: PathBuf,
}

impl Installer {
    /// Open (creating if necessary) an installer rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, InstallerError> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        fs::create_dir_all(root.join(STAGED_DIR))?;
        Ok(Installer { root })
    }

    fn staged_dir(&self, release_id: &ObjectId) -> PathBuf {
        self.root.join(STAGED_DIR).join(release_id.as_str())
    }

    fn current_link(&self) -> PathBuf {
        self.root.join(CURRENT_LINK)
    }

    fn previous_marker(&self) -> PathBuf {
        self.root.join(PREVIOUS_MARKER)
    }

    fn event_log_path(&self) -> PathBuf {
        self.root.join(EVENT_LOG_FILE)
    }

    /// The full persisted event log, in order -- durable evidence of what
    /// this installer has done, queryable after any process exit. Pass the
    /// result to [`verify_install_event_log`] before trusting it as
    /// anything more than telemetry.
    pub fn event_log(&self) -> Result<Vec<InstallEvent>, InstallerError> {
        Ok(event_log::read_events(&self.event_log_path())?)
    }

    /// The most recently recorded version string for `release_id`, found
    /// by scanning this installer's own event log -- used to recover
    /// `from_version` for a release this installer only ever knew by id
    /// (e.g. what was active before the release currently being
    /// activated), without needing a side-channel version-lookup service.
    fn version_of(&self, release_id: &ObjectId) -> Result<Option<String>, InstallerError> {
        let events = self.event_log()?;
        Ok(events
            .iter()
            .rev()
            .find(|e| &e.release_id == release_id && e.to_version.is_some())
            .and_then(|e| e.to_version.clone()))
    }

    /// Append one event to the persisted log, deriving `sequence` and
    /// `previous_event_hash` from the log's current tail. Re-reads the log
    /// on every call rather than caching a counter in memory, matching
    /// this crate's existing "no in-memory state to get out of sync with a
    /// process restart" design -- acceptable given a device's lifetime
    /// event count is small, not a hot path.
    #[allow(clippy::too_many_arguments)]
    fn append_event(
        &self,
        kind: InstallEventKind,
        release_id: ObjectId,
        artifact_digest: Option<[u8; 32]>,
        from_version: Option<String>,
        to_version: Option<String>,
        reason: Option<String>,
        timestamp_ms: u64,
    ) -> Result<InstallEvent, InstallerError> {
        let existing = self.event_log()?;
        let sequence = existing.len() as u64;
        let previous_event_hash = existing.last().map(|e| e.event_hash);
        let event = InstallEvent::new(
            sequence,
            previous_event_hash,
            kind,
            release_id,
            artifact_digest,
            from_version,
            to_version,
            reason,
            timestamp_ms,
        );
        event_log::append_event(&self.event_log_path(), &event).map_err(InstallLogError::Io)?;
        Ok(event)
    }

    /// The release id `current` points at, if anything has ever been
    /// activated. Reads the real filesystem, never cached state.
    pub fn current(&self) -> Result<Option<ObjectId>, InstallerError> {
        let link = self.current_link();
        match fs::read_link(&link) {
            Ok(target) => {
                let name = target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or(InstallerError::CorruptCurrentLink)?;
                let id = ObjectId::parse(name).map_err(|_| InstallerError::CorruptCurrentLink)?;
                Ok(Some(id))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Fetch `verified`'s artifact bytes from `store` (real chunk
    /// reassembly, `mini_media::assemble`) and write them to a real local
    /// staging directory. Re-verifies the digest independently of
    /// `mini-media`'s own internal check, since trusting a single crate's
    /// internal verification for a step this security-sensitive is exactly
    /// the kind of shortcut this tree's culture rejects. Records
    /// `Discovered`/`Verified`/`Staged` events in the persisted log --
    /// `Verified` here means "received an already-verified
    /// `VerifiedRelease` and is recording that fact," not that this crate
    /// re-derived governance/attestation trust (it never does; see the
    /// module docs).
    pub fn stage<B: Backend>(
        &self,
        store: &Store<B>,
        verified: &VerifiedRelease,
        timestamp_ms: u64,
    ) -> Result<StagedRelease, InstallerError> {
        self.append_event(
            InstallEventKind::Discovered,
            verified.id.clone(),
            Some(verified.artifact.digest),
            None,
            Some(verified.version.clone()),
            None,
            timestamp_ms,
        )?;
        self.append_event(
            InstallEventKind::Verified,
            verified.id.clone(),
            Some(verified.artifact.digest),
            None,
            Some(verified.version.clone()),
            None,
            timestamp_ms,
        )?;

        let bytes = mini_media::assemble(store, &verified.artifact)?;
        let digest = HashAlgorithm::Blake3.digest(&bytes);
        if digest != verified.artifact.digest {
            return Err(InstallerError::DigestMismatch);
        }
        let dir = self.staged_dir(&verified.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(ARTIFACT_FILE);
        fs::write(&path, &bytes)?;

        self.append_event(
            InstallEventKind::Staged,
            verified.id.clone(),
            Some(digest),
            None,
            Some(verified.version.clone()),
            None,
            timestamp_ms,
        )?;

        Ok(StagedRelease {
            release_id: verified.id.clone(),
            version: verified.version.clone(),
            digest,
            len: bytes.len() as u64,
            path,
            state: InstallState::Staged,
        })
    }

    /// Re-read and re-verify a staged release's on-disk bytes immediately
    /// before activation. Catches corruption or tampering of the staging
    /// directory between [`Self::stage`] and [`Self::activate`] -- a real
    /// check against real bytes on disk, not a re-derivation of trust
    /// already established by `stage`/`mini-forge`/`mini-update`. Records
    /// `PreflightPassed` then `AwaitingOwnerApproval` -- the latter is not
    /// a blocking wait, just the log recording that the pipeline is now in
    /// that gap, exactly as this crate's own docs already describe it.
    pub fn preflight(
        &self,
        staged: &StagedRelease,
        timestamp_ms: u64,
    ) -> Result<PreflightPassed, InstallerError> {
        let on_disk = fs::read(&staged.path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                InstallerError::StagedArtifactMissing
            } else {
                InstallerError::Io(e)
            }
        })?;
        if on_disk.len() as u64 != staged.len {
            return Err(InstallerError::DigestMismatch);
        }
        let digest = HashAlgorithm::Blake3.digest(&on_disk);
        if digest != staged.digest {
            return Err(InstallerError::DigestMismatch);
        }

        self.append_event(
            InstallEventKind::PreflightPassed,
            staged.release_id.clone(),
            Some(digest),
            None,
            Some(staged.version.clone()),
            None,
            timestamp_ms,
        )?;
        self.append_event(
            InstallEventKind::AwaitingOwnerApproval,
            staged.release_id.clone(),
            Some(digest),
            None,
            Some(staged.version.clone()),
            None,
            timestamp_ms,
        )?;

        Ok(PreflightPassed {
            release_id: staged.release_id.clone(),
            version: staged.version.clone(),
            state: InstallState::PreflightPassed,
        })
    }

    /// Atomically flip the `current` pointer to `passed`'s staged
    /// directory, but only if `approval` explicitly names that exact
    /// release id. Records whatever was active before (if anything) so
    /// [`Self::rollback`] can restore it later, including across a process
    /// restart (the record is a real file, not in-memory state). Records
    /// `OwnerApproved` then `Activating` -- using `approval.approved_at_ms`
    /// as both events' timestamp, since this crate never waits between
    /// approval and activation.
    ///
    /// **Boundary rule:** the event log records that an approval was
    /// presented; it never substitutes for one. This function still
    /// refuses to activate anything without a real, matching
    /// [`OwnerApproval`] -- nothing about the log changes that.
    pub fn activate(
        &self,
        passed: &PreflightPassed,
        approval: &OwnerApproval,
    ) -> Result<ActivationRecord, InstallerError> {
        if approval.release_id != passed.release_id {
            return Err(InstallerError::ApprovalMismatch {
                approved: approval.release_id.clone(),
                staged: passed.release_id.clone(),
            });
        }
        let target = self.staged_dir(&passed.release_id);
        if !target.is_dir() {
            return Err(InstallerError::StagedArtifactMissing);
        }
        let previous = self.current()?;
        let from_version = match &previous {
            Some(prev) => self.version_of(prev)?,
            None => None,
        };

        self.append_event(
            InstallEventKind::OwnerApproved,
            passed.release_id.clone(),
            None,
            from_version.clone(),
            Some(passed.version.clone()),
            None,
            approval.approved_at_ms,
        )?;

        self.swap_current(&target)?;
        match &previous {
            Some(prev) => fs::write(self.previous_marker(), prev.as_str())?,
            None => remove_if_present(&self.previous_marker())?,
        }

        self.append_event(
            InstallEventKind::Activating,
            passed.release_id.clone(),
            None,
            from_version,
            Some(passed.version.clone()),
            None,
            approval.approved_at_ms,
        )?;

        Ok(ActivationRecord {
            release_id: passed.release_id.clone(),
            version: passed.version.clone(),
            previous,
            state: InstallState::Active,
        })
    }

    /// Run the caller's health predicate against the newly activated
    /// release. This crate cannot know what "healthy" means for arbitrary
    /// installed software -- the same caller-supplied-policy pattern as
    /// `mini_update::FreshnessPolicy` -- so `healthy` is entirely the
    /// caller's own check (a port answering, a process alive, a smoke
    /// test passing). On failure, automatically rolls back to whatever was
    /// active before, never forward.
    pub fn health_check(
        &self,
        activation: ActivationRecord,
        healthy: impl FnOnce() -> bool,
        timestamp_ms: u64,
    ) -> Result<HealthCheckOutcome, InstallerError> {
        self.append_event(
            InstallEventKind::HealthCheckStarted,
            activation.release_id.clone(),
            None,
            None,
            Some(activation.version.clone()),
            None,
            timestamp_ms,
        )?;

        if healthy() {
            self.append_event(
                InstallEventKind::HealthCheckPassed,
                activation.release_id.clone(),
                None,
                None,
                Some(activation.version.clone()),
                None,
                timestamp_ms,
            )?;
            return Ok(HealthCheckOutcome::Active(activation.release_id));
        }

        self.append_event(
            InstallEventKind::HealthCheckFailed,
            activation.release_id.clone(),
            None,
            None,
            Some(activation.version.clone()),
            None,
            timestamp_ms,
        )?;

        match self.rollback_internal(timestamp_ms, None) {
            Ok(restored) => Ok(HealthCheckOutcome::RolledBack {
                failed: activation.release_id,
                restored,
            }),
            Err(InstallerError::NoPriorActivation) => {
                // Nothing to fall back to -- never leave a known-unhealthy
                // release silently marked `current` just because there was
                // no prior release to restore instead.
                remove_if_present(&self.current_link())?;
                self.append_event(
                    InstallEventKind::FailedWithNoPriorRelease,
                    activation.release_id.clone(),
                    None,
                    None,
                    Some(activation.version.clone()),
                    None,
                    timestamp_ms,
                )?;
                Ok(HealthCheckOutcome::FailedWithNoPriorRelease {
                    failed: activation.release_id,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Restore whatever release was active immediately before the most
    /// recent [`Self::activate`] call. Errors cleanly
    /// ([`InstallerError::NoPriorActivation`]) if there is nothing to roll
    /// back to, rather than silently doing nothing. Consumes the recorded
    /// "previous" pointer on success, so a second rollback call in a row
    /// fails cleanly instead of toggling back and forth between two
    /// releases.
    ///
    /// Called directly (not via a failed [`Self::health_check`]), this
    /// records a `reason` explaining the rollback wasn't health-check-
    /// triggered -- [`verify_install_event_log`] treats a `RollbackStarted`
    /// event with neither a preceding `HealthCheckFailed` nor a `reason`
    /// as unexplained, not as evidence.
    pub fn rollback(&self, timestamp_ms: u64) -> Result<ObjectId, InstallerError> {
        self.rollback_internal(
            timestamp_ms,
            Some("manual rollback (Installer::rollback called directly)".to_string()),
        )
    }

    fn rollback_internal(
        &self,
        timestamp_ms: u64,
        reason: Option<String>,
    ) -> Result<ObjectId, InstallerError> {
        let failed = self.current()?.ok_or(InstallerError::NoPriorActivation)?;
        let marker = self.previous_marker();
        let raw = fs::read_to_string(&marker).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                InstallerError::NoPriorActivation
            } else {
                InstallerError::Io(e)
            }
        })?;
        let prev = ObjectId::parse(raw.trim()).map_err(|_| InstallerError::CorruptCurrentLink)?;
        let target = self.staged_dir(&prev);
        if !target.is_dir() {
            return Err(InstallerError::StagedArtifactMissing);
        }

        let failed_version = self.version_of(&failed)?;
        let restored_version = self.version_of(&prev)?;

        self.append_event(
            InstallEventKind::RollbackStarted,
            failed.clone(),
            None,
            failed_version.clone(),
            restored_version.clone(),
            reason,
            timestamp_ms,
        )?;

        self.swap_current(&target)?;
        remove_if_present(&marker)?;

        self.append_event(
            InstallEventKind::RolledBack,
            failed.clone(),
            None,
            failed_version,
            restored_version.clone(),
            None,
            timestamp_ms,
        )?;
        self.append_event(
            InstallEventKind::PreviousReleaseActive,
            prev.clone(),
            None,
            None,
            restored_version,
            None,
            timestamp_ms,
        )?;

        Ok(prev)
    }

    /// Atomic pointer flip: build a symlink under a fresh temp name, then
    /// `rename` it over `current`. `rename` within one directory is atomic
    /// on every Unix filesystem this crate targets, so `current` is never
    /// observably missing or half-written.
    fn swap_current(&self, target: &Path) -> Result<(), InstallerError> {
        let tmp = self
            .root
            .join(format!("current.tmp.{}", std::process::id()));
        remove_if_present(&tmp)?;
        std::os::unix::fs::symlink(target, &tmp)?;
        fs::rename(&tmp, self.current_link())?;
        Ok(())
    }
}

fn remove_if_present(path: &Path) -> Result<(), InstallerError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

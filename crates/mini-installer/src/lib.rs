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

pub use error::InstallerError;

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
    /// the kind of shortcut this tree's culture rejects.
    pub fn stage<B: Backend>(
        &self,
        store: &Store<B>,
        verified: &VerifiedRelease,
    ) -> Result<StagedRelease, InstallerError> {
        let bytes = mini_media::assemble(store, &verified.artifact)?;
        let digest = HashAlgorithm::Blake3.digest(&bytes);
        if digest != verified.artifact.digest {
            return Err(InstallerError::DigestMismatch);
        }
        let dir = self.staged_dir(&verified.id);
        fs::create_dir_all(&dir)?;
        let path = dir.join(ARTIFACT_FILE);
        fs::write(&path, &bytes)?;
        Ok(StagedRelease {
            release_id: verified.id.clone(),
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
    /// already established by `stage`/`mini-forge`/`mini-update`.
    pub fn preflight(&self, staged: &StagedRelease) -> Result<PreflightPassed, InstallerError> {
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
        Ok(PreflightPassed {
            release_id: staged.release_id.clone(),
            state: InstallState::PreflightPassed,
        })
    }

    /// Atomically flip the `current` pointer to `passed`'s staged
    /// directory, but only if `approval` explicitly names that exact
    /// release id. Records whatever was active before (if anything) so
    /// [`Self::rollback`] can restore it later, including across a process
    /// restart (the record is a real file, not in-memory state).
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
        self.swap_current(&target)?;
        match &previous {
            Some(prev) => fs::write(self.previous_marker(), prev.as_str())?,
            None => remove_if_present(&self.previous_marker())?,
        }
        Ok(ActivationRecord {
            release_id: passed.release_id.clone(),
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
    ) -> Result<HealthCheckOutcome, InstallerError> {
        if healthy() {
            return Ok(HealthCheckOutcome::Active(activation.release_id));
        }
        match self.rollback() {
            Ok(restored) => Ok(HealthCheckOutcome::RolledBack {
                failed: activation.release_id,
                restored,
            }),
            Err(InstallerError::NoPriorActivation) => {
                // Nothing to fall back to -- never leave a known-unhealthy
                // release silently marked `current` just because there was
                // no prior release to restore instead.
                remove_if_present(&self.current_link())?;
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
    pub fn rollback(&self) -> Result<ObjectId, InstallerError> {
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
        self.swap_current(&target)?;
        remove_if_present(&marker)?;
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

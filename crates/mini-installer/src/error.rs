use mini_objects::ObjectId;

#[derive(Debug)]
#[non_exhaustive]
pub enum InstallerError {
    Io(std::io::Error),
    Media(mini_media::MediaError),
    /// Re-verified artifact bytes did not match the expected digest --
    /// either at staging time (bytes assembled from the store don't match
    /// what the release claims) or at preflight time (the staged file on
    /// disk was corrupted or tampered with after staging).
    DigestMismatch,
    /// [`crate::Installer::activate`] was called with an
    /// [`crate::OwnerApproval`] naming a different release than the one
    /// that passed preflight.
    ApprovalMismatch {
        approved: ObjectId,
        staged: ObjectId,
    },
    /// A staged release's directory is missing on disk when it was
    /// expected to exist (activation or rollback target).
    StagedArtifactMissing,
    /// [`crate::Installer::rollback`] was called with nothing recorded to
    /// roll back to.
    NoPriorActivation,
    /// The `current` symlink (or the `previous` marker) exists but does
    /// not point at / name a well-formed object id.
    CorruptCurrentLink,
    /// Appending to or reading back the persisted event log failed --
    /// distinct from every error above, since those are about the
    /// install *action* failing, while this is about the durable record
    /// of actions failing (see `crate::event_log`'s module docs on why
    /// that's a separate concern).
    Log(crate::InstallLogError),
    /// A cross-process reconstruction method
    /// ([`crate::Installer::staged_release`],
    /// [`crate::Installer::preflight_passed`],
    /// [`crate::Installer::activation_record`]) found no event at all for
    /// the given release id -- there is nothing on disk to reconstruct.
    NoSuchRelease(ObjectId),
    /// A reconstruction method found events for the release id, but its
    /// most recent recorded state is not the one that method reconstructs
    /// -- e.g. asking for a [`crate::StagedRelease`] when the log's last
    /// event for that release is already `PreflightPassed`.
    WrongState {
        release_id: ObjectId,
        expected: crate::InstallEventKind,
        found: crate::InstallEventKind,
    },
    /// [`crate::Installer::activation_record`] was asked to reconstruct an
    /// [`crate::ActivationRecord`] for a release id that is not what
    /// `current` currently points at.
    NotCurrentlyActive(ObjectId),
}

impl From<std::io::Error> for InstallerError {
    fn from(e: std::io::Error) -> Self {
        InstallerError::Io(e)
    }
}

impl From<mini_media::MediaError> for InstallerError {
    fn from(e: mini_media::MediaError) -> Self {
        InstallerError::Media(e)
    }
}

impl From<crate::InstallLogError> for InstallerError {
    fn from(e: crate::InstallLogError) -> Self {
        InstallerError::Log(e)
    }
}

impl core::fmt::Display for InstallerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InstallerError::Io(e) => write!(f, "installer I/O error: {e}"),
            InstallerError::Media(e) => write!(f, "installer media error: {e}"),
            InstallerError::DigestMismatch => write!(f, "artifact digest mismatch"),
            InstallerError::ApprovalMismatch { approved, staged } => write!(
                f,
                "owner approval names release {} but staged release is {}",
                approved.as_str(),
                staged.as_str()
            ),
            InstallerError::StagedArtifactMissing => write!(f, "staged artifact missing on disk"),
            InstallerError::NoPriorActivation => {
                write!(f, "no prior activation to roll back to")
            }
            InstallerError::CorruptCurrentLink => {
                write!(f, "current/previous pointer is corrupt")
            }
            InstallerError::Log(e) => write!(f, "installer event log error: {e}"),
            InstallerError::NoSuchRelease(id) => {
                write!(f, "no recorded events for release {}", id.as_str())
            }
            InstallerError::WrongState {
                release_id,
                expected,
                found,
            } => write!(
                f,
                "release {} is in state {found:?}, expected {expected:?}",
                release_id.as_str()
            ),
            InstallerError::NotCurrentlyActive(id) => {
                write!(
                    f,
                    "release {} is not the currently active release",
                    id.as_str()
                )
            }
        }
    }
}

impl std::error::Error for InstallerError {}

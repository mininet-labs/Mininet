//! Errors this crate's coordination functions can return.

use mini_settlement::SettlementError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ContributionError>;

/// Why a contribution-coordination operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContributionError {
    /// A reward split's basis points did not sum to 10,000 (100.00%).
    InvalidSplit,
    /// The delivery verdict's content id does not match the engagement's
    /// terms (the manifest being delivered).
    ContentIdMismatch,
    /// The verdict's host root is not this engagement's performer.
    HostRoleMismatch,
    /// The verdict's witness root is not this engagement's payer.
    WitnessRoleMismatch,
    /// Settlement can only be built from a locally completed engagement.
    EngagementNotCompleted,
    /// Too many payee shares to assign distinct sequence numbers starting
    /// from the caller-supplied `start_sequence` without overflow.
    SequenceOverflow,
    /// Building one of the split payment claims failed.
    ClaimSigning(SettlementError),
}

impl core::fmt::Display for ContributionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ContributionError::InvalidSplit => {
                write!(f, "reward split basis points must sum to 10,000")
            }
            ContributionError::ContentIdMismatch => write!(
                f,
                "delivery verdict content id does not match the engagement's terms"
            ),
            ContributionError::HostRoleMismatch => write!(
                f,
                "delivery verdict host is not this engagement's performer"
            ),
            ContributionError::WitnessRoleMismatch => {
                write!(f, "delivery verdict witness is not this engagement's payer")
            }
            ContributionError::EngagementNotCompleted => {
                write!(f, "engagement has not locally reached Completed")
            }
            ContributionError::SequenceOverflow => {
                write!(f, "too many payee shares for the supplied start sequence")
            }
            ContributionError::ClaimSigning(e) => write!(f, "claim signing failed: {e}"),
        }
    }
}

impl std::error::Error for ContributionError {}

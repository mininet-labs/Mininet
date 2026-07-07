//! Error types for `mini-chain`.

use did_mini::IdentityError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ChainError>;

/// Why a chain operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChainError {
    /// A validator set had no members.
    EmptyValidatorSet,
    /// A validator set named the same identity root more than once.
    DuplicateValidator,
    /// A vote's claimed device/root did not match the KEL supplied for it.
    DeviceMismatch,
    /// The voting device is not delegated with `Capabilities::VOTE`.
    MissingVoteCapability,
    /// A vote's transcript did not match its `kind`/`height`/`round`/`block_hash`.
    Malformed,
    /// Too few distinct, currently-valid validator votes to reach quorum.
    QuorumNotMet {
        /// Distinct validator roots required (`> 2/3` of the set).
        needed: usize,
        /// Distinct validator roots actually counted.
        got: usize,
    },
    /// Identity/KEL verification failure.
    Identity(IdentityError),
}

impl core::fmt::Display for ChainError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ChainError::EmptyValidatorSet => write!(f, "validator set is empty"),
            ChainError::DuplicateValidator => {
                write!(f, "validator set names the same identity root twice")
            }
            ChainError::DeviceMismatch => write!(f, "vote does not match the supplied KELs"),
            ChainError::MissingVoteCapability => {
                write!(f, "voting device is not delegated with VOTE capability")
            }
            ChainError::Malformed => write!(f, "vote transcript is malformed"),
            ChainError::QuorumNotMet { needed, got } => {
                write!(f, "quorum not met: needed {needed}, got {got}")
            }
            ChainError::Identity(e) => write!(f, "identity: {e}"),
        }
    }
}
impl std::error::Error for ChainError {}
impl From<IdentityError> for ChainError {
    fn from(e: IdentityError) -> Self {
        ChainError::Identity(e)
    }
}

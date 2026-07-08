//! Errors for vouch verification and graph/confidence computation.

use did_mini::IdentityError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, UniquenessError>;

/// Why a vouch attestation was rejected, or why a graph/confidence
/// computation could not proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UniquenessError {
    /// The attestation used an unsupported vouch protocol version.
    UnsupportedVersion(u8),
    /// A party's device identifier does not match the KEL supplied for it.
    DeviceMismatch,
    /// A party's KEL digest does not match the KEL supplied for it.
    KelDigestMismatch,
    /// A device lacks the `ATTEST` capability from its identity root.
    MissingAttestCapability,
    /// A nonce was reused (or the two parties reused each other's nonce): replay.
    Replay,
    /// Both devices belong to the same identity root: an identity root
    /// cannot vouch for itself.
    SelfVouch,
    /// An underlying identity/delegation/signature failure.
    Identity(IdentityError),
}

impl core::fmt::Display for UniquenessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            UniquenessError::UnsupportedVersion(v) => {
                write!(f, "unsupported vouch version {v}")
            }
            UniquenessError::DeviceMismatch => {
                write!(f, "device identifier does not match its KEL")
            }
            UniquenessError::KelDigestMismatch => write!(f, "KEL digest does not match its KEL"),
            UniquenessError::MissingAttestCapability => {
                write!(f, "device lacks the ATTEST capability")
            }
            UniquenessError::Replay => write!(f, "nonce reuse (replay) detected"),
            UniquenessError::SelfVouch => {
                write!(f, "both devices belong to the same identity root")
            }
            UniquenessError::Identity(e) => write!(f, "identity error: {e}"),
        }
    }
}

impl std::error::Error for UniquenessError {}

impl From<IdentityError> for UniquenessError {
    fn from(e: IdentityError) -> Self {
        UniquenessError::Identity(e)
    }
}

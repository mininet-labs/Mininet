//! Errors for storage-served receipt verification.

use did_mini::IdentityError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, StorageProofError>;

/// Why a storage-served receipt was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageProofError {
    /// The receipt used an unsupported protocol version.
    UnsupportedVersion(u8),
    /// A receipt claimed zero bytes served — not evidence of anything.
    ZeroBytes,
    /// The claimed timestamp is outside the verifier's freshness policy.
    TooOld,
    /// A party's device identifier does not match the KEL supplied for it.
    DeviceMismatch,
    /// A device lacks the `ATTEST` capability from its identity root.
    MissingAttestCapability,
    /// A nonce was reused (or the two parties reused each other's nonce): replay.
    Replay,
    /// Both devices belong to the same identity root: a host cannot witness
    /// (and be rewarded for) its own storage.
    SelfServe,
    /// An underlying identity/delegation/signature failure.
    Identity(IdentityError),
}

impl core::fmt::Display for StorageProofError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StorageProofError::UnsupportedVersion(v) => {
                write!(f, "unsupported storage-receipt version {v}")
            }
            StorageProofError::ZeroBytes => write!(f, "receipt claims zero bytes served"),
            StorageProofError::TooOld => write!(f, "receipt is outside the freshness policy"),
            StorageProofError::DeviceMismatch => {
                write!(f, "device identifier does not match its KEL")
            }
            StorageProofError::MissingAttestCapability => {
                write!(f, "device lacks the ATTEST capability")
            }
            StorageProofError::Replay => write!(f, "nonce reuse (replay) detected"),
            StorageProofError::SelfServe => {
                write!(f, "both devices belong to the same identity root")
            }
            StorageProofError::Identity(e) => write!(f, "identity error: {e}"),
        }
    }
}

impl std::error::Error for StorageProofError {}

impl From<IdentityError> for StorageProofError {
    fn from(e: IdentityError) -> Self {
        StorageProofError::Identity(e)
    }
}

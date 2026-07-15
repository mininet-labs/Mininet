//! Errors for the private-lookup-boundary crate.

use did_mini::IdentityError;
use mini_crypto::CryptoError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, IndexError>;

/// Why a lookup-label derivation, private-index-record, or local-index
/// operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IndexError {
    /// Bytes ended before a declared length.
    Truncated,
    /// Bytes remained after a complete decode.
    TrailingBytes,
    /// A declared count or length exceeded a hard decode limit.
    LimitExceeded,
    /// An unrecognized [`crate::LookupPurpose`] tag.
    BadLookupPurpose,
    /// An unrecognized [`crate::RecordSizeClass`] tag.
    BadSizeClass,
    /// The private index record's format version is not recognized.
    UnsupportedRecordVersion,
    /// An encrypted descriptor exceeded its declared
    /// [`crate::RecordSizeClass`]'s byte budget.
    RecordExceedsSizeClass,
    /// The record's writer signature does not verify against the
    /// supplied KEL.
    BadSignature,
    /// The record's `expires_at_ms` has already passed.
    Expired,
    /// A write to [`crate::LocalIndex`] named a different writer than the
    /// record already stored at that lookup label — rejected rather than
    /// silently allowing a hijack of someone else's label.
    WriterMismatch,
    /// A write to [`crate::LocalIndex`] did not strictly increase
    /// `sequence` over the record already stored at that lookup label —
    /// rejected as a rollback/replay attempt.
    RollbackRejected,
    /// An identity/delegation/signature failure.
    Identity(IdentityError),
    /// A cryptographic primitive failure.
    Crypto(CryptoError),
}

impl core::fmt::Display for IndexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IndexError::Truncated => write!(f, "private-index bytes truncated"),
            IndexError::TrailingBytes => write!(f, "trailing bytes after private-index structure"),
            IndexError::LimitExceeded => write!(f, "decode limit exceeded"),
            IndexError::BadLookupPurpose => write!(f, "unrecognized lookup purpose tag"),
            IndexError::BadSizeClass => write!(f, "unrecognized record size class tag"),
            IndexError::UnsupportedRecordVersion => {
                write!(f, "unsupported or unrecognized private-index record version")
            }
            IndexError::RecordExceedsSizeClass => {
                write!(f, "encrypted descriptor exceeds its declared size class")
            }
            IndexError::BadSignature => {
                write!(f, "private-index record signature does not verify")
            }
            IndexError::Expired => write!(f, "private-index record has expired"),
            IndexError::WriterMismatch => write!(
                f,
                "write named a different writer than the record already at this label"
            ),
            IndexError::RollbackRejected => write!(
                f,
                "write did not strictly increase sequence over the stored record"
            ),
            IndexError::Identity(e) => write!(f, "identity error: {e}"),
            IndexError::Crypto(e) => write!(f, "crypto error: {e}"),
        }
    }
}

impl std::error::Error for IndexError {}

impl From<IdentityError> for IndexError {
    fn from(e: IdentityError) -> Self {
        IndexError::Identity(e)
    }
}
impl From<CryptoError> for IndexError {
    fn from(e: CryptoError) -> Self {
        IndexError::Crypto(e)
    }
}

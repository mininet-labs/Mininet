//! Errors for the intake-vocabulary crate.

use mini_crypto::CryptoError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, IntakeError>;

/// Why an intake type's decode or state transition failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntakeError {
    /// Bytes ended before a declared length.
    Truncated,
    /// Bytes remained after a complete decode.
    TrailingBytes,
    /// A declared count or length exceeded a hard decode limit.
    LimitExceeded,
    /// The envelope's format version is not recognized.
    UnsupportedVersion,
    /// An unrecognized [`crate::MediaType`] tag.
    BadMediaType,
    /// An unrecognized [`crate::RepresentationKind`] tag.
    BadRepresentationKind,
    /// An unrecognized [`crate::AuthorityClass`] tag.
    BadAuthorityClass,
    /// An unrecognized [`crate::ReviewState`] tag.
    BadReviewState,
    /// An unrecognized [`crate::IntakeLink`] tag.
    BadIntakeLink,
    /// A requested [`crate::ReviewState`] transition is not permitted
    /// from the envelope's current state — see
    /// [`crate::IntakeEnvelope::advance_review_state`].
    InvalidReviewTransition,
    /// A requested [`crate::AuthorityClass`] promotion is not permitted
    /// from the envelope's current class — see
    /// [`crate::IntakeEnvelope::promote_authority`].
    InvalidAuthorityPromotion,
    /// A cryptographic primitive failure (surfaced by [`mini_crypto::Multihash`]
    /// decoding).
    Crypto(CryptoError),
}

impl core::fmt::Display for IntakeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IntakeError::Truncated => write!(f, "intake bytes truncated"),
            IntakeError::TrailingBytes => write!(f, "trailing bytes after intake structure"),
            IntakeError::LimitExceeded => write!(f, "decode limit exceeded"),
            IntakeError::UnsupportedVersion => {
                write!(f, "unsupported or unrecognized intake envelope version")
            }
            IntakeError::BadMediaType => write!(f, "unrecognized media type tag"),
            IntakeError::BadRepresentationKind => {
                write!(f, "unrecognized representation kind tag")
            }
            IntakeError::BadAuthorityClass => write!(f, "unrecognized authority class tag"),
            IntakeError::BadReviewState => write!(f, "unrecognized review state tag"),
            IntakeError::BadIntakeLink => write!(f, "unrecognized intake link tag"),
            IntakeError::InvalidReviewTransition => {
                write!(f, "requested review-state transition is not permitted")
            }
            IntakeError::InvalidAuthorityPromotion => {
                write!(f, "requested authority-class promotion is not permitted")
            }
            IntakeError::Crypto(e) => write!(f, "crypto error: {e}"),
        }
    }
}

impl std::error::Error for IntakeError {}

impl From<CryptoError> for IntakeError {
    fn from(e: CryptoError) -> Self {
        IntakeError::Crypto(e)
    }
}

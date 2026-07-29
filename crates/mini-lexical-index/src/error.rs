//! Errors for index construction, serialization, and decode.

use mini_crypto::CryptoError;

pub type Result<T> = core::result::Result<T, LexicalIndexError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexicalIndexError {
    /// Ran out of bytes mid-structure while decoding a segment.
    Truncated,
    /// Bytes remained after a complete segment was decoded.
    TrailingBytes,
    /// A declared length or count exceeded this crate's cap.
    LimitExceeded,
    /// A length-prefixed string was not valid UTF-8.
    NotUtf8,
    /// Unrecognized field tag.
    BadField,
    /// A decoded segment was not in canonical form — its terms or
    /// documents were not in the sorted order this crate always emits.
    /// Rejecting non-canonical encodings keeps the segment↔bytes mapping
    /// one-to-one, so a segment's content address is well defined.
    NotCanonical,
    /// A decoded segment's declared format version is not one this build
    /// understands.
    UnsupportedVersion,
    /// Underlying digest could not be parsed.
    Crypto(CryptoError),
}

impl From<CryptoError> for LexicalIndexError {
    fn from(e: CryptoError) -> Self {
        LexicalIndexError::Crypto(e)
    }
}

impl core::fmt::Display for LexicalIndexError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LexicalIndexError::Truncated => write!(f, "index segment bytes truncated"),
            LexicalIndexError::TrailingBytes => {
                write!(f, "trailing bytes after index segment")
            }
            LexicalIndexError::LimitExceeded => write!(f, "decode limit exceeded"),
            LexicalIndexError::NotUtf8 => write!(f, "string field was not valid UTF-8"),
            LexicalIndexError::BadField => write!(f, "unrecognized field tag"),
            LexicalIndexError::NotCanonical => {
                write!(f, "index segment bytes were not in canonical sorted order")
            }
            LexicalIndexError::UnsupportedVersion => {
                write!(f, "unsupported index segment format version")
            }
            LexicalIndexError::Crypto(e) => write!(f, "digest error: {e}"),
        }
    }
}

impl std::error::Error for LexicalIndexError {}

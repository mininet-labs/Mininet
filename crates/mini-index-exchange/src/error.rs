//! Errors for publishing, encoding, and verifying segment publications.
//!
//! The verification failures are kept distinct on purpose: a caller acting
//! on an untrusted publication must be able to tell *why* it was rejected —
//! a forged signature, a segment whose bytes do not match the published
//! content address, and a malformed frame are three different threats, and
//! collapsing them would hide which one occurred.

use mini_crypto::CryptoError;
use mini_lexical_index::LexicalIndexError;

pub type Result<T> = core::result::Result<T, ExchangeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExchangeError {
    /// Ran out of bytes mid-structure while decoding.
    Truncated,
    /// Bytes remained after a complete structure was decoded.
    TrailingBytes,
    /// A declared length exceeded this crate's cap.
    LimitExceeded,
    /// The publisher's signature did not verify over the published manifest.
    BadSignature,
    /// The segment's re-derived content address did not equal the published
    /// `segment_id`: the bytes are not the segment that was published.
    SegmentIdMismatch,
    /// The segment's actual shape (document/term counts, format version) did
    /// not match the published manifest, even though the id matched — a
    /// malformed or inconsistent manifest.
    ManifestMismatch,
    /// The decoded segment bytes were not a valid index segment.
    Segment(LexicalIndexError),
    /// A key or signature could not be parsed, or an unknown suite tag.
    Crypto(CryptoError),
}

impl From<LexicalIndexError> for ExchangeError {
    fn from(e: LexicalIndexError) -> Self {
        ExchangeError::Segment(e)
    }
}

impl From<CryptoError> for ExchangeError {
    fn from(e: CryptoError) -> Self {
        ExchangeError::Crypto(e)
    }
}

impl core::fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ExchangeError::Truncated => write!(f, "publication bytes truncated"),
            ExchangeError::TrailingBytes => write!(f, "trailing bytes after publication"),
            ExchangeError::LimitExceeded => write!(f, "decode limit exceeded"),
            ExchangeError::BadSignature => {
                write!(f, "publisher signature did not verify over the manifest")
            }
            ExchangeError::SegmentIdMismatch => {
                write!(
                    f,
                    "segment bytes do not match the published content address"
                )
            }
            ExchangeError::ManifestMismatch => {
                write!(f, "segment shape does not match the published manifest")
            }
            ExchangeError::Segment(e) => write!(f, "invalid index segment: {e}"),
            ExchangeError::Crypto(e) => write!(f, "crypto error: {e}"),
        }
    }
}

impl std::error::Error for ExchangeError {}

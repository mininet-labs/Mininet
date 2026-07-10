//! Error type for `mini-provenance`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    /// A provenance record claimed zero output digests -- a build that
    /// produced nothing is not evidence of anything.
    NoOutputs,
    /// `reproducibility_group` was empty or exceeded
    /// [`crate::MAX_GROUP_BYTES`].
    BadGroup,
    /// `finished_ms` was before `started_ms`.
    BadTimeRange,
    /// The stored object was not a well-formed provenance record.
    BadObject,
    /// Store failure.
    Store(mini_store::StoreError),
    /// Object build/parse failure.
    Object(mini_objects::ObjectError),
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProvenanceError::NoOutputs => write!(f, "provenance record has no output digests"),
            ProvenanceError::BadGroup => write!(f, "invalid reproducibility group"),
            ProvenanceError::BadTimeRange => write!(f, "finished_ms is before started_ms"),
            ProvenanceError::BadObject => write!(f, "malformed provenance object"),
            ProvenanceError::Store(e) => write!(f, "store: {e}"),
            ProvenanceError::Object(e) => write!(f, "object: {e}"),
        }
    }
}

impl std::error::Error for ProvenanceError {}

impl From<mini_store::StoreError> for ProvenanceError {
    fn from(e: mini_store::StoreError) -> Self {
        ProvenanceError::Store(e)
    }
}

impl From<mini_objects::ObjectError> for ProvenanceError {
    fn from(e: mini_objects::ObjectError) -> Self {
        ProvenanceError::Object(e)
    }
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ProvenanceError>;

//! Query crate errors.

use mini_ranker::RankerError;

pub type Result<T> = core::result::Result<T, QueryError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum QueryError {
    /// The underlying ranker pass failed.
    Ranker(RankerError),
    /// A ranked result's document has no entry in the [`crate::DocumentContextTable`],
    /// so no source observation can be attached to it. Like
    /// `RankerError::MissingDocumentMetadata`, this is a caller wiring gap
    /// (the context table should cover every document the corpus does)
    /// surfaced rather than papered over with a placeholder observation id.
    MissingDocumentContext,
}

impl From<RankerError> for QueryError {
    fn from(e: RankerError) -> Self {
        QueryError::Ranker(e)
    }
}

impl core::fmt::Display for QueryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            QueryError::Ranker(e) => write!(f, "ranker: {e}"),
            QueryError::MissingDocumentContext => {
                write!(f, "ranked document has no context-table entry")
            }
        }
    }
}

impl std::error::Error for QueryError {}

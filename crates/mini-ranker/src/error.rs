//! Ranker errors.

use mini_web_types::WebTypeError;

pub type Result<T> = core::result::Result<T, RankerError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RankerError {
    /// A document referenced by the index has no metadata in the corpus, so
    /// it cannot be turned into a displayable result (no URL, title, or
    /// availability). This is a caller wiring error — the corpus should
    /// cover every document in the index — surfaced rather than silently
    /// dropped so the gap is visible.
    MissingDocumentMetadata,
    /// A weight or score fell outside the valid basis-point range while
    /// composing a `mini-web-types` value.
    Web(WebTypeError),
}

impl From<WebTypeError> for RankerError {
    fn from(e: WebTypeError) -> Self {
        RankerError::Web(e)
    }
}

impl core::fmt::Display for RankerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RankerError::MissingDocumentMetadata => {
                write!(f, "indexed document has no corpus metadata")
            }
            RankerError::Web(e) => write!(f, "web vocabulary error: {e:?}"),
        }
    }
}

impl std::error::Error for RankerError {}

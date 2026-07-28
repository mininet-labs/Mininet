/// Extraction errors. Malformed markup is not one of these — the parser is
/// deliberately lenient about tag soup (see the crate doc comment); only
/// caller-controllable resource limits are hard failures.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtractError {
    /// `html.len()` exceeded [`crate::MAX_HTML_BYTES`]. Bounding input size
    /// bounds total parse work, since the parser makes one pass over the
    /// bytes.
    InputTooLarge { byte_length: usize },
}

pub type Result<T> = std::result::Result<T, ExtractError>;

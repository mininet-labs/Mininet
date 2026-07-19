use std::fmt;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, IntakeCoordError>;

/// Why a local-intake or lookup operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum IntakeCoordError {
    /// Filesystem I/O on the source path itself (not the backend).
    Io(std::io::Error),
    /// The backend (blob storage) failed.
    Store(mini_store::StoreError),
    /// A stored or freshly built envelope failed to encode/decode.
    Types(mini_intake_types::IntakeError),
    /// The file's extension does not map to a Track B2 media type. Track B2
    /// is scoped to local text/Markdown intake only (research report §25,
    /// PR B2); anything else needs a later extractor (Track B3/B4).
    UnsupportedMediaType,
    /// The file's bytes are not valid UTF-8 text, so they cannot honestly be
    /// labeled `MediaType::TextPlain`/`MediaType::Markdown`.
    NotUtf8,
}

impl fmt::Display for IntakeCoordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IntakeCoordError::Io(e) => write!(f, "intake source I/O error: {e}"),
            IntakeCoordError::Store(e) => write!(f, "intake backend error: {e}"),
            IntakeCoordError::Types(e) => write!(f, "intake envelope error: {e}"),
            IntakeCoordError::UnsupportedMediaType => {
                write!(f, "unsupported media type for local text/Markdown intake")
            }
            IntakeCoordError::NotUtf8 => write!(f, "source bytes are not valid UTF-8 text"),
        }
    }
}

impl std::error::Error for IntakeCoordError {}

impl From<std::io::Error> for IntakeCoordError {
    fn from(value: std::io::Error) -> Self {
        IntakeCoordError::Io(value)
    }
}

impl From<mini_store::StoreError> for IntakeCoordError {
    fn from(value: mini_store::StoreError) -> Self {
        IntakeCoordError::Store(value)
    }
}

impl From<mini_intake_types::IntakeError> for IntakeCoordError {
    fn from(value: mini_intake_types::IntakeError) -> Self {
        IntakeCoordError::Types(value)
    }
}

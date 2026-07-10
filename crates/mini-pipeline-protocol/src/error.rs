//! Error type for `mini-pipeline-protocol`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// A framed message declared a length exceeding the caller's bound —
    /// refused before allocating, the same discipline every decoder in
    /// this tree applies to attacker-controlled length prefixes.
    MessageTooLarge { declared: u32, max: usize },
    /// The underlying I/O failed. `String` because `std::io::Error` isn't
    /// `PartialEq`/`Eq`, and this crate's own errors need to be.
    Io(String),
    /// A message's bytes did not decode as the expected type.
    BadMessage,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::MessageTooLarge { declared, max } => {
                write!(
                    f,
                    "framed message declares {declared} bytes, over the {max}-byte bound"
                )
            }
            ProtocolError::Io(e) => write!(f, "I/O error: {e}"),
            ProtocolError::BadMessage => write!(f, "malformed protocol message"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<std::io::Error> for ProtocolError {
    fn from(e: std::io::Error) -> Self {
        ProtocolError::Io(e.to_string())
    }
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ProtocolError>;

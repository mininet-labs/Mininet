//! Error type for `mini-net`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// The local OS CSPRNG failed while generating an ephemeral peer id.
    Entropy,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Entropy => write!(f, "failed to generate ephemeral peer id"),
        }
    }
}

impl std::error::Error for NetError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, NetError>;

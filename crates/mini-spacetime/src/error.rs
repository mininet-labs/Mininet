//! Error type for `mini-spacetime`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceTimeError {
    /// A parameter was invalid (e.g. a zero cap).
    InvalidParams,
}

impl fmt::Display for SpaceTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpaceTimeError::InvalidParams => write!(f, "invalid proposer-weight parameters"),
        }
    }
}

impl std::error::Error for SpaceTimeError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, SpaceTimeError>;

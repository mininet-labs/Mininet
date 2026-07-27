//! Error type for `mini-commons-policy`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommonsPolicyError {
    /// A wire message was truncated, carried an unknown discriminant byte,
    /// or had trailing bytes past a well-formed message.
    Malformed,
}

impl fmt::Display for CommonsPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommonsPolicyError::Malformed => {
                write!(f, "malformed commons-policy wire message")
            }
        }
    }
}

impl std::error::Error for CommonsPolicyError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, CommonsPolicyError>;

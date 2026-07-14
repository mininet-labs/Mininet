//! Error type for `mini-privacy-policy`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyPolicyError {
    /// A wire message was truncated, carried an unknown discriminant byte,
    /// or had trailing bytes past a well-formed message.
    Malformed,
    /// A property list claimed more entries than [`crate::MAX_PROPERTIES`].
    TooManyProperties,
    /// A mechanism list claimed more entries than [`crate::MAX_MECHANISMS`].
    TooManyMechanisms,
    /// A residual-floor list claimed more entries than
    /// [`crate::MAX_FLOORS`].
    TooManyFloors,
}

impl fmt::Display for PrivacyPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrivacyPolicyError::Malformed => write!(f, "malformed privacy-policy wire message"),
            PrivacyPolicyError::TooManyProperties => {
                write!(f, "property list exceeds the maximum entry count")
            }
            PrivacyPolicyError::TooManyMechanisms => {
                write!(f, "mechanism list exceeds the maximum entry count")
            }
            PrivacyPolicyError::TooManyFloors => {
                write!(f, "residual-floor list exceeds the maximum entry count")
            }
        }
    }
}

impl std::error::Error for PrivacyPolicyError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PrivacyPolicyError>;

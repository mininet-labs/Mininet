//! Error type for `mini-resource-pricing`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingError {
    /// A price computation would overflow `u64` micro-MINI. Returned
    /// rather than saturating or panicking — an overflowed quote must
    /// never be silently truncated into a *smaller*, wrong price.
    Overflow,
}

impl fmt::Display for PricingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PricingError::Overflow => write!(f, "price computation overflowed u64 micro-MINI"),
        }
    }
}

impl std::error::Error for PricingError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PricingError>;

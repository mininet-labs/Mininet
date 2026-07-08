//! Error type for `mini-value`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueError {
    /// No governed fee rate is in effect at the requested time.
    NoRateInEffect,
    /// A new rate entry's effective time was not strictly after the
    /// previous entry's.
    OutOfOrderRateEntry,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueError::NoRateInEffect => write!(f, "no governed fee rate in effect at this time"),
            ValueError::OutOfOrderRateEntry => {
                write!(f, "rate entry is not strictly after the previous one")
            }
        }
    }
}

impl std::error::Error for ValueError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ValueError>;

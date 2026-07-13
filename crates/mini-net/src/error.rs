//! Error type for `mini-net`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// The local OS CSPRNG failed while generating an ephemeral peer id.
    Entropy,
    /// A peer-exchange ([`crate::pex`]) frame was truncated, carried an
    /// unknown tag/address-kind byte, or had trailing bytes past a
    /// well-formed message.
    MalformedPex,
    /// A peer-exchange response claimed more records than
    /// [`crate::pex::MAX_PEX_RECORDS`] allows.
    TooManyPexRecords,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Entropy => write!(f, "failed to generate ephemeral peer id"),
            NetError::MalformedPex => write!(f, "malformed peer-exchange message"),
            NetError::TooManyPexRecords => {
                write!(f, "peer-exchange response exceeds the maximum record count")
            }
        }
    }
}

impl std::error::Error for NetError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, NetError>;

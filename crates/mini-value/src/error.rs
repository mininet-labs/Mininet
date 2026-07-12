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
    /// The local OS CSPRNG failed while generating cryptographic randomness.
    Entropy,
    /// A ring signature or stealth-address operation received malformed
    /// input (e.g. an empty ring, or an out-of-range real-signer index).
    InvalidInput,
    /// A governed price entry claimed `micro_mini_per_micro_cent == 0`,
    /// which would make every fee free regardless of the real-world value
    /// target — never a legitimate governed price, only ever a bug or an
    /// attempt to defeat the fee mechanism (roadmap #44's fee-manipulation
    /// finding). Rejected unconditionally; this crate has no opinion on
    /// *who* may call [`crate::fee::PriceHistory::add_entry`] (that
    /// authorization is a caller/governance concern), only on whether a
    /// zero price is ever a sane value once a call is made.
    ZeroPrice,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueError::NoRateInEffect => write!(f, "no governed fee rate in effect at this time"),
            ValueError::OutOfOrderRateEntry => {
                write!(f, "rate entry is not strictly after the previous one")
            }
            ValueError::Entropy => write!(f, "failed to generate cryptographic randomness"),
            ValueError::InvalidInput => {
                write!(f, "invalid ring signature or stealth address input")
            }
            ValueError::ZeroPrice => {
                write!(f, "a governed price of zero would make every fee free")
            }
        }
    }
}

impl std::error::Error for ValueError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ValueError>;

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
    /// A governed fee rate was zero. A zero conversion rate silently turns
    /// every positive fee target into a free action and is never valid.
    ZeroFeeRate,
    /// A fee quote cannot be represented in the ledger's `u64` amount type.
    FeeOverflow,
    /// The local OS CSPRNG failed while generating cryptographic randomness.
    Entropy,
    /// A ring signature or stealth-address operation received malformed
    /// input (e.g. an empty ring, or an out-of-range real-signer index).
    InvalidInput,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueError::NoRateInEffect => write!(f, "no governed fee rate in effect at this time"),
            ValueError::OutOfOrderRateEntry => {
                write!(f, "rate entry is not strictly after the previous one")
            }
            ValueError::ZeroFeeRate => write!(f, "governed fee rate must be non-zero"),
            ValueError::FeeOverflow => write!(f, "fee quote exceeds the ledger amount range"),
            ValueError::Entropy => write!(f, "failed to generate cryptographic randomness"),
            ValueError::InvalidInput => {
                write!(f, "invalid ring signature or stealth address input")
            }
        }
    }
}

impl std::error::Error for ValueError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ValueError>;

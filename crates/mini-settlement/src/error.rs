//! Errors for `mini-settlement`.

use core::fmt;

/// Errors constructing, verifying, or reconciling a payment claim.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SettlementError {
    /// A claim's signature did not verify against its claimed payer key.
    BadSignature,
    /// A payer/payee key was not a well-formed key for its suite.
    BadKey,
    /// An amount of zero was rejected — a claim must move real value.
    ZeroAmount,
    /// `valid_until_ms` was not strictly after the claim's construction
    /// time, or was zero — every claim must have a real, bounded window.
    BadValidityWindow,
    /// This claim conflicts with a different claim already observed for
    /// the same `(payer, nonce)` pair — see [`crate::ClaimWatcher`].
    ConflictsWithKnownClaim,
}

impl fmt::Display for SettlementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettlementError::BadSignature => write!(f, "claim signature does not verify"),
            SettlementError::BadKey => write!(f, "malformed payer or payee key"),
            SettlementError::ZeroAmount => write!(f, "claim amount must be nonzero"),
            SettlementError::BadValidityWindow => {
                write!(
                    f,
                    "claim validity window is zero or already expired at signing time"
                )
            }
            SettlementError::ConflictsWithKnownClaim => write!(
                f,
                "a different claim was already observed for this (payer, nonce) pair"
            ),
        }
    }
}

impl std::error::Error for SettlementError {}

/// Convenience result type for this crate.
pub type Result<T> = core::result::Result<T, SettlementError>;

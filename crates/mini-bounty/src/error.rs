//! Error type for `mini-bounty`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BountyError {
    /// A pool was constructed with no grants, or a grant's claim public
    /// key was not a valid 32-byte compressed point.
    InvalidPool,
    /// A claim's key image had already been recorded against this pool —
    /// the same grant cannot pay out twice.
    AlreadyClaimed,
    /// The ring signature scheme returned `None` or `false`: no real
    /// scheme is wired in, or the claim does not prove membership in the
    /// pool's grant set for the exact (pool, payout address) it claims.
    InvalidClaim,
}

impl fmt::Display for BountyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BountyError::InvalidPool => write!(f, "invalid bounty pool"),
            BountyError::AlreadyClaimed => write!(f, "this grant has already been claimed"),
            BountyError::InvalidClaim => write!(f, "claim failed ring-signature verification"),
        }
    }
}

impl std::error::Error for BountyError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, BountyError>;

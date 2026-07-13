//! Error type for `mini-execution`.

use core::fmt;

/// Errors this crate can produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionError {
    /// A candidate block's height did not immediately follow the chain's
    /// current height.
    WrongHeight { expected: u64, got: u64 },
    /// A candidate block's `prev_hash` did not match the current chain
    /// tip's header hash.
    WrongParent,
    /// A block's quorum certificate did not verify — see
    /// [`mini_chain::ChainError`] for the underlying reason. This block is
    /// not finalized and must never be applied.
    NotFinalized(mini_chain::ChainError),
    /// A block's header claimed a `state_root` that does not match the
    /// state actually produced by applying its body — the proposer either
    /// lied about the result or the body/state disagree for some other
    /// reason. Never silently accepted.
    StateRootMismatch,
    /// A block body contained more claims than [`crate::MAX_CLAIMS_PER_BLOCK`]
    /// — an allocation/CPU bound applied before processing, the same
    /// discipline `mini-chain::MAX_VOTES_PER_CERTIFICATE` applies.
    TooManyClaims,
    /// A candidate block's `timestamp_ms` did not equal its own height.
    /// `timestamp_ms` is deterministic logical time, not proposer-supplied
    /// wall time (roadmap #44's timestamp-attack finding): a signature only
    /// proves who proposed a value, never that it reflects real time, so
    /// consensus gives the proposer no discretion over it at all rather than
    /// merely bounding what discretion would otherwise exist. Every honest
    /// node enforces this identically, so it can never itself cause two
    /// honest chains to disagree (Directive 4).
    TimestampNotDeterministic { expected: u64, got: u64 },
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionError::WrongHeight { expected, got } => {
                write!(f, "expected block height {expected}, got {got}")
            }
            ExecutionError::WrongParent => {
                write!(f, "block's prev_hash does not match the chain tip")
            }
            ExecutionError::NotFinalized(e) => write!(f, "block is not finalized: {e}"),
            ExecutionError::StateRootMismatch => {
                write!(
                    f,
                    "header's state_root does not match the state its body produces"
                )
            }
            ExecutionError::TooManyClaims => write!(f, "block body exceeds the claim-count cap"),
            ExecutionError::TimestampNotDeterministic { expected, got } => write!(
                f,
                "block timestamp_ms {got} does not equal its required deterministic value {expected}"
            ),
        }
    }
}

impl std::error::Error for ExecutionError {}

impl From<mini_chain::ChainError> for ExecutionError {
    fn from(e: mini_chain::ChainError) -> Self {
        ExecutionError::NotFinalized(e)
    }
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ExecutionError>;

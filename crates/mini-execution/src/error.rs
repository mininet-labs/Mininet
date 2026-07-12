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
    /// A candidate block's `timestamp_ms` did not strictly exceed the
    /// previous finalized block's — a proposer-controlled field is
    /// otherwise free to stay flat or run backwards with no consequence
    /// (roadmap #44's timestamp-attack finding). Every honest node enforces
    /// this identically, so it can never itself cause two honest chains to
    /// disagree (Directive 4) — a block either commits everywhere or nowhere.
    NonMonotonicTimestamp { previous: u64, got: u64 },
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
            ExecutionError::NonMonotonicTimestamp { previous, got } => write!(
                f,
                "block timestamp_ms {got} does not strictly exceed the previous block's {previous}"
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

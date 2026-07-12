//! Error types for `mini-consensus`.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ConsensusError>;

/// Why a consensus operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConsensusError {
    /// A message's wire encoding was truncated, over-long, or otherwise
    /// malformed. Purely structural — a well-framed but untrusted message
    /// decodes fine and is rejected later, on its merits, by the round
    /// driver and `mini_chain::verify_finality`.
    Malformed,
    /// A decoded message declared a size beyond this crate's hard cap for
    /// untrusted input ([`crate::MAX_MESSAGE_BYTES`] and the per-field
    /// bounds around it).
    TooLarge,
    /// Chain/finality verification failed while forming or applying a
    /// certificate.
    Chain(mini_chain::ChainError),
    /// The state machine could not apply a finalized block.
    Execution(mini_execution::ExecutionError),
    /// A transport (socket) error surfaced while running a networked round.
    Transport(mini_bearer::BearerError),
    /// A round was driven past the guarantees this crate currently offers —
    /// most often: the target height was not reached before the caller's
    /// deadline, because round-0's proposer never delivered (see the
    /// crate-level "Honest limits": there is no view-change yet).
    Stalled,
}

impl core::fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConsensusError::Malformed => write!(f, "consensus message is malformed"),
            ConsensusError::TooLarge => write!(f, "consensus message exceeds its size cap"),
            ConsensusError::Chain(e) => write!(f, "chain: {e}"),
            ConsensusError::Execution(e) => write!(f, "execution: {e}"),
            ConsensusError::Transport(e) => write!(f, "transport: {e}"),
            ConsensusError::Stalled => {
                write!(f, "round stalled before reaching the target height")
            }
        }
    }
}

impl std::error::Error for ConsensusError {}

impl From<mini_chain::ChainError> for ConsensusError {
    fn from(e: mini_chain::ChainError) -> Self {
        ConsensusError::Chain(e)
    }
}

impl From<mini_execution::ExecutionError> for ConsensusError {
    fn from(e: mini_execution::ExecutionError) -> Self {
        ConsensusError::Execution(e)
    }
}

impl From<mini_bearer::BearerError> for ConsensusError {
    fn from(e: mini_bearer::BearerError) -> Self {
        ConsensusError::Transport(e)
    }
}

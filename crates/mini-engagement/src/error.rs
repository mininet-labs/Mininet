//! Errors for the engagement state machine.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, EngagementError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EngagementError {
    /// The requested transition is not defined from the engagement's
    /// current state (e.g. accepting an already-`Completed` engagement).
    InvalidTransition,
    /// A milestone release would push the running released total past
    /// the escrowed claim's amount.
    MilestoneExceedsEscrow,
    /// Reconciling the escrow claim against a canonical ledger failed
    /// (bad signature, malformed key) -- see
    /// [`crate::settlement::canonical_completion_status`].
    Settlement(mini_settlement::SettlementError),
}

impl core::fmt::Display for EngagementError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EngagementError::InvalidTransition => {
                write!(f, "invalid engagement state transition")
            }
            EngagementError::MilestoneExceedsEscrow => {
                write!(f, "milestone release would exceed the escrowed amount")
            }
            EngagementError::Settlement(inner) => {
                write!(f, "escrow claim settlement reconciliation failed: {inner}")
            }
        }
    }
}

impl std::error::Error for EngagementError {}

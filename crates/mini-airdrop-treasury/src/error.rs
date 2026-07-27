//! Errors for `mini-airdrop-treasury`.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PayoutApprovalError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PayoutApprovalError {
    /// Fewer distinct, verified, authorized signers approved this payout
    /// than [`mini_treasury::TreasurySignerSet::threshold`] requires.
    ThresholdNotMet,
    /// No candidate approvals were presented at all -- distinguished from
    /// [`PayoutApprovalError::ThresholdNotMet`] so a caller can tell
    /// "nobody has voted yet" from "some voted, not enough."
    NoApprovalsPresented,
}

impl core::fmt::Display for PayoutApprovalError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PayoutApprovalError::ThresholdNotMet => {
                write!(
                    f,
                    "not enough verified signer approvals to meet the treasury threshold"
                )
            }
            PayoutApprovalError::NoApprovalsPresented => {
                write!(f, "no candidate approvals were presented")
            }
        }
    }
}

impl std::error::Error for PayoutApprovalError {}

/// Result alias for [`crate::reconciliation`].
pub type ReconciliationResult<T> = core::result::Result<T, ReconciliationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReconciliationError {
    /// The treasury balance presented does not cover every allocation in
    /// the snapshot.
    InsufficientTreasuryBalance {
        required_micro: u64,
        available_micro: u64,
    },
    /// Summing every entry's `amount_micro` would overflow `u64`. Kept
    /// distinct from [`ReconciliationError::InsufficientTreasuryBalance`]
    /// so a caller can tell "the numbers are honestly too big to add" from
    /// "the numbers add up fine but exceed the balance."
    TotalAllocationOverflow,
}

impl core::fmt::Display for ReconciliationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReconciliationError::InsufficientTreasuryBalance {
                required_micro,
                available_micro,
            } => write!(
                f,
                "treasury balance {available_micro} micro-MINI does not cover the {required_micro} micro-MINI this snapshot allocates"
            ),
            ReconciliationError::TotalAllocationOverflow => {
                write!(f, "summing this snapshot's allocations overflows u64")
            }
        }
    }
}

impl std::error::Error for ReconciliationError {}

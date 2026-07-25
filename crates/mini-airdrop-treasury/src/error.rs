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

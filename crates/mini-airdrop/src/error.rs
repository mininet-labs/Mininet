//! Errors for `mini-airdrop`.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, AirdropError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AirdropError {
    /// A snapshot builder was handed a second entry for an identity root
    /// already present -- one claim per identity root, enforced at
    /// construction, not left to a caller's discipline.
    DuplicateIdentityRoot,
    /// An allocation of zero moves nothing; every entry must be real.
    ZeroAmount,
    /// [`crate::snapshot::AllocationEntry::reason`] exceeded
    /// [`crate::snapshot::MAX_REASON_BYTES`].
    ReasonTooLong,
    /// A snapshot exceeded [`crate::snapshot::MAX_ENTRIES`].
    TooManyEntries,
    /// A campaign id exceeded [`crate::snapshot::MAX_CAMPAIGN_ID_BYTES`].
    CampaignIdTooLong,
    /// A recipient address exceeded
    /// [`crate::claim::MAX_RECIPIENT_BYTES`].
    RecipientTooLong,
    /// A [`crate::claim::ClaimRequest::campaign_id`] does not match the
    /// snapshot it was presented against -- the same claim replayed
    /// against a different campaign is rejected, not silently accepted.
    CampaignMismatch,
    /// The claimant's presented KEL does not correspond to
    /// [`crate::claim::ClaimRequest::identity_root`] -- either forged or
    /// simply the wrong KEL supplied.
    IdentityMismatch,
    /// The claimant's KEL failed self-verification (malformed, broken
    /// signature chain, etc.) before this crate ever got to check the
    /// claim signature.
    BadKel(did_mini::IdentityError),
    /// The claim request's signatures did not meet the claimant's current
    /// KEL threshold -- the claimant did not prove control of the
    /// identity root.
    SignatureThresholdNotMet,
    /// `identity_root` has no entry in the snapshot at all.
    NotEligible,
    /// This identity root has already redeemed its allocation for this
    /// campaign -- the entire reason [`crate::registry::ClaimedRegistry`]
    /// exists.
    AlreadyClaimed,
}

impl core::fmt::Display for AirdropError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AirdropError::DuplicateIdentityRoot => {
                write!(f, "identity root already has an entry in this snapshot")
            }
            AirdropError::ZeroAmount => write!(f, "allocation amount must be nonzero"),
            AirdropError::ReasonTooLong => {
                write!(f, "allocation reason exceeds the maximum length")
            }
            AirdropError::TooManyEntries => write!(f, "snapshot exceeds the maximum entry count"),
            AirdropError::CampaignIdTooLong => write!(f, "campaign id exceeds the maximum length"),
            AirdropError::RecipientTooLong => {
                write!(f, "recipient address exceeds the maximum length")
            }
            AirdropError::CampaignMismatch => {
                write!(
                    f,
                    "claim request's campaign id does not match this snapshot"
                )
            }
            AirdropError::IdentityMismatch => {
                write!(
                    f,
                    "presented KEL does not correspond to the claimed identity root"
                )
            }
            AirdropError::BadKel(inner) => write!(f, "claimant KEL failed verification: {inner}"),
            AirdropError::SignatureThresholdNotMet => {
                write!(
                    f,
                    "claim request signatures did not meet the identity's signing threshold"
                )
            }
            AirdropError::NotEligible => {
                write!(f, "identity root has no allocation in this snapshot")
            }
            AirdropError::AlreadyClaimed => {
                write!(f, "identity root has already claimed this campaign")
            }
        }
    }
}

impl std::error::Error for AirdropError {}

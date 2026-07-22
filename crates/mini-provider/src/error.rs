//! Errors for the edge/provider vocabulary.

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ProviderError>;

/// Why a declaration or grant failed structural well-formedness.
///
/// This crate never judges whether a provider is good, honest, licensed, or
/// safe -- the protocol has no opinion and must never acquire one (FD-18
/// Part I, T2). Every variant here is a *structural* defect (an unbounded
/// hold, an already-expired offer, an inverted time window) -- never a
/// judgment about the provider's trustworthiness.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderError {
    /// The declaration's `expires_at_ms` is not in the future of `now_ms`.
    DeclarationExpired(u64),
    /// `CustodyPosture::JustInTime` was declared with a zero-length hold
    /// bound, which is indistinguishable from an unbounded hold.
    UnboundedHold,
    /// `description` exceeds [`crate::MAX_DESCRIPTION_BYTES`].
    DescriptionTooLong,
    /// `jurisdictions` exceeds [`crate::MAX_JURISDICTION_CLAIMS`].
    TooManyJurisdictionClaims,
    /// `data_required` or `retained_data` exceeds [`crate::MAX_DATA_REQUIREMENTS`].
    TooManyDataRequirements,
    /// An `EngagementGrant`'s `not_before_ms` is not strictly before
    /// `not_after_ms`.
    InvalidGrantWindow,
    /// An `EngagementGrant` was constructed with zero permits -- a grant
    /// that authorizes nothing is not a grant.
    EmptyPermits,
    /// `permits` exceeds [`crate::MAX_PERMITS`].
    TooManyPermits,
}

impl core::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProviderError::DeclarationExpired(at_ms) => {
                write!(f, "provider declaration expired at {at_ms}")
            }
            ProviderError::UnboundedHold => {
                write!(f, "just-in-time custody posture has an unbounded hold")
            }
            ProviderError::DescriptionTooLong => write!(f, "description exceeds the size limit"),
            ProviderError::TooManyJurisdictionClaims => {
                write!(f, "too many jurisdiction claims")
            }
            ProviderError::TooManyDataRequirements => write!(f, "too many data requirements"),
            ProviderError::InvalidGrantWindow => {
                write!(f, "grant not_before_ms is not strictly before not_after_ms")
            }
            ProviderError::EmptyPermits => write!(f, "grant carries no permits"),
            ProviderError::TooManyPermits => write!(f, "too many permits"),
        }
    }
}

impl std::error::Error for ProviderError {}

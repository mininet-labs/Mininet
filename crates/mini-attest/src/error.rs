//! Errors for Tier-0 engagement attestations.

use did_mini::IdentityError;
use mini_crypto::CryptoError;
use mini_engagement::EngagementError;
use mini_objects::ObjectError;
use mini_provider::ProviderError;

pub type Result<T> = core::result::Result<T, AttestError>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttestError {
    Truncated,
    TrailingBytes,
    /// A signature list arrived unsorted or with a repeated key index,
    /// so one logical object would have had more than one valid wire
    /// encoding -- and, being content-addressed, more than one identity.
    NoncanonicalSignatureOrder,
    LimitExceeded,
    UnsupportedReceiptVersion,
    UnsupportedReviewVersion,
    UnsupportedAssuranceTier,
    InvalidDid,
    InvalidObjectId,
    InvalidReceiptId,
    InvalidEpochLength,
    InvalidEpochWindow,
    ReceiptExpired,
    ReceiptNotYetComplete,
    GrantInactiveAtCompletion,
    EngagementNotCanonicallyComplete,
    ProviderMismatch,
    DeclarationMismatch,
    TermsMismatch,
    EngagementMismatch,
    CompletionStateMismatch,
    SettlementReferenceMismatch,
    HolderCommitmentMismatch,
    ReviewSubjectMismatch,
    ReviewerMismatch,
    BadProviderSignature,
    BadReviewerSignature,
    ReviewPayloadMismatch,
    DuplicateReview,
    Engagement(EngagementError),
    Provider(ProviderError),
    Identity(IdentityError),
    Crypto(CryptoError),
    Object(ObjectError),
}

impl core::fmt::Display for AttestError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            AttestError::Truncated => "attestation bytes are truncated",
            AttestError::TrailingBytes => "trailing bytes after attestation structure",
            AttestError::NoncanonicalSignatureOrder => "signature indices are unsorted or repeated",
            AttestError::LimitExceeded => "attestation decode limit exceeded",
            AttestError::UnsupportedReceiptVersion => "unsupported completion receipt version",
            AttestError::UnsupportedReviewVersion => "unsupported signed review version",
            AttestError::UnsupportedAssuranceTier => "unsupported attestation assurance tier",
            AttestError::InvalidDid => "invalid did:mini identifier",
            AttestError::InvalidObjectId => "invalid content-addressed object id",
            AttestError::InvalidReceiptId => "receipt id does not match receipt bytes",
            AttestError::InvalidEpochLength => "epoch length must be greater than zero",
            AttestError::InvalidEpochWindow => "receipt epoch window is invalid",
            AttestError::ReceiptExpired => "completion receipt has expired",
            AttestError::ReceiptNotYetComplete => "completion time is in the future",
            AttestError::GrantInactiveAtCompletion => {
                "provider grant was not active when the engagement completed"
            }
            AttestError::EngagementNotCanonicallyComplete => {
                "engagement is not both locally and canonically complete"
            }
            AttestError::ProviderMismatch => "provider does not match engagement or receipt",
            AttestError::DeclarationMismatch => "provider declaration does not match receipt",
            AttestError::TermsMismatch => "engagement terms do not match receipt",
            AttestError::EngagementMismatch => "engagement id does not match receipt",
            AttestError::CompletionStateMismatch => "completion state commitment does not match",
            AttestError::SettlementReferenceMismatch => {
                "canonical settlement reference does not match"
            }
            AttestError::HolderCommitmentMismatch => {
                "holder token does not match the receipt commitment"
            }
            AttestError::ReviewSubjectMismatch => "review subject does not match the receipt",
            AttestError::ReviewerMismatch => {
                "reviewer is not the pairwise subject bound to the holder token"
            }
            AttestError::BadProviderSignature => "provider signature does not verify",
            AttestError::BadReviewerSignature => "reviewer signature does not verify",
            AttestError::ReviewPayloadMismatch => "review payload hash does not match",
            AttestError::DuplicateReview => {
                "this receipt has already reviewed this subject in this registry"
            }
            AttestError::Engagement(_) => "engagement verification failed",
            AttestError::Provider(_) => "provider grant verification failed",
            AttestError::Identity(_) => "identity verification failed",
            AttestError::Crypto(_) => "cryptographic operation failed",
            AttestError::Object(_) => "object identifier operation failed",
        };
        f.write_str(message)
    }
}

impl std::error::Error for AttestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AttestError::Engagement(error) => Some(error),
            AttestError::Provider(error) => Some(error),
            AttestError::Identity(error) => Some(error),
            AttestError::Crypto(error) => Some(error),
            AttestError::Object(error) => Some(error),
            _ => None,
        }
    }
}

impl From<EngagementError> for AttestError {
    fn from(error: EngagementError) -> Self {
        AttestError::Engagement(error)
    }
}

impl From<ProviderError> for AttestError {
    fn from(error: ProviderError) -> Self {
        AttestError::Provider(error)
    }
}

impl From<IdentityError> for AttestError {
    fn from(error: IdentityError) -> Self {
        AttestError::Identity(error)
    }
}

impl From<CryptoError> for AttestError {
    fn from(error: CryptoError) -> Self {
        AttestError::Crypto(error)
    }
}

impl From<ObjectError> for AttestError {
    fn from(error: ObjectError) -> Self {
        AttestError::Object(error)
    }
}

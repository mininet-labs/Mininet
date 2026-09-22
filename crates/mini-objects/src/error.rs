//! Errors for the object model.

use did_mini::IdentityError;
use mini_crypto::CryptoError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, ObjectError>;

/// Why an object failed to decode or verify.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ObjectError {
    /// Bytes ended before a declared length.
    Truncated,
    /// Bytes remained after a complete decode.
    TrailingBytes,
    /// A signature list arrived unsorted or with a repeated key index,
    /// so one logical object would have had more than one valid wire
    /// encoding -- and, being content-addressed, more than one identity.
    NoncanonicalSignatureOrder,
    /// A field was structurally invalid.
    BadObject,
    /// A declared count or length exceeded a hard decode limit.
    LimitExceeded,
    /// The object's id does not match its canonical bytes.
    IdMismatch,
    /// The signing device named by the object does not match the KEL supplied.
    DeviceMismatch,
    /// The device lacks the capability required to author this object type.
    MissingCapability,
    /// The envelope's version byte is not one this decoder recognizes.
    UnsupportedEnvelopeVersion,
    /// A [`crate::capability`] grant's format version is not recognized.
    UnsupportedCapabilityVersion,
    /// A capability check was made against a scope the grant was not
    /// issued for.
    CapabilityScopeMismatch,
    /// A capability check asked for a right the grant does not carry.
    CapabilityRightMismatch,
    /// The presented token secret does not match the grant's commitment.
    CapabilityTokenMismatch,
    /// The holder proof was not made by the grant's named grantee.
    CapabilityGranteeMismatch,
    /// A verifier-issued request challenge has already authorized an operation.
    CapabilityRequestReplay,
    /// The grant's validity window has already ended.
    CapabilityExpired,
    /// The grant's validity window has not started yet.
    CapabilityNotYetValid,
    /// A valid signature over a grant is not, by itself, authorization
    /// over the requested resource (F-13): the grant's issuer does not
    /// match the resource owner the caller supplied. Any signer can
    /// produce a perfectly valid, well-formed grant naming any resource
    /// -- [`crate::capability::CapabilityGrant::validate`] refuses to
    /// treat that as authorization unless the caller establishes, through
    /// its own trusted channel, who actually owns the resource and
    /// supplies that identity for this check.
    CapabilityIssuerNotResourceOwner,
    /// An AI-disclosure object's mandatory provenance metadata (producing
    /// system id, model id, or production time) was empty or otherwise
    /// structurally invalid. This field is required, never optional, so it
    /// cannot be silently omitted by a caller.
    MissingAiProvenance,
    /// An AI-disclosure object's wire encoding was decoded through the
    /// human-authored [`crate::Object`] path, or vice versa. The two
    /// envelopes are deliberately distinct byte formats (a different leading
    /// tag) precisely so this substitution is impossible: this error means
    /// something upstream already forced bytes across that boundary rather
    /// than this decoder catching a genuine ambiguity.
    WrongEnvelopeKind,
    /// An identity/delegation/signature failure.
    Identity(IdentityError),
    /// A cryptographic primitive failure.
    Crypto(CryptoError),
}

impl core::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ObjectError::Truncated => write!(f, "object bytes truncated"),
            ObjectError::CapabilityRequestReplay => {
                write!(f, "capability request challenge was already consumed")
            }
            ObjectError::TrailingBytes => write!(f, "trailing bytes after object"),
            ObjectError::NoncanonicalSignatureOrder => {
                write!(f, "signature indices are unsorted or repeated")
            }
            ObjectError::BadObject => write!(f, "structurally invalid object"),
            ObjectError::LimitExceeded => write!(f, "decode limit exceeded"),
            ObjectError::IdMismatch => write!(f, "object id does not match bytes"),
            ObjectError::DeviceMismatch => write!(f, "signing device does not match KEL"),
            ObjectError::MissingCapability => {
                write!(f, "device lacks the capability for this object type")
            }
            ObjectError::UnsupportedEnvelopeVersion => {
                write!(f, "unsupported or unrecognized envelope version")
            }
            ObjectError::UnsupportedCapabilityVersion => {
                write!(f, "unsupported or unrecognized capability grant version")
            }
            ObjectError::CapabilityScopeMismatch => {
                write!(f, "capability grant does not cover the requested scope")
            }
            ObjectError::CapabilityRightMismatch => {
                write!(f, "capability grant does not cover the requested right")
            }
            ObjectError::CapabilityTokenMismatch => {
                write!(f, "capability token does not match the grant's commitment")
            }
            ObjectError::CapabilityGranteeMismatch => {
                write!(f, "holder proof was not made by the grant's named grantee")
            }
            ObjectError::CapabilityExpired => write!(f, "capability grant has expired"),
            ObjectError::CapabilityNotYetValid => {
                write!(f, "capability grant is not valid yet")
            }
            ObjectError::CapabilityIssuerNotResourceOwner => {
                write!(
                    f,
                    "capability grant's issuer is not the resource's actual owner"
                )
            }
            ObjectError::MissingAiProvenance => {
                write!(
                    f,
                    "AI disclosure object is missing mandatory provenance metadata"
                )
            }
            ObjectError::WrongEnvelopeKind => {
                write!(
                    f,
                    "bytes belong to the other envelope kind (human/AI mismatch)"
                )
            }
            ObjectError::Identity(e) => write!(f, "identity error: {e}"),
            ObjectError::Crypto(e) => write!(f, "crypto error: {e}"),
        }
    }
}

impl std::error::Error for ObjectError {}

impl From<IdentityError> for ObjectError {
    fn from(e: IdentityError) -> Self {
        ObjectError::Identity(e)
    }
}
impl From<CryptoError> for ObjectError {
    fn from(e: CryptoError) -> Self {
        ObjectError::Crypto(e)
    }
}

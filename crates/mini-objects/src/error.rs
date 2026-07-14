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
    /// The grant's validity window has already ended.
    CapabilityExpired,
    /// The grant's validity window has not started yet.
    CapabilityNotYetValid,
    /// An identity/delegation/signature failure.
    Identity(IdentityError),
    /// A cryptographic primitive failure.
    Crypto(CryptoError),
}

impl core::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ObjectError::Truncated => write!(f, "object bytes truncated"),
            ObjectError::TrailingBytes => write!(f, "trailing bytes after object"),
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

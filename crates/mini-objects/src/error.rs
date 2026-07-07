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

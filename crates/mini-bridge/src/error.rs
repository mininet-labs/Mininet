//! Errors for the bridge/pluggable-transport crate.

use did_mini::IdentityError;
use mini_bearer::BearerError;
use mini_crypto::CryptoError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, BridgeError>;

/// Why a bridge-descriptor or transport operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BridgeError {
    /// Bytes ended before a declared length.
    Truncated,
    /// Bytes remained after a complete decode.
    TrailingBytes,
    /// A declared count or length exceeded a hard decode limit.
    LimitExceeded,
    /// An unrecognized [`crate::TransportId`] tag.
    BadTransportId,
    /// The bridge descriptor's format version is not recognized.
    UnsupportedDescriptorVersion,
    /// The descriptor's issuer signature does not verify against the
    /// supplied KEL.
    BadSignature,
    /// The descriptor's validity window has already ended.
    Expired,
    /// The descriptor's validity window has not started yet.
    NotYetValid,
    /// A caller asked to connect with a transport whose capabilities
    /// cannot satisfy the caller's declared minimum requirements — e.g.
    /// requesting `active_probe_resistance >= Cryptographic` from a
    /// transport declared `None`. Refusing this before dialing prevents a
    /// silent downgrade to a weaker transport than the caller believes it
    /// is using.
    DowngradeRejected,
    /// A [`crate::PluggableTransport`] was asked to connect using a
    /// [`crate::BridgeDescriptor`] issued for a different
    /// [`crate::TransportId`].
    TransportMismatch,
    /// The descriptor's opaque endpoint bytes could not be interpreted by
    /// the transport being used to dial it.
    BadEndpoint,
    /// An identity/delegation/signature failure.
    Identity(IdentityError),
    /// A cryptographic primitive failure.
    Crypto(CryptoError),
    /// A bearer/channel transport failure.
    Bearer(BearerError),
}

impl core::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BridgeError::Truncated => write!(f, "bridge descriptor bytes truncated"),
            BridgeError::TrailingBytes => write!(f, "trailing bytes after bridge structure"),
            BridgeError::LimitExceeded => write!(f, "decode limit exceeded"),
            BridgeError::BadTransportId => write!(f, "unrecognized transport id tag"),
            BridgeError::UnsupportedDescriptorVersion => {
                write!(f, "unsupported or unrecognized bridge descriptor version")
            }
            BridgeError::BadSignature => write!(f, "bridge descriptor signature does not verify"),
            BridgeError::Expired => write!(f, "bridge descriptor has expired"),
            BridgeError::NotYetValid => write!(f, "bridge descriptor is not valid yet"),
            BridgeError::DowngradeRejected => write!(
                f,
                "transport capabilities do not satisfy the caller's declared minimum"
            ),
            BridgeError::TransportMismatch => {
                write!(f, "bridge descriptor was issued for a different transport")
            }
            BridgeError::BadEndpoint => write!(f, "opaque endpoint bytes could not be parsed"),
            BridgeError::Identity(e) => write!(f, "identity error: {e}"),
            BridgeError::Crypto(e) => write!(f, "crypto error: {e}"),
            BridgeError::Bearer(e) => write!(f, "bearer error: {e}"),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<IdentityError> for BridgeError {
    fn from(e: IdentityError) -> Self {
        BridgeError::Identity(e)
    }
}
impl From<CryptoError> for BridgeError {
    fn from(e: CryptoError) -> Self {
        BridgeError::Crypto(e)
    }
}
impl From<BearerError> for BridgeError {
    fn from(e: BearerError) -> Self {
        BridgeError::Bearer(e)
    }
}

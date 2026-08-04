//! Errors for authenticated transport and secure peer discovery.

use did_mini::IdentityError;
use mini_bearer::BearerError;
use mini_crypto::CryptoError;
use mini_relay::RelayError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, TransportSecurityError>;

/// Why a transport-authentication or discovery operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportSecurityError {
    Truncated,
    TrailingBytes,
    Malformed,
    LimitExceeded,
    UnsupportedVersion,
    WrongRole,
    WrongPurpose,
    WrongNetwork,
    NotYetValid,
    Expired,
    LifetimeTooLong,
    EmptyCapability,
    CapabilityDenied,
    IdentityMismatch,
    EndpointMismatch,
    RoutingKeyMismatch,
    Replay,
    InvalidSelectionPolicy,
    MixedTransportNotReviewed,
    /// Every bounded dial candidate failed before a fully authenticated
    /// connection existed. No partially verified state is returned.
    DialExhausted {
        attempted: usize,
    },
    /// Two onion roles reused a visible endpoint, routing key, root, or device.
    RouteEndpointReuse,
    /// A bearer send/receive or channel-open failure made the ordered CH1 state
    /// ambiguous. The connection is permanently unusable and must be replaced.
    ConnectionPoisoned,
    Bearer(BearerError),
    Relay(RelayError),
    Identity(IdentityError),
    Crypto(CryptoError),
}

impl core::fmt::Display for TransportSecurityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => write!(f, "transport-security bytes truncated"),
            Self::TrailingBytes => write!(f, "trailing bytes after transport-security value"),
            Self::Malformed => write!(f, "malformed transport-security value"),
            Self::LimitExceeded => write!(f, "transport-security limit exceeded"),
            Self::UnsupportedVersion => write!(f, "unsupported transport-security version"),
            Self::WrongRole => write!(f, "session authentication has the wrong endpoint role"),
            Self::WrongPurpose => write!(f, "session authentication has the wrong typed purpose"),
            Self::WrongNetwork => write!(f, "peer advertisement belongs to another network"),
            Self::NotYetValid => write!(f, "signed transport value is not valid yet"),
            Self::Expired => write!(f, "signed transport value has expired"),
            Self::LifetimeTooLong => write!(f, "signed transport value exceeds its maximum lifetime"),
            Self::EmptyCapability => write!(f, "transport purpose maps to no delegated capability"),
            Self::CapabilityDenied => write!(f, "delegated device lacks the required transport capability"),
            Self::IdentityMismatch => write!(f, "claim identity does not match the supplied KELs"),
            Self::EndpointMismatch => write!(f, "self-certifying endpoint id does not match the claim"),
            Self::RoutingKeyMismatch => write!(f, "routing key does not match the authenticated endpoint"),
            Self::Replay => write!(f, "transport authentication or advertisement replayed"),
            Self::InvalidSelectionPolicy => write!(f, "invalid diverse-peer selection policy"),
            Self::MixedTransportNotReviewed => write!(
                f,
                "mixed/burst transport is unavailable until the exact executor receives independent review"
            ),
            Self::DialExhausted { attempted } => write!(
                f,
                "all {attempted} bounded transport candidates failed authentication or connection"
            ),
            Self::RouteEndpointReuse => write!(
                f,
                "one visible transport endpoint, routing key, root, or device was assigned multiple onion roles"
            ),
            Self::ConnectionPoisoned => write!(
                f,
                "authenticated connection is unusable after an ambiguous bearer/channel failure"
            ),
            Self::Bearer(error) => write!(f, "bearer/channel operation failed: {error}"),
            Self::Relay(error) => write!(f, "relay/onion operation failed: {error}"),
            Self::Identity(error) => write!(f, "identity verification failed: {error}"),
            Self::Crypto(error) => write!(f, "cryptographic operation failed: {error}"),
        }
    }
}

impl std::error::Error for TransportSecurityError {}

impl From<BearerError> for TransportSecurityError {
    fn from(error: BearerError) -> Self {
        Self::Bearer(error)
    }
}

impl From<RelayError> for TransportSecurityError {
    fn from(error: RelayError) -> Self {
        Self::Relay(error)
    }
}

impl From<IdentityError> for TransportSecurityError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}

impl From<CryptoError> for TransportSecurityError {
    fn from(error: CryptoError) -> Self {
        Self::Crypto(error)
    }
}

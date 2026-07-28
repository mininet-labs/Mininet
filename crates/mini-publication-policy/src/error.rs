//! Error type for `mini-publication-policy`.

use core::fmt;

use mini_relay::RelayError;
use mini_resource_pricing::PricingError;
use mini_transport_policy::TransportPolicyError;

/// Errors this crate can produce. Every variant wraps a typed error from
/// the crate that actually detected the problem -- never a generic
/// string -- so a caller can match on the underlying cause the same way
/// it would if it had called `mini-transport-policy`/`mini-resource-
/// pricing`/`mini-relay` directly.
///
/// Not `Copy` -- [`RelayError`] itself isn't, since some of its variants
/// wrap `mini-crypto`/`did-mini` errors that carry non-`Copy` state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationPolicyError {
    /// The requested [`crate::PublicationProfile::transport`] tier cannot
    /// satisfy a requested [`mini_privacy_policy::ProtectionProperty`].
    Routing(TransportPolicyError),
    /// Pricing the requested transport/payload would overflow.
    Pricing(PricingError),
    /// Planning relay roles for a source-hiding path failed -- either the
    /// tier needs no relay at all, needs machinery `mini-relay` does not
    /// implement, or the achieved privacy did not name the onion-relay
    /// mechanism.
    Relay(RelayError),
}

impl fmt::Display for PublicationPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublicationPolicyError::Routing(err) => write!(f, "{err}"),
            PublicationPolicyError::Pricing(err) => write!(f, "{err}"),
            PublicationPolicyError::Relay(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PublicationPolicyError {}

impl From<TransportPolicyError> for PublicationPolicyError {
    fn from(err: TransportPolicyError) -> Self {
        PublicationPolicyError::Routing(err)
    }
}

impl From<PricingError> for PublicationPolicyError {
    fn from(err: PricingError) -> Self {
        PublicationPolicyError::Pricing(err)
    }
}

impl From<RelayError> for PublicationPolicyError {
    fn from(err: RelayError) -> Self {
        PublicationPolicyError::Relay(err)
    }
}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PublicationPolicyError>;

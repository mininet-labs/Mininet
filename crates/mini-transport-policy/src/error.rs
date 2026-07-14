//! Error type for `mini-transport-policy`.

use core::fmt;

use mini_privacy_policy::ProtectionProperty;

/// Errors this crate can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportPolicyError {
    /// A [`crate::TransportRequest`] asked for a property that needs a
    /// higher tier than the one it requested. The router fails closed
    /// here rather than silently routing at a lower tier than the
    /// property needs — see [`crate::route`]'s own docs.
    UnsatisfiableProperty {
        property: ProtectionProperty,
        requested_tier: mini_privacy_policy::PrivacyTier,
        minimum_tier: mini_privacy_policy::PrivacyTier,
    },
}

impl fmt::Display for TransportPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportPolicyError::UnsatisfiableProperty {
                property,
                requested_tier,
                minimum_tier,
            } => write!(
                f,
                "{property:?} needs at least tier {minimum_tier:?}, but tier \
                 {requested_tier:?} was requested"
            ),
        }
    }
}

impl std::error::Error for TransportPolicyError {}

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, TransportPolicyError>;

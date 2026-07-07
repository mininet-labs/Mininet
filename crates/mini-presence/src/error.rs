//! Errors for presence attestation and verification.

use did_mini::IdentityError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, PresenceError>;

/// Why a presence attestation was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PresenceError {
    /// The attestation used an unsupported presence protocol version.
    UnsupportedVersion(u8),
    /// The transport cannot evidence physical co-presence (e.g. an internet relay).
    NotProximityTransport,
    /// The attestation is not bound to the channel the verifier observed.
    BindingMismatch,
    /// The start/finish timestamps are inconsistent or outside policy.
    BadTimeWindow,
    /// Too few round-trip range samples to judge proximity.
    NotEnoughRangeSamples,
    /// The measured round-trip range exceeds the policy threshold.
    RangeExceeded,
    /// A party's device identifier does not match the KEL supplied for it.
    DeviceMismatch,
    /// A party's KEL digest does not match the KEL supplied for it.
    KelDigestMismatch,
    /// A device lacks the `ATTEST` capability from its identity root.
    MissingAttestCapability,
    /// A nonce was reused (or the two parties reused each other's nonce): replay.
    Replay,
    /// Both devices belong to the same identity root: an identity root cannot be co-present
    /// with itself (P2 target — presence is evidence of two identity roots meeting).
    SelfPresence,
    /// An underlying identity/delegation/signature failure.
    Identity(IdentityError),
}

impl core::fmt::Display for PresenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PresenceError::UnsupportedVersion(v) => {
                write!(f, "unsupported presence version {v}")
            }
            PresenceError::NotProximityTransport => {
                write!(f, "transport cannot evidence physical co-presence")
            }
            PresenceError::BindingMismatch => write!(f, "attestation not bound to this channel"),
            PresenceError::BadTimeWindow => write!(f, "inconsistent or out-of-policy time window"),
            PresenceError::NotEnoughRangeSamples => write!(f, "too few range samples"),
            PresenceError::RangeExceeded => write!(f, "round-trip range exceeds policy"),
            PresenceError::DeviceMismatch => write!(f, "device identifier does not match its KEL"),
            PresenceError::KelDigestMismatch => write!(f, "KEL digest does not match its KEL"),
            PresenceError::MissingAttestCapability => {
                write!(f, "device lacks the ATTEST capability")
            }
            PresenceError::Replay => write!(f, "nonce reuse (replay) detected"),
            PresenceError::SelfPresence => {
                write!(f, "both devices belong to the same identity root")
            }
            PresenceError::Identity(e) => write!(f, "identity error: {e}"),
        }
    }
}

impl std::error::Error for PresenceError {}

impl From<IdentityError> for PresenceError {
    fn from(e: IdentityError) -> Self {
        PresenceError::Identity(e)
    }
}

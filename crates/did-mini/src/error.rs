//! Error types for `did-mini`.

use core::fmt;
use mini_crypto::CryptoError;

/// Errors produced while building, encoding, or verifying a `did:mini` identity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentityError {
    /// A cryptographic primitive failed (bad key/signature length, etc.).
    Crypto(CryptoError),
    /// The byte buffer ended before a field could be fully read.
    Truncated,
    /// Bytes remained after a structure was fully decoded.
    TrailingBytes,
    /// An event carried an unrecognised type tag.
    UnknownEventTag(u8),
    /// The Key Event Log had no events.
    EmptyKel,
    /// The first event was not a well-formed inception (`icp`, sn 0, no prior),
    /// or a later event claimed to be an inception.
    NotInception,
    /// An event's sequence number did not follow the previous one.
    WrongSequence { expected: u64, got: u64 },
    /// The self-certifying identifier did not match the inception it claims to
    /// derive from — the identity is not authentic.
    ScidMismatch,
    /// An event's `prior` digest did not match the previous event — the log was
    /// tampered with or reordered.
    BrokenChain { sn: u64 },
    /// A rotation revealed keys that do not match the prior pre-rotation
    /// commitment — the rotation is not authorised by the legitimate controller.
    PreRotationMismatch { sn: u64 },
    /// Too few valid signatures from the authoritative keys to meet the
    /// signing threshold.
    ThresholdNotMet { sn: u64, needed: u32, got: u32 },
    /// A structurally invalid event (e.g. a non-UTF-8 identifier, a malformed
    /// establishment, or an out-of-spec field).
    BadEvent,
    /// A decoded field was larger than this wire profile permits.
    FieldTooLarge { field: &'static str, max: usize, got: usize },
    /// A decoded vector had too many entries for this wire profile.
    TooManyItems { field: &'static str, max: usize, got: usize },
    /// An establishment event had no keys.
    EmptyKeySet,
    /// An establishment threshold was zero or larger than the key set.
    InvalidThreshold { threshold: u32, key_count: usize },
    /// An establishment event repeated the same public key.
    DuplicateKey,
    /// A pre-rotation commitment set was empty or had an invalid next threshold.
    InvalidNextThreshold { threshold: u32, commitment_count: usize },
    /// A string was not a valid `did:mini:<scid>` identifier.
    DidFormat,
    /// A device is not (or no longer) delegated by the claimed human-root, or the
    /// device does not name that root as its delegator (SPEC-01 §6).
    NotDelegated,
    /// Detached signatures over a message did not reach the identity's threshold.
    SignatureThresholdNotMet {
        /// Distinct valid signatures required.
        needed: u32,
        /// Distinct valid signatures found.
        got: u32,
    },
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentityError::Crypto(e) => write!(f, "crypto error: {e}"),
            IdentityError::Truncated => write!(f, "buffer truncated"),
            IdentityError::TrailingBytes => write!(f, "unexpected trailing bytes"),
            IdentityError::UnknownEventTag(t) => write!(f, "unknown event tag: 0x{t:02x}"),
            IdentityError::EmptyKel => write!(f, "empty key event log"),
            IdentityError::NotInception => write!(f, "malformed or misplaced inception event"),
            IdentityError::WrongSequence { expected, got } => {
                write!(f, "out-of-order event: expected sn {expected}, got {got}")
            }
            IdentityError::ScidMismatch => write!(f, "scid does not match inception (not authentic)"),
            IdentityError::BrokenChain { sn } => write!(f, "broken kel chain at sn {sn}"),
            IdentityError::PreRotationMismatch { sn } => {
                write!(f, "rotation at sn {sn} does not match pre-rotation commitment")
            }
            IdentityError::ThresholdNotMet { sn, needed, got } => {
                write!(f, "signing threshold not met at sn {sn}: needed {needed}, got {got}")
            }
            IdentityError::BadEvent => write!(f, "structurally invalid event"),
            IdentityError::FieldTooLarge { field, max, got } => {
                write!(f, "field {field} too large: max {max} bytes/items, got {got}")
            }
            IdentityError::TooManyItems { field, max, got } => {
                write!(f, "too many {field}: max {max}, got {got}")
            }
            IdentityError::EmptyKeySet => write!(f, "establishment event has no keys"),
            IdentityError::InvalidThreshold { threshold, key_count } => write!(
                f,
                "invalid threshold {threshold} for key set of size {key_count}"
            ),
            IdentityError::DuplicateKey => write!(f, "establishment event repeats a public key"),
            IdentityError::InvalidNextThreshold {
                threshold,
                commitment_count,
            } => write!(
                f,
                "invalid next threshold {threshold} for {commitment_count} commitments"
            ),
            IdentityError::DidFormat => write!(f, "not a valid did:mini identifier"),
            IdentityError::NotDelegated => write!(f, "device is not delegated by this root"),
            IdentityError::SignatureThresholdNotMet { needed, got } => write!(
                f,
                "signature threshold not met: needed {needed}, got {got}"
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<CryptoError> for IdentityError {
    fn from(e: CryptoError) -> Self {
        IdentityError::Crypto(e)
    }
}

/// Convenience result type for this crate.
pub type Result<T> = core::result::Result<T, IdentityError>;

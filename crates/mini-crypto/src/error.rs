//! Error types for `mini-crypto`.

use core::fmt;

/// Errors produced by the cryptographic primitives in this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CryptoError {
    /// A byte slice had the wrong length for the target type.
    BadLength { expected: usize, got: usize },
    /// A public key failed to decode (e.g. not a valid curve point).
    InvalidPublicKey,
    /// A signature failed to verify against the message and key.
    BadSignature,
    /// A signature-suite identifier byte was not recognised.
    UnknownSuite(u8),
    /// A key-agreement-suite identifier byte was not recognised.
    UnknownKeyAgreementSuite(u8),
    /// An AEAD-suite identifier byte was not recognised.
    UnknownAeadSuite(u8),
    /// A KDF-suite identifier byte was not recognised.
    UnknownKdfSuite(u8),
    /// A key-agreement operation mixed incompatible suites.
    KeyAgreementSuiteMismatch,
    /// Authenticated encryption or decryption failed.
    Aead,
    /// Key derivation failed, usually because the requested output was too long.
    Kdf,
    /// A multibase string used an unsupported or unknown base prefix.
    UnsupportedMultibase(char),
    /// A multibase string was empty (no base prefix).
    EmptyMultibase,
    /// Base decoding failed (malformed characters).
    BadEncoding,
    /// A multihash code was not recognised, **or** is structurally forbidden —
    /// notably SHA-1 (code `0x11`), which is collision-broken and may never be
    /// used for content addressing (SPEC-11 \[FREEZE\]).
    UnknownOrForbiddenHashCode(u64),
    /// A varint was malformed or overflowed.
    BadVarint,
    /// Randomness could not be obtained from the operating system.
    Entropy,
    /// A suite-specific [`crate::SigningKey`] method (e.g.
    /// [`crate::SigningKey::sign_ml_dsa_65`]) was called on a key
    /// belonging to a different suite.
    SignatureSuiteMismatch,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::BadLength { expected, got } => {
                write!(f, "bad length: expected {expected} bytes, got {got}")
            }
            CryptoError::InvalidPublicKey => write!(f, "invalid public key"),
            CryptoError::BadSignature => write!(f, "signature verification failed"),
            CryptoError::UnknownSuite(b) => write!(f, "unknown signature suite id: {b}"),
            CryptoError::UnknownKeyAgreementSuite(b) => {
                write!(f, "unknown key-agreement suite id: {b}")
            }
            CryptoError::UnknownAeadSuite(b) => write!(f, "unknown AEAD suite id: {b}"),
            CryptoError::UnknownKdfSuite(b) => write!(f, "unknown KDF suite id: {b}"),
            CryptoError::KeyAgreementSuiteMismatch => {
                write!(f, "key-agreement suite mismatch")
            }
            CryptoError::Aead => write!(f, "authenticated encryption/decryption failed"),
            CryptoError::Kdf => write!(f, "key derivation failed"),
            CryptoError::UnsupportedMultibase(c) => {
                write!(f, "unsupported multibase prefix: {c:?}")
            }
            CryptoError::EmptyMultibase => write!(f, "empty multibase string"),
            CryptoError::BadEncoding => write!(f, "malformed base encoding"),
            CryptoError::UnknownOrForbiddenHashCode(c) => {
                write!(f, "unknown or forbidden multihash code: 0x{c:x}")
            }
            CryptoError::BadVarint => write!(f, "malformed varint"),
            CryptoError::Entropy => write!(f, "could not obtain system entropy"),
            CryptoError::SignatureSuiteMismatch => {
                write!(f, "signing key does not belong to the requested suite")
            }
        }
    }
}

impl std::error::Error for CryptoError {}

/// Convenience result type for this crate.
pub type Result<T> = core::result::Result<T, CryptoError>;

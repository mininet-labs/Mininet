//! Suite-tagged signing/verifying keys and signatures.
//!
//! Keys are derived from a 32-byte seed, so generation is deterministic for tests
//! and reproducible across platforms; production callers use
//! [`SigningKey::generate`], which draws the seed from the operating system's
//! CSPRNG.
//!
//! Per SPEC-01 G1, secret key material is intended to live only on the user's
//! device. This type never serialises its secret half as part of any wire format;
//! the only export path is the explicit, loudly-named
//! [`SigningKey::to_seed_bytes`] for secure on-device storage, and the [`Debug`]
//! impl redacts the secret.

use ed25519_dalek::{
    Signature as DalekSignature, Signer, SigningKey as DalekSigningKey, Verifier,
    VerifyingKey as DalekVerifyingKey,
};

use zeroize::Zeroize;

use crate::error::{CryptoError, Result};
use crate::suite::SignatureSuite;

/// A public verifying key, tagged with its suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyingKey {
    suite: SignatureSuite,
    inner: DalekVerifyingKey,
}

/// A secret signing key, tagged with its suite. Secret material stays on-device.
#[derive(Clone)]
pub struct SigningKey {
    suite: SignatureSuite,
    inner: DalekSigningKey,
}

/// A signature, tagged with the suite that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    suite: SignatureSuite,
    bytes: [u8; 64],
}

impl SigningKey {
    /// Deterministically derive a signing key from a 32-byte seed.
    ///
    /// The same seed yields the same key on every platform — used heavily in tests
    /// and anywhere reproducible key derivation is needed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        SigningKey {
            suite: SignatureSuite::Ed25519,
            inner: DalekSigningKey::from_bytes(seed),
        }
    }

    /// Generate a fresh signing key using operating-system entropy.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|_| CryptoError::Entropy)?;
        let key = SigningKey::from_seed(&seed);
        // Best-effort scrub of the local seed copy.
        seed.zeroize();
        Ok(key)
    }

    /// The suite this key belongs to.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// The corresponding public verifying key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            suite: self.suite,
            inner: self.inner.verifying_key(),
        }
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig: DalekSignature = self.inner.sign(message);
        Signature {
            suite: self.suite,
            bytes: sig.to_bytes(),
        }
    }

    /// Export the 32-byte seed for **secure on-device storage only**.
    ///
    /// This is the single place secret material leaves the type, and it is named
    /// loudly on purpose. It must never be placed in any network message
    /// (SPEC-01 G1: keys never leave the device).
    pub fn to_seed_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }
}

impl core::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print secret material.
        f.debug_struct("SigningKey")
            .field("suite", &self.suite)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl VerifyingKey {
    /// The suite this key belongs to.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// Raw public-key bytes (without the suite tag).
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Reconstruct a verifying key from a suite tag and raw bytes.
    pub fn from_suite_bytes(suite: SignatureSuite, bytes: &[u8]) -> Result<Self> {
        match suite {
            SignatureSuite::Ed25519 => {
                let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadLength {
                    expected: 32,
                    got: bytes.len(),
                })?;
                let inner = DalekVerifyingKey::from_bytes(&arr)
                    .map_err(|_| CryptoError::InvalidPublicKey)?;
                Ok(VerifyingKey { suite, inner })
            }
        }
    }

    /// Verify `signature` over `message`. Returns `Err(BadSignature)` on failure,
    /// including a suite mismatch.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        if signature.suite != self.suite {
            return Err(CryptoError::BadSignature);
        }
        let sig = DalekSignature::from_bytes(&signature.bytes);
        self.inner
            .verify(message, &sig)
            .map_err(|_| CryptoError::BadSignature)
    }
}

impl Signature {
    /// The suite that produced this signature.
    pub fn suite(&self) -> SignatureSuite {
        self.suite
    }

    /// Raw 64-byte signature (without the suite tag).
    pub fn to_bytes(&self) -> [u8; 64] {
        self.bytes
    }

    /// Reconstruct a signature from a suite tag and raw bytes.
    pub fn from_suite_bytes(suite: SignatureSuite, bytes: &[u8]) -> Result<Self> {
        match suite {
            SignatureSuite::Ed25519 => {
                let arr: [u8; 64] = bytes.try_into().map_err(|_| CryptoError::BadLength {
                    expected: 64,
                    got: bytes.len(),
                })?;
                Ok(Signature { suite, bytes: arr })
            }
        }
    }
}

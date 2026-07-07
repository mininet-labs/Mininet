//! Suite-tagged X25519 key agreement for Noise-style sessions.
//!
//! Mininet's bearer layer needs an anonymous encrypted channel before it can move
//! identity logs, presence attestations, genesis chunks, or update bundles over
//! Bluetooth. This module supplies the DH primitive for that channel without
//! hand-rolling curve arithmetic: X25519 is provided by `x25519-dalek`, while this
//! crate adds Mininet's usual suite tags, length checks, and secret-redacting
//! wrappers.

use x25519_dalek::{PublicKey as DalekPublicKey, StaticSecret};
use zeroize::Zeroize;

use crate::error::{CryptoError, Result};

/// Versioned key-agreement suites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyAgreementSuite {
    /// X25519 over Curve25519 (RFC 7748). The current Noise-ready DH suite.
    X25519,
}

impl KeyAgreementSuite {
    /// The current default suite for new key-agreement keys.
    pub const DEFAULT: KeyAgreementSuite = KeyAgreementSuite::X25519;

    /// Stable single-byte wire tag for this suite.
    pub const fn tag(self) -> u8 {
        match self {
            KeyAgreementSuite::X25519 => 0x01,
        }
    }

    /// Parse a key-agreement suite from a wire tag.
    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0x01 => Ok(KeyAgreementSuite::X25519),
            other => Err(CryptoError::UnknownKeyAgreementSuite(other)),
        }
    }

    /// Public-key length in bytes for this suite.
    pub const fn public_key_len(self) -> usize {
        match self {
            KeyAgreementSuite::X25519 => 32,
        }
    }

    /// Secret-key seed length in bytes for this suite.
    pub const fn secret_key_len(self) -> usize {
        match self {
            KeyAgreementSuite::X25519 => 32,
        }
    }

    /// Shared-secret length in bytes for this suite.
    pub const fn shared_secret_len(self) -> usize {
        match self {
            KeyAgreementSuite::X25519 => 32,
        }
    }
}

/// A public key-agreement key, tagged with its suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgreementPublicKey {
    suite: KeyAgreementSuite,
    bytes: [u8; 32],
}

/// A secret key-agreement seed, tagged with its suite.
///
/// The bytes are never serialized implicitly. `Debug` redacts them and `Drop`
/// zeroizes them on a best-effort basis.
pub struct AgreementSecretKey {
    suite: KeyAgreementSuite,
    bytes: [u8; 32],
}

/// A Diffie-Hellman shared secret.
///
/// This is raw key material. Pass it through HKDF before using it as an AEAD key.
pub struct SharedSecret {
    suite: KeyAgreementSuite,
    bytes: [u8; 32],
}

impl AgreementSecretKey {
    /// Deterministically derive an X25519 secret from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        AgreementSecretKey {
            suite: KeyAgreementSuite::X25519,
            bytes: *seed,
        }
    }

    /// Generate a fresh X25519 secret using operating-system entropy.
    pub fn generate() -> Result<Self> {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|_| CryptoError::Entropy)?;
        let key = Self::from_seed(&seed);
        seed.zeroize();
        Ok(key)
    }

    /// The suite this secret key belongs to.
    pub fn suite(&self) -> KeyAgreementSuite {
        self.suite
    }

    /// Derive the corresponding public key.
    pub fn public_key(&self) -> AgreementPublicKey {
        match self.suite {
            KeyAgreementSuite::X25519 => {
                let secret = StaticSecret::from(self.bytes);
                let public = DalekPublicKey::from(&secret);
                AgreementPublicKey {
                    suite: self.suite,
                    bytes: public.to_bytes(),
                }
            }
        }
    }

    /// Perform Diffie-Hellman with `peer`.
    ///
    /// The all-zero shared secret is rejected, which catches small-order public
    /// keys and prevents a malicious peer from forcing a known shared secret.
    pub fn agree(&self, peer: &AgreementPublicKey) -> Result<SharedSecret> {
        if peer.suite != self.suite {
            return Err(CryptoError::KeyAgreementSuiteMismatch);
        }
        match self.suite {
            KeyAgreementSuite::X25519 => {
                let secret = StaticSecret::from(self.bytes);
                let peer_public = DalekPublicKey::from(peer.bytes);
                let shared = secret.diffie_hellman(&peer_public).to_bytes();
                if shared.iter().all(|&b| b == 0) {
                    return Err(CryptoError::InvalidPublicKey);
                }
                Ok(SharedSecret {
                    suite: self.suite,
                    bytes: shared,
                })
            }
        }
    }

    /// Export the 32-byte seed for **secure on-device storage only**.
    pub fn to_seed_bytes(&self) -> [u8; 32] {
        self.bytes
    }
}

impl Clone for AgreementSecretKey {
    fn clone(&self) -> Self {
        AgreementSecretKey {
            suite: self.suite,
            bytes: self.bytes,
        }
    }
}

impl Drop for AgreementSecretKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl core::fmt::Debug for AgreementSecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AgreementSecretKey")
            .field("suite", &self.suite)
            .field("secret", &"<redacted>")
            .finish()
    }
}

impl AgreementPublicKey {
    /// The suite this public key belongs to.
    pub fn suite(&self) -> KeyAgreementSuite {
        self.suite
    }

    /// Raw public-key bytes without the suite tag.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.bytes
    }

    /// Reconstruct a public key from a suite tag and raw bytes.
    pub fn from_suite_bytes(suite: KeyAgreementSuite, bytes: &[u8]) -> Result<Self> {
        match suite {
            KeyAgreementSuite::X25519 => {
                let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadLength {
                    expected: suite.public_key_len(),
                    got: bytes.len(),
                })?;
                Ok(AgreementPublicKey { suite, bytes: arr })
            }
        }
    }
}

impl SharedSecret {
    /// The suite that produced this shared secret.
    pub fn suite(&self) -> KeyAgreementSuite {
        self.suite
    }

    /// Expose the raw shared secret for immediate HKDF input.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Copy the raw shared secret for immediate HKDF input.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.bytes
    }
}

impl Clone for SharedSecret {
    fn clone(&self) -> Self {
        SharedSecret {
            suite: self.suite,
            bytes: self.bytes,
        }
    }
}

impl PartialEq for SharedSecret {
    fn eq(&self, other: &Self) -> bool {
        self.suite == other.suite && self.bytes == other.bytes
    }
}

impl Eq for SharedSecret {}

impl Drop for SharedSecret {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl core::fmt::Debug for SharedSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SharedSecret")
            .field("suite", &self.suite)
            .field("secret", &"<redacted>")
            .finish()
    }
}

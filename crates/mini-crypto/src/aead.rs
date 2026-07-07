//! Suite-tagged authenticated encryption for Mininet sessions.
//!
//! The first bearer implementation uses an anonymous encrypted channel over
//! Bluetooth/local Wi-Fi. That channel needs an AEAD; this module provides
//! ChaCha20-Poly1305 via
//! the audited `chacha20poly1305` crate, wrapped in Mininet suite tags and
//! redacting key types.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::Zeroize;

use crate::error::{CryptoError, Result};

/// Versioned AEAD suites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AeadSuite {
    /// ChaCha20-Poly1305 with a 256-bit key and 96-bit nonce (RFC 8439).
    ChaCha20Poly1305,
}

impl AeadSuite {
    /// The current default AEAD suite for new channels.
    pub const DEFAULT: AeadSuite = AeadSuite::ChaCha20Poly1305;

    /// Stable single-byte wire tag for this suite.
    pub const fn tag(self) -> u8 {
        match self {
            AeadSuite::ChaCha20Poly1305 => 0x01,
        }
    }

    /// Parse an AEAD suite from a wire tag.
    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0x01 => Ok(AeadSuite::ChaCha20Poly1305),
            other => Err(CryptoError::UnknownAeadSuite(other)),
        }
    }

    /// Key length in bytes for this AEAD.
    pub const fn key_len(self) -> usize {
        match self {
            AeadSuite::ChaCha20Poly1305 => 32,
        }
    }

    /// Nonce length in bytes for this AEAD.
    pub const fn nonce_len(self) -> usize {
        match self {
            AeadSuite::ChaCha20Poly1305 => 12,
        }
    }
}

/// A 96-bit ChaCha20-Poly1305 nonce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AeadNonce([u8; 12]);

/// An AEAD key, tagged with its suite.
///
/// This is raw symmetric key material. `Debug` redacts it and `Drop` zeroizes it
/// on a best-effort basis.
pub struct AeadKey {
    suite: AeadSuite,
    bytes: [u8; 32],
}

impl AeadNonce {
    /// Build a nonce from exact bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; 12] = bytes.try_into().map_err(|_| CryptoError::BadLength {
            expected: 12,
            got: bytes.len(),
        })?;
        Ok(AeadNonce(arr))
    }

    /// Generate a random nonce using operating-system entropy.
    pub fn generate() -> Result<Self> {
        let mut bytes = [0u8; 12];
        getrandom::getrandom(&mut bytes).map_err(|_| CryptoError::Entropy)?;
        Ok(AeadNonce(bytes))
    }

    /// Raw nonce bytes.
    pub fn to_bytes(self) -> [u8; 12] {
        self.0
    }

    /// Borrow raw nonce bytes.
    pub fn as_bytes(&self) -> &[u8; 12] {
        &self.0
    }
}

impl AeadKey {
    /// Build an AEAD key from a suite tag and exact key bytes.
    pub fn from_suite_bytes(suite: AeadSuite, bytes: &[u8]) -> Result<Self> {
        match suite {
            AeadSuite::ChaCha20Poly1305 => {
                let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadLength {
                    expected: suite.key_len(),
                    got: bytes.len(),
                })?;
                Ok(AeadKey { suite, bytes: arr })
            }
        }
    }

    /// Generate a fresh AEAD key using operating-system entropy.
    pub fn generate(suite: AeadSuite) -> Result<Self> {
        match suite {
            AeadSuite::ChaCha20Poly1305 => {
                let mut bytes = [0u8; 32];
                getrandom::getrandom(&mut bytes).map_err(|_| CryptoError::Entropy)?;
                Ok(AeadKey { suite, bytes })
            }
        }
    }

    /// The suite this key belongs to.
    pub fn suite(&self) -> AeadSuite {
        self.suite
    }

    /// Export key bytes for **secure local storage or test vectors only**.
    pub fn to_key_bytes(&self) -> [u8; 32] {
        self.bytes
    }

    /// Encrypt `plaintext` with `nonce` and authenticated associated data `aad`.
    pub fn encrypt(&self, nonce: &AeadNonce, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        match self.suite {
            AeadSuite::ChaCha20Poly1305 => {
                let cipher =
                    ChaCha20Poly1305::new_from_slice(&self.bytes).map_err(|_| CryptoError::Aead)?;
                cipher
                    .encrypt(
                        Nonce::from_slice(nonce.as_bytes()),
                        Payload {
                            msg: plaintext,
                            aad,
                        },
                    )
                    .map_err(|_| CryptoError::Aead)
            }
        }
    }

    /// Decrypt and authenticate `ciphertext` with `nonce` and associated data `aad`.
    pub fn decrypt(&self, nonce: &AeadNonce, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        match self.suite {
            AeadSuite::ChaCha20Poly1305 => {
                let cipher =
                    ChaCha20Poly1305::new_from_slice(&self.bytes).map_err(|_| CryptoError::Aead)?;
                cipher
                    .decrypt(
                        Nonce::from_slice(nonce.as_bytes()),
                        Payload {
                            msg: ciphertext,
                            aad,
                        },
                    )
                    .map_err(|_| CryptoError::Aead)
            }
        }
    }
}

impl Clone for AeadKey {
    fn clone(&self) -> Self {
        AeadKey {
            suite: self.suite,
            bytes: self.bytes,
        }
    }
}

impl PartialEq for AeadKey {
    fn eq(&self, other: &Self) -> bool {
        self.suite == other.suite && self.bytes == other.bytes
    }
}

impl Eq for AeadKey {}

impl Drop for AeadKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl core::fmt::Debug for AeadKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AeadKey")
            .field("suite", &self.suite)
            .field("secret", &"<redacted>")
            .finish()
    }
}

//! Suite-tagged HKDF for deriving session keys from Diffie-Hellman output.
//!
//! Encrypted channels derive multiple traffic keys from handshake material. This
//! module exposes HKDF-SHA256 as the first KDF suite and keeps the API small:
//! derive bytes, or derive an AEAD key directly.

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::aead::{AeadKey, AeadSuite};
use crate::agreement::SharedSecret;
use crate::error::{CryptoError, Result};

/// Maximum HKDF-SHA256 output permitted by RFC 5869: 255 * HashLen.
pub const MAX_HKDF_SHA256_OUTPUT_BYTES: usize = 255 * 32;

/// Versioned KDF suites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KdfSuite {
    /// HKDF with SHA-256 (RFC 5869).
    HkdfSha256,
}

impl KdfSuite {
    /// The current default KDF suite for new channels.
    pub const DEFAULT: KdfSuite = KdfSuite::HkdfSha256;

    /// Stable single-byte wire tag for this suite.
    pub const fn tag(self) -> u8 {
        match self {
            KdfSuite::HkdfSha256 => 0x01,
        }
    }

    /// Parse a KDF suite from a wire tag.
    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0x01 => Ok(KdfSuite::HkdfSha256),
            other => Err(CryptoError::UnknownKdfSuite(other)),
        }
    }

    /// Derive `len` bytes from input key material.
    pub fn derive_bytes(
        self,
        salt: Option<&[u8]>,
        input_key_material: &[u8],
        info: &[u8],
        len: usize,
    ) -> Result<Vec<u8>> {
        match self {
            KdfSuite::HkdfSha256 => {
                if len > MAX_HKDF_SHA256_OUTPUT_BYTES {
                    return Err(CryptoError::BadLength {
                        expected: MAX_HKDF_SHA256_OUTPUT_BYTES,
                        got: len,
                    });
                }
                let hk = Hkdf::<Sha256>::new(salt, input_key_material);
                let mut out = vec![0u8; len];
                hk.expand(info, &mut out).map_err(|_| CryptoError::Kdf)?;
                Ok(out)
            }
        }
    }

    /// Derive an AEAD key from input key material.
    pub fn derive_aead_key(
        self,
        salt: Option<&[u8]>,
        input_key_material: &[u8],
        info: &[u8],
        suite: AeadSuite,
    ) -> Result<AeadKey> {
        let mut bytes = self.derive_bytes(salt, input_key_material, info, suite.key_len())?;
        let key = AeadKey::from_suite_bytes(suite, &bytes);
        bytes.zeroize();
        key
    }

    /// Derive an AEAD key from a Diffie-Hellman shared secret.
    pub fn derive_aead_key_from_shared(
        self,
        salt: Option<&[u8]>,
        shared: &SharedSecret,
        info: &[u8],
        suite: AeadSuite,
    ) -> Result<AeadKey> {
        self.derive_aead_key(salt, shared.as_bytes(), info, suite)
    }
}

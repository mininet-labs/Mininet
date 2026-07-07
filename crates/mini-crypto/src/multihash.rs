//! Minimal, auditable multihash: `<varint code><varint length><digest>`.
//!
//! We hand-roll the tiny subset we need rather than pull a large dependency, so
//! the security-critical content-addressing path stays small and easy to review.
//! Only the algorithms in [`HashAlgorithm`] can be produced, and SHA-1 (`0x11`) is
//! rejected on decode — the structural form of SPEC-11's frozen hash-hardening
//! rule.

use crate::error::{CryptoError, Result};
use crate::hash::{HashAlgorithm, FORBIDDEN_SHA1_CODE};

/// A self-describing hash: algorithm code + length + digest bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Multihash {
    algorithm: HashAlgorithm,
    digest: Vec<u8>,
}

impl Multihash {
    /// Hash `data` with `algorithm` and wrap it as a multihash.
    pub fn of(algorithm: HashAlgorithm, data: &[u8]) -> Self {
        Multihash {
            algorithm,
            digest: algorithm.digest(data).to_vec(),
        }
    }

    /// The algorithm used.
    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    /// The raw digest bytes.
    pub fn digest(&self) -> &[u8] {
        &self.digest
    }

    /// Encode to canonical `<varint code><varint length><digest>` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.digest.len() + 4);
        write_uvarint(self.algorithm.multihash_code(), &mut out);
        write_uvarint(self.digest.len() as u64, &mut out);
        out.extend_from_slice(&self.digest);
        out
    }

    /// Decode multihash bytes, rejecting unknown or forbidden (SHA-1) codes and
    /// length mismatches.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (code, rest) = read_uvarint(bytes)?;
        let (len, rest) = read_uvarint(rest)?;
        let len = usize::try_from(len).map_err(|_| CryptoError::BadLength {
            expected: usize::MAX,
            got: rest.len(),
        })?;
        if rest.len() != len {
            return Err(CryptoError::BadLength {
                expected: len,
                got: rest.len(),
            });
        }
        if code == FORBIDDEN_SHA1_CODE {
            return Err(CryptoError::UnknownOrForbiddenHashCode(code));
        }
        let algorithm = match code {
            0x1e => HashAlgorithm::Blake3,
            0x12 => HashAlgorithm::Sha256,
            other => return Err(CryptoError::UnknownOrForbiddenHashCode(other)),
        };
        let expected = algorithm.digest_len();
        if rest.len() != expected {
            return Err(CryptoError::BadLength {
                expected,
                got: rest.len(),
            });
        }
        let parsed = Multihash {
            algorithm,
            digest: rest.to_vec(),
        };
        if parsed.to_bytes() != bytes {
            return Err(CryptoError::BadEncoding);
        }
        Ok(parsed)
    }
}

/// Write an unsigned LEB128 varint.
pub(crate) fn write_uvarint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Read an unsigned LEB128 varint, returning `(value, remaining-bytes)`.
pub(crate) fn read_uvarint(bytes: &[u8]) -> Result<(u64, &[u8])> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        if i >= 10 || shift >= 64 {
            return Err(CryptoError::BadVarint);
        }
        let payload = byte & 0x7f;
        if shift == 63 && payload > 1 {
            return Err(CryptoError::BadVarint);
        }
        value |= u64::from(payload)
            .checked_shl(shift)
            .ok_or(CryptoError::BadVarint)?;
        if byte & 0x80 == 0 {
            return Ok((value, &bytes[i + 1..]));
        }
        shift += 7;
    }
    Err(CryptoError::BadVarint)
}

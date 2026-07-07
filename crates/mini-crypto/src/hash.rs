//! Content hashing for Mininet.
//!
//! ## Frozen invariant — strong hash, never SHA-1
//!
//! SPEC-11 \[FREEZE\]: *"Canonical content addressing uses a STRONG hash
//! (SHA-256 / BLAKE3 multihash), because Git's default SHA-1 object id is
//! collision-broken."* SPEC-01 §3 echoes this for the `did:mini` identifier.
//!
//! This invariant is enforced **structurally**, not by convention: the
//! [`HashAlgorithm`] enum has no SHA-1 variant, so no caller can produce a SHA-1
//! content address through this API. The corresponding multihash code (`0x11`) is
//! likewise rejected on decode (see [`crate::multihash`]).

use blake3::Hasher as Blake3Hasher;
use sha2::{Digest, Sha256};

/// The set of hash algorithms permitted for canonical content addressing.
///
/// Deliberately small. There is **no** `Sha1` variant and there never will be:
/// SHA-1 is collision-broken and forbidden by SPEC-11's frozen hash-hardening
/// rule. Adding a weak algorithm here would be a constitution-level regression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HashAlgorithm {
    /// BLAKE3, 256-bit output. Default for new content addresses.
    Blake3,
    /// SHA2-256, 256-bit output. Retained for Git-object interop (SPEC-11's
    /// SHA-256 git-object plan) and broad ecosystem compatibility.
    Sha256,
}

impl HashAlgorithm {
    /// The unsigned-varint multihash code for this algorithm.
    ///
    /// `0x1e` = blake3, `0x12` = sha2-256 (per the multicodec table).
    pub const fn multihash_code(self) -> u64 {
        match self {
            HashAlgorithm::Blake3 => 0x1e,
            HashAlgorithm::Sha256 => 0x12,
        }
    }

    /// Digest length in bytes (both supported algorithms emit 32 bytes here).
    pub const fn digest_len(self) -> usize {
        32
    }

    /// Hash `data` with this algorithm, returning the raw 32-byte digest.
    pub fn digest(self, data: &[u8]) -> [u8; 32] {
        match self {
            HashAlgorithm::Blake3 => {
                let mut h = Blake3Hasher::new();
                h.update(data);
                *h.finalize().as_bytes()
            }
            HashAlgorithm::Sha256 => {
                let mut h = Sha256::new();
                h.update(data);
                let out = h.finalize();
                let mut digest = [0u8; 32];
                digest.copy_from_slice(&out);
                digest
            }
        }
    }
}

/// The default algorithm for *new* Mininet content addresses.
pub const DEFAULT_HASH: HashAlgorithm = HashAlgorithm::Blake3;

/// Convenience: BLAKE3-256 digest of `data`.
pub fn blake3_256(data: &[u8]) -> [u8; 32] {
    HashAlgorithm::Blake3.digest(data)
}

/// Convenience: SHA2-256 digest of `data`.
pub fn sha2_256(data: &[u8]) -> [u8; 32] {
    HashAlgorithm::Sha256.digest(data)
}

/// The multihash code for the (forbidden) SHA-1 algorithm.
///
/// Exposed only so the decoder and tests can explicitly **reject** it. There is
/// no code path in this crate that hashes with SHA-1.
pub const FORBIDDEN_SHA1_CODE: u64 = 0x11;

//! Versioned signature suites — the crypto-agility invariant.
//!
//! ## Frozen invariant — the crypto layer must stay agile
//!
//! SPEC-01 §13 \[FREEZE\]: *"the identity layer MUST remain crypto-agile — no
//! single signature algorithm is hard-wired for the life of the system. (The
//! CURRENT default suite is a TUNABLE parameter.)"*
//!
//! Every key and signature in Mininet is **tagged** with the suite that produced
//! it, so a verifier always knows which algorithm to apply. New suites — notably a
//! post-quantum suite such as ML-DSA-65 (FIPS 204), per SPEC-01 §13 and SPEC-05 —
//! are added by extending [`SignatureSuite`] *without* changing any wire format or
//! call site, because the suite tag travels with the data. Nothing in this crate
//! assumes Ed25519 is the only suite; it is merely the current default.

use crate::error::{CryptoError, Result};

/// A versioned signature suite identifier.
///
/// The single-byte [`tag`](SignatureSuite::tag) is serialised alongside keys and
/// signatures, so verifiers always know which algorithm to apply. This tag is the
/// mechanism that keeps the system migratable to post-quantum signatures over its
/// century-scale lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignatureSuite {
    /// Ed25519 (RFC 8032). The current default suite.
    Ed25519,
    // Reserved for the post-quantum migration target (FIPS 204 / ML-DSA-65),
    // wire tag 0x02. Added here when the implementation lands — no call site
    // changes, because the tag travels with every key and signature.
    //   MlDsa65,
}

impl SignatureSuite {
    /// The current default suite for *new* identities and keys.
    ///
    /// This default is a TUNABLE parameter (SPEC-01 §16): the governing population
    /// may change which suite is the default, but the *agility itself* is frozen —
    /// there must always be a migration path to another suite.
    pub const DEFAULT: SignatureSuite = SignatureSuite::Ed25519;

    /// Stable single-byte wire tag for this suite.
    pub const fn tag(self) -> u8 {
        match self {
            SignatureSuite::Ed25519 => 0x01,
            // SignatureSuite::MlDsa65 => 0x02, // reserved
        }
    }

    /// Parse a suite from its wire tag.
    pub fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            0x01 => Ok(SignatureSuite::Ed25519),
            other => Err(CryptoError::UnknownSuite(other)),
        }
    }

    /// Public-key length in bytes for this suite.
    pub const fn public_key_len(self) -> usize {
        match self {
            SignatureSuite::Ed25519 => 32,
        }
    }

    /// Signature length in bytes for this suite.
    pub const fn signature_len(self) -> usize {
        match self {
            SignatureSuite::Ed25519 => 64,
        }
    }
}

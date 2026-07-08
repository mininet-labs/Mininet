//! The elliptic-curve group [`crate::frost`] is built on, plus the hash
//! mappings the protocol needs. Same choice and same rationale as
//! `mini_value::curve` (D-0036/D-0037's precedent): `curve25519-dalek`'s
//! Ristretto construction is depended on rather than reimplemented — the
//! audited primitive-layer crate `ed25519-dalek`/`x25519-dalek` already
//! build on (D-0014) — while the FROST threshold-signature protocol on
//! top is Mininet-owned.
//!
//! This is a deliberate, small duplication of `mini_value::curve` rather
//! than a cross-crate dependency: `mini-value` (transaction privacy) and
//! `mini-treasury` (custody) are different risk domains that should stay
//! independently reviewable, the same way the whitepaper and D-0035 point
//! 5 already treat them as separate line items.
//!
//! [FREEZE reminder — see D-0037] This is a founder-overridden, AI-authored
//! prototype pending external cryptography audit. Nothing in this module
//! or the protocol built on it should be treated as production-ready.

pub use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
pub use curve25519_dalek::scalar::Scalar;

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT as BASEPOINT;

/// The group's public generator.
pub fn basepoint() -> RistrettoPoint {
    BASEPOINT
}

/// A fresh random scalar from the OS CSPRNG (two independent 32-byte draws,
/// reduced mod the group order via the wide reduction so there is no bias
/// toward the low end of the range).
pub fn random_scalar() -> crate::error::Result<Scalar> {
    let mut wide = [0u8; 64];
    let a = mini_crypto::random_32().map_err(|_| crate::error::TreasuryError::Entropy)?;
    let b = mini_crypto::random_32().map_err(|_| crate::error::TreasuryError::Entropy)?;
    wide[..32].copy_from_slice(&a);
    wide[32..].copy_from_slice(&b);
    Ok(Scalar::from_bytes_mod_order_wide(&wide))
}

/// Hash arbitrary bytes to a scalar (BLAKE3's 64-byte extendable output,
/// reduced mod the group order). Used for FROST's binding factors (`rho_i`)
/// and Schnorr challenge (`c`).
pub fn hash_to_scalar(parts: &[&[u8]]) -> Scalar {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    let mut wide = [0u8; 64];
    hasher.finalize_xof().fill(&mut wide);
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_to_scalar_is_deterministic() {
        let a = hash_to_scalar(&[b"hello", b"world"]);
        let b = hash_to_scalar(&[b"hello", b"world"]);
        assert_eq!(a, b);
    }

    #[test]
    fn hash_to_scalar_differs_for_different_input() {
        let a = hash_to_scalar(&[b"hello"]);
        let b = hash_to_scalar(&[b"world"]);
        assert_ne!(a, b);
    }

    #[test]
    fn random_scalar_calls_differ() {
        let a = random_scalar().unwrap();
        let b = random_scalar().unwrap();
        assert_ne!(a, b);
    }
}

//! The elliptic-curve group everything in [`crate::stealth_impl`] and
//! [`crate::ring_impl`] is built on, plus the two hash mappings both
//! protocols need. Per D-0036: the group arithmetic itself is
//! `curve25519-dalek`'s Ristretto construction — the same audited
//! primitive-layer crate `ed25519-dalek`/`x25519-dalek` already build on
//! (D-0014) — depended on rather than reimplemented; the stealth-address
//! and ring-signature protocols on top are Mininet-owned.
//!
//! Ristretto, not raw Edwards/Curve25519 points, is used deliberately:
//! Curve25519 has cofactor 8, so raw Edwards points admit small-subgroup
//! elements that can silently break protocols built directly on top of
//! them (a well-known source of subtle bugs in ad-hoc Ed25519-based
//! schemes). Ristretto quotients that cofactor away, giving a clean
//! prime-order group — exactly what a custom protocol like this needs.
//!
//! [FREEZE reminder — see D-0036] This is a founder-overridden, AI-authored
//! prototype pending external cryptography audit. Nothing in this module
//! or the protocols built on it should be treated as production-ready.

pub use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
pub use curve25519_dalek::scalar::Scalar;

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT as BASEPOINT;

/// The group's public generator.
pub fn basepoint() -> RistrettoPoint {
    BASEPOINT
}

/// A fresh random scalar from the OS CSPRNG
/// ([`mini_crypto::random_32`] twice, reduced mod the group order via the
/// wide reduction so there is no bias toward the low end of the range).
pub fn random_scalar() -> crate::error::Result<Scalar> {
    let mut wide = [0u8; 64];
    let a = mini_crypto::random_32().map_err(|_| crate::error::ValueError::Entropy)?;
    let b = mini_crypto::random_32().map_err(|_| crate::error::ValueError::Entropy)?;
    wide[..32].copy_from_slice(&a);
    wide[32..].copy_from_slice(&b);
    Ok(Scalar::from_bytes_mod_order_wide(&wide))
}

/// Hash arbitrary bytes to a scalar (BLAKE3's 64-byte extendable output,
/// reduced mod the group order via the wide reduction). Used for every
/// Fiat-Shamir challenge in [`crate::ring_impl`] and the shared-secret
/// scalar in [`crate::stealth_impl`].
pub fn hash_to_scalar(parts: &[&[u8]]) -> Scalar {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    let mut wide = [0u8; 64];
    hasher.finalize_xof().fill(&mut wide);
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// Hash arbitrary bytes to a group element (BLAKE3's 64-byte extendable
/// output, mapped to a uniformly random Ristretto point). Used by
/// [`crate::ring_impl`] for the key-image base point `Hp(P)`.
pub fn hash_to_point(parts: &[&[u8]]) -> RistrettoPoint {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    let mut wide = [0u8; 64];
    hasher.finalize_xof().fill(&mut wide);
    RistrettoPoint::from_uniform_bytes(&wide)
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
    fn hash_to_point_is_deterministic_and_on_the_curve() {
        let a = hash_to_point(&[b"hello"]);
        let b = hash_to_point(&[b"hello"]);
        assert_eq!(a.compress(), b.compress());
    }

    #[test]
    fn random_scalar_calls_differ() {
        let a = random_scalar().unwrap();
        let b = random_scalar().unwrap();
        assert_ne!(a, b);
    }
}

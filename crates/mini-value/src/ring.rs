//! The seam a real ring-signature scheme fills in.
//!
//! A ring signature lets one of `N` public keys sign a message while
//! proving only "one of these N authorized this," never which one — the
//! anonymity-set property Monero uses to hide which of several decoy
//! outputs a spend actually consumes. A **key image**, deterministically
//! derived from the real signer's secret key, accompanies the signature so
//! the network can detect and reject a double-spend of the same output
//! without ever learning which ring member spent it.
//!
//! [`crate::ring_impl`] is a real (MLSAG-style linkable ring signature)
//! implementation of this trait, per the founder override recorded in
//! D-0036. [`NoRingSignature`] remains available as the fail-closed
//! reference for anyone not opting into the prototype.
//!
//! ## Honest limit [D-0036]
//!
//! This is a founder-overridden, AI-authored prototype, not the human-
//! authored, externally-audited implementation D-0035 point 5 otherwise
//! requires. Getting this subtly wrong is catastrophic in either
//! direction: a flawed anonymity set can deanonymize the real signer, and
//! a flawed key-image derivation can allow double-spends or accidentally
//! link unrelated spends together. Treat
//! [`crate::ring_impl::MininetRingSignature`] as a prototype pending a
//! specialized external audit before any real value depends on it.

/// A ring signature over an anonymity-set-sized ring: one challenge value
/// (the Fiat-Shamir hash chain's anchor), one response scalar per ring
/// member, and the key image used for double-spend detection. Each field
/// is a compressed 32-byte scalar/point, opaque at this trait's level —
/// the internal meaning belongs to whatever concrete scheme implements it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingSignature {
    /// The Fiat-Shamir challenge chain's anchor value.
    pub challenge: Vec<u8>,
    /// One response scalar per ring member, same order as the ring.
    pub responses: Vec<Vec<u8>>,
    /// The key image, for double-spend detection.
    pub key_image: Vec<u8>,
}

/// A source of ring-signature signing and verification.
pub trait RingSignatureScheme {
    /// Sign `message` as one of the members of `ring` (the real signer's
    /// secret key is supplied to the implementation out-of-band, never
    /// through this trait's parameters). `None` means no real
    /// implementation is available.
    fn sign(&mut self, ring: &[Vec<u8>], message: &[u8]) -> Option<RingSignature>;

    /// Verify `signature` was produced by some member of `ring` over
    /// `message`.
    fn verify(&self, ring: &[Vec<u8>], message: &[u8], signature: &RingSignature) -> bool;
}

/// The reference [`RingSignatureScheme`]: never signs, never verifies
/// anything as valid. This is the correct, permanent behavior until the
/// human-authored, externally-audited implementation described above
/// exists — treating any signature as valid without one would be treating
/// an unproven spend as authorized.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRingSignature;

impl RingSignatureScheme for NoRingSignature {
    fn sign(&mut self, _ring: &[Vec<u8>], _message: &[u8]) -> Option<RingSignature> {
        None
    }

    fn verify(&self, _ring: &[Vec<u8>], _message: &[u8], _signature: &RingSignature) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_ring_signature_never_signs() {
        let mut scheme = NoRingSignature;
        assert_eq!(scheme.sign(&[vec![1, 2, 3]], b"message"), None);
    }

    #[test]
    fn no_ring_signature_never_verifies_anything_as_valid() {
        let scheme = NoRingSignature;
        let fake = RingSignature {
            challenge: vec![0u8; 32],
            responses: vec![vec![0u8; 32]],
            key_image: vec![0u8; 32],
        };
        assert!(!scheme.verify(&[vec![1, 2, 3]], b"message", &fake));
    }
}

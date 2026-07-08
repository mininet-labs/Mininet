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
//! ## Honest limit — do not implement this without a human cryptographer
//!
//! Getting this subtly wrong is catastrophic in either direction: a flawed
//! anonymity set can deanonymize the real signer, and a flawed key-image
//! derivation can allow double-spends or accidentally link unrelated
//! spends together. This is exactly the class of component the whitepaper
//! (§11, on treasury/bridge custody) and D-0035 point 5 require human
//! authorship and external audit for, extended here to transaction
//! privacy generally. [`NoRingSignature`] is the only implementation in
//! this repo: it signs nothing and **verifies nothing as valid**, fail-
//! closed rather than fail-open, so an absent real implementation can
//! never be mistaken for a working one.

/// A ring signature: the signature bytes plus the key image used for
/// double-spend detection. Opaque byte blobs — this crate defines no
/// internal structure for either, since that structure belongs to whatever
/// concrete scheme a human-authored implementation adopts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingSignature {
    /// Scheme-specific signature bytes.
    pub signature_bytes: Vec<u8>,
    /// Scheme-specific key image, for double-spend detection.
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
            signature_bytes: vec![0u8; 32],
            key_image: vec![0u8; 32],
        };
        assert!(!scheme.verify(&[vec![1, 2, 3]], b"message", &fake));
    }
}

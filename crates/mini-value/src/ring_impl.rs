//! A real linkable ring signature implementation of
//! [`crate::ring::RingSignatureScheme`] (a single-layer MLSAG/AOS-style
//! construction — the pre-CLSAG/Bulletproofs scheme CryptoNote/early
//! Monero used, chosen for being the simplest correctly-documented
//! linkable ring construction). Founder-overridden, AI-authored prototype
//! — see [`crate::ring`]'s honest limit and D-0036. Do not treat this as
//! production-ready.
//!
//! ## The scheme
//!
//! Given a ring of public keys `P_0, ..., P_{n-1}` and a real signer at
//! secret index `pi` with secret key `x_pi` (`P_pi = x_pi * G`), the
//! signer proves "I know the discrete log of one ring member" without
//! revealing which, via a Fiat-Shamir hash chain that closes into a loop
//! only at the real index:
//!
//! 1. Key image `I = x_pi * Hp(P_pi)` — deterministic in `x_pi`, so the
//!    same real key always produces the same `I` no matter which ring or
//!    message it signs, letting the network detect a double-spend of the
//!    same key without learning which ring member it was.
//! 2. A random nonce `alpha` seeds `L_pi = alpha*G`, `R_pi = alpha*Hp(P_pi)`,
//!    which hashes forward into the next index's challenge.
//! 3. For every other index, a random response `s_j` is chosen *first*,
//!    and `L_j = s_j*G + c_j*P_j`, `R_j = s_j*Hp(P_j) + c_j*I` are forced
//!    to be consistent with the (already-known) challenge `c_j` — these
//!    indices need no discrete-log knowledge at all.
//! 4. The chain must wrap back around to the same challenge it started
//!    from at index `pi`, which is only possible because `alpha` was
//!    chosen freely at that one index — closing it (`s_pi = alpha -
//!    c_pi * x_pi`) is exactly the one place a genuine secret key is used.
//!
//! Verification recomputes the entire hash chain from the stored anchor
//! challenge and checks it returns to the same value — a check that holds
//! regardless of which index was real, which is the anonymity property.

use crate::curve::{hash_to_point, hash_to_scalar, RistrettoPoint, Scalar};
use crate::ring::{RingSignature, RingSignatureScheme};

fn decompress_point(bytes: &[u8]) -> Option<RistrettoPoint> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    crate::curve::CompressedRistretto(arr).decompress()
}

fn decompress_scalar(bytes: &[u8]) -> Option<Scalar> {
    let arr: [u8; 32] = bytes.try_into().ok()?;
    Some(Scalar::from_bytes_mod_order(arr))
}

fn challenge_hash(
    message: &[u8],
    l: &RistrettoPoint,
    r: &RistrettoPoint,
    key_image_bytes: &[u8],
) -> Scalar {
    hash_to_scalar(&[
        message,
        l.compress().as_bytes(),
        r.compress().as_bytes(),
        key_image_bytes,
    ])
}

/// The prototype [`RingSignatureScheme`] implementation (D-0036). Holds the
/// real signer's ring position and secret key as internal state, since the
/// trait's `sign` signature carries neither (the secret is supplied
/// out-of-band, never through trait parameters — see [`RingSignatureScheme`]'s
/// own docs).
pub struct MininetRingSignature {
    secret_index: usize,
    secret_key: Scalar,
}

impl core::fmt::Debug for MininetRingSignature {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MininetRingSignature")
            .field("secret_index", &self.secret_index)
            .finish_non_exhaustive()
    }
}

impl MininetRingSignature {
    /// Build a signer for ring position `secret_index`, holding
    /// `secret_key_bytes` (a 32-byte scalar) as the real signer's secret
    /// key. `None` if `secret_key_bytes` is not 32 bytes.
    pub fn new(secret_index: usize, secret_key_bytes: &[u8]) -> Option<Self> {
        let secret_key = decompress_scalar(secret_key_bytes)?;
        Some(MininetRingSignature {
            secret_index,
            secret_key,
        })
    }
}

impl RingSignatureScheme for MininetRingSignature {
    fn sign(&mut self, ring: &[Vec<u8>], message: &[u8]) -> Option<RingSignature> {
        let n = ring.len();
        if n == 0 || self.secret_index >= n {
            return None;
        }
        let points: Vec<RistrettoPoint> = ring
            .iter()
            .map(|p| decompress_point(p))
            .collect::<Option<_>>()?;
        let pi = self.secret_index;

        let image_base = hash_to_point(&[points[pi].compress().as_bytes()]);
        let key_image = self.secret_key * image_base;
        let key_image_bytes = key_image.compress().to_bytes();

        let mut c = vec![Scalar::ZERO; n];
        let mut s = vec![Scalar::ZERO; n];

        let alpha = crate::curve::random_scalar().ok()?;
        let l_pi = alpha * crate::curve::basepoint();
        let r_pi = alpha * image_base;
        c[(pi + 1) % n] = challenge_hash(message, &l_pi, &r_pi, &key_image_bytes);

        let mut j = (pi + 1) % n;
        while j != pi {
            s[j] = crate::curve::random_scalar().ok()?;
            let hp_j = hash_to_point(&[points[j].compress().as_bytes()]);
            let l_j = s[j] * crate::curve::basepoint() + c[j] * points[j];
            let r_j = s[j] * hp_j + c[j] * key_image;
            let next = (j + 1) % n;
            c[next] = challenge_hash(message, &l_j, &r_j, &key_image_bytes);
            j = next;
        }

        s[pi] = alpha - c[pi] * self.secret_key;

        Some(RingSignature {
            challenge: c[0].to_bytes().to_vec(),
            responses: s.iter().map(|x| x.to_bytes().to_vec()).collect(),
            key_image: key_image_bytes.to_vec(),
        })
    }

    fn verify(&self, ring: &[Vec<u8>], message: &[u8], signature: &RingSignature) -> bool {
        let n = ring.len();
        if n == 0 || signature.responses.len() != n {
            return false;
        }
        let Some(points) = ring
            .iter()
            .map(|p| decompress_point(p))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let Some(key_image) = decompress_point(&signature.key_image) else {
            return false;
        };
        let Some(c0) = decompress_scalar(&signature.challenge) else {
            return false;
        };
        let Some(s) = signature
            .responses
            .iter()
            .map(|r| decompress_scalar(r))
            .collect::<Option<Vec<Scalar>>>()
        else {
            return false;
        };

        let mut c = c0;
        for j in 0..n {
            let hp_j = hash_to_point(&[points[j].compress().as_bytes()]);
            let l_j = s[j] * crate::curve::basepoint() + c * points[j];
            let r_j = s[j] * hp_j + c * key_image;
            c = challenge_hash(message, &l_j, &r_j, &signature.key_image);
        }
        c == c0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{basepoint, random_scalar};

    /// A ring of `n` random public keys, with the real signer's secret key
    /// planted at `real_index`.
    fn ring_with_real_key_at(n: usize, real_index: usize) -> (Vec<Vec<u8>>, Scalar) {
        let real_secret = random_scalar().unwrap();
        let ring: Vec<Vec<u8>> = (0..n)
            .map(|i| {
                let point = if i == real_index {
                    real_secret * basepoint()
                } else {
                    random_scalar().unwrap() * basepoint()
                };
                point.compress().to_bytes().to_vec()
            })
            .collect();
        (ring, real_secret)
    }

    #[test]
    fn a_valid_signature_verifies() {
        let (ring, secret) = ring_with_real_key_at(5, 2);
        let mut signer = MininetRingSignature::new(2, &secret.to_bytes()).unwrap();
        let sig = signer.sign(&ring, b"spend this output").unwrap();

        let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
        assert!(verifier.verify(&ring, b"spend this output", &sig));
    }

    #[test]
    fn verification_succeeds_regardless_of_which_index_was_real() {
        for real_index in 0..5 {
            let (ring, secret) = ring_with_real_key_at(5, real_index);
            let mut signer = MininetRingSignature::new(real_index, &secret.to_bytes()).unwrap();
            let sig = signer.sign(&ring, b"message").unwrap();
            let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
            assert!(
                verifier.verify(&ring, b"message", &sig),
                "failed for real_index={real_index}"
            );
        }
    }

    #[test]
    fn a_tampered_message_fails_verification() {
        let (ring, secret) = ring_with_real_key_at(4, 1);
        let mut signer = MininetRingSignature::new(1, &secret.to_bytes()).unwrap();
        let sig = signer.sign(&ring, b"original message").unwrap();

        let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
        assert!(!verifier.verify(&ring, b"different message", &sig));
    }

    #[test]
    fn a_tampered_response_fails_verification() {
        let (ring, secret) = ring_with_real_key_at(4, 1);
        let mut signer = MininetRingSignature::new(1, &secret.to_bytes()).unwrap();
        let mut sig = signer.sign(&ring, b"message").unwrap();
        sig.responses[0] = random_scalar().unwrap().to_bytes().to_vec();

        let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
        assert!(!verifier.verify(&ring, b"message", &sig));
    }

    #[test]
    fn a_swapped_decoy_fails_verification() {
        let (mut ring, secret) = ring_with_real_key_at(4, 1);
        let mut signer = MininetRingSignature::new(1, &secret.to_bytes()).unwrap();
        let sig = signer.sign(&ring, b"message").unwrap();

        // Swap out a decoy (not the real signer's own key) for an unrelated
        // point after signing.
        ring[2] = (random_scalar().unwrap() * basepoint())
            .compress()
            .to_bytes()
            .to_vec();

        let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
        assert!(!verifier.verify(&ring, b"message", &sig));
    }

    #[test]
    fn the_same_real_key_produces_the_same_key_image_across_signings() {
        let real_secret = random_scalar().unwrap();
        let real_point = real_secret * basepoint();

        // Two different rings (different decoys, different real index),
        // same real key.
        let ring_a: Vec<Vec<u8>> = vec![
            real_point.compress().to_bytes().to_vec(),
            (random_scalar().unwrap() * basepoint())
                .compress()
                .to_bytes()
                .to_vec(),
        ];
        let ring_b: Vec<Vec<u8>> = vec![
            (random_scalar().unwrap() * basepoint())
                .compress()
                .to_bytes()
                .to_vec(),
            real_point.compress().to_bytes().to_vec(),
        ];

        let mut signer_a = MininetRingSignature::new(0, &real_secret.to_bytes()).unwrap();
        let sig_a = signer_a.sign(&ring_a, b"tx a").unwrap();
        let mut signer_b = MininetRingSignature::new(1, &real_secret.to_bytes()).unwrap();
        let sig_b = signer_b.sign(&ring_b, b"tx b").unwrap();

        assert_eq!(sig_a.key_image, sig_b.key_image);
    }

    #[test]
    fn different_real_keys_produce_different_key_images() {
        let (ring_a, secret_a) = ring_with_real_key_at(3, 0);
        let (ring_b, secret_b) = ring_with_real_key_at(3, 0);
        let mut signer_a = MininetRingSignature::new(0, &secret_a.to_bytes()).unwrap();
        let mut signer_b = MininetRingSignature::new(0, &secret_b.to_bytes()).unwrap();

        let sig_a = signer_a.sign(&ring_a, b"message").unwrap();
        let sig_b = signer_b.sign(&ring_b, b"message").unwrap();
        assert_ne!(sig_a.key_image, sig_b.key_image);
    }

    #[test]
    fn signing_with_an_out_of_range_index_fails_without_panicking() {
        let (ring, secret) = ring_with_real_key_at(3, 0);
        let mut signer = MininetRingSignature::new(10, &secret.to_bytes()).unwrap();
        assert_eq!(signer.sign(&ring, b"message"), None);
    }

    #[test]
    fn signing_an_empty_ring_fails_without_panicking() {
        let mut signer = MininetRingSignature::new(0, &[1u8; 32]).unwrap();
        assert_eq!(signer.sign(&[], b"message"), None);
    }

    #[test]
    fn malformed_ring_member_fails_verification_without_panicking() {
        let (ring, secret) = ring_with_real_key_at(3, 0);
        let mut signer = MininetRingSignature::new(0, &secret.to_bytes()).unwrap();
        let sig = signer.sign(&ring, b"message").unwrap();

        let mut bad_ring = ring;
        bad_ring[1] = vec![0u8; 4]; // not a valid compressed point
        let verifier = MininetRingSignature::new(0, &[0u8; 32]).unwrap();
        assert!(!verifier.verify(&bad_ring, b"message", &sig));
    }
}

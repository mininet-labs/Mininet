//! The inner product argument (IPA): the recursive folding protocol that
//! gives Bulletproofs their logarithmic proof size. Proves knowledge of
//! vectors `a`, `b` (length a power of two) such that
//! `P = <a,G> + <b,H> + <a,b>*Q`, without revealing `a`/`b` beyond two
//! final folded scalars.
//!
//! This module is generic over what `P`, `G`, `H` represent — it doesn't
//! know it's being used for a range proof. [`crate::bp_range`] is the
//! caller that constructs the right `P`/`G`/`H'` for that purpose.
//!
//! ## Folding identity this implementation relies on
//!
//! Each round with challenge `u` replaces `(a,b,G,H)` with half-length
//! `(a',b',G',H')` via `a' = u*a_lo + u⁻¹*a_hi`, `b' = u⁻¹*b_lo + u*b_hi`,
//! `G' = u⁻¹*G_lo + u*G_hi`, `H' = u*H_lo + u⁻¹*H_hi`, and the commitment
//! updates as `P' = P + u²*L + u⁻²*R` where `L`/`R` are the cross terms
//! sent each round. This crate's docs (D-0036/D-0037 process note) record
//! that this identity was hand-derived and cross-checked term-by-term
//! before implementation, not copied from memory of the original paper
//! without verification.

use curve25519_dalek::traits::Identity;

use crate::curve::{hash_to_scalar, RistrettoPoint, Scalar};

pub(crate) fn inner_product(a: &[Scalar], b: &[Scalar]) -> Scalar {
    a.iter()
        .zip(b.iter())
        .fold(Scalar::ZERO, |acc, (x, y)| acc + x * y)
}

pub(crate) fn multiscalar_mul(scalars: &[Scalar], points: &[RistrettoPoint]) -> RistrettoPoint {
    scalars
        .iter()
        .zip(points.iter())
        .fold(RistrettoPoint::identity(), |acc, (s, p)| acc + s * p)
}

/// An inner-product proof: `log2(n)` pairs of cross-term commitments, plus
/// two final folded scalars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerProductProof {
    /// Left cross-term commitments, one per round.
    pub l_points: Vec<[u8; 32]>,
    /// Right cross-term commitments, one per round.
    pub r_points: Vec<[u8; 32]>,
    /// The final folded `a` scalar.
    pub a: [u8; 32],
    /// The final folded `b` scalar.
    pub b: [u8; 32],
}

/// Prove knowledge of `a`, `b` (both length `g.len()`, a power of two)
/// such that `<a,G> + <b,H> + <a,b>*q` equals the commitment the caller
/// is proving against. `transcript_seed` binds this proof to the calling
/// protocol's own Fiat-Shamir transcript, so an IPA proof from one
/// instance cannot be replayed into another.
pub fn prove(
    mut g: Vec<RistrettoPoint>,
    mut h: Vec<RistrettoPoint>,
    q: RistrettoPoint,
    mut a: Vec<Scalar>,
    mut b: Vec<Scalar>,
    transcript_seed: &[u8],
) -> InnerProductProof {
    let mut l_points = Vec::new();
    let mut r_points = Vec::new();
    let mut transcript = transcript_seed.to_vec();

    while g.len() > 1 {
        let n_half = g.len() / 2;
        let (a_lo, a_hi) = a.split_at(n_half);
        let (b_lo, b_hi) = b.split_at(n_half);
        let (g_lo, g_hi) = g.split_at(n_half);
        let (h_lo, h_hi) = h.split_at(n_half);

        let c_l = inner_product(a_lo, b_hi);
        let c_r = inner_product(a_hi, b_lo);

        let l = multiscalar_mul(a_lo, g_hi) + multiscalar_mul(b_hi, h_lo) + c_l * q;
        let r = multiscalar_mul(a_hi, g_lo) + multiscalar_mul(b_lo, h_hi) + c_r * q;

        let l_bytes = l.compress().to_bytes();
        let r_bytes = r.compress().to_bytes();
        transcript.extend_from_slice(&l_bytes);
        transcript.extend_from_slice(&r_bytes);
        l_points.push(l_bytes);
        r_points.push(r_bytes);

        let u = hash_to_scalar(&[&transcript]);
        let u_inv = u.invert();

        let new_a: Vec<Scalar> = (0..n_half).map(|i| a_lo[i] * u + a_hi[i] * u_inv).collect();
        let new_b: Vec<Scalar> = (0..n_half).map(|i| b_lo[i] * u_inv + b_hi[i] * u).collect();
        let new_g: Vec<RistrettoPoint> =
            (0..n_half).map(|i| g_lo[i] * u_inv + g_hi[i] * u).collect();
        let new_h: Vec<RistrettoPoint> =
            (0..n_half).map(|i| h_lo[i] * u + h_hi[i] * u_inv).collect();

        a = new_a;
        b = new_b;
        g = new_g;
        h = new_h;
    }

    InnerProductProof {
        l_points,
        r_points,
        a: a[0].to_bytes(),
        b: b[0].to_bytes(),
    }
}

/// Verify an [`InnerProductProof`] against the claimed commitment `p`
/// (already including the `<a,b>*q` term), generators `g`/`h`, and the
/// same `transcript_seed` the prover used.
pub fn verify(
    mut g: Vec<RistrettoPoint>,
    mut h: Vec<RistrettoPoint>,
    q: RistrettoPoint,
    mut p: RistrettoPoint,
    proof: &InnerProductProof,
    transcript_seed: &[u8],
) -> bool {
    if proof.l_points.len() != proof.r_points.len() {
        return false;
    }
    // g.len() must be a power of two matching the number of proof rounds.
    if g.len() != (1usize << proof.l_points.len()) {
        return false;
    }

    let mut transcript = transcript_seed.to_vec();
    let mut challenges = Vec::with_capacity(proof.l_points.len());
    for (l_bytes, r_bytes) in proof.l_points.iter().zip(&proof.r_points) {
        transcript.extend_from_slice(l_bytes);
        transcript.extend_from_slice(r_bytes);
        challenges.push(hash_to_scalar(&[&transcript]));
    }

    for (round, u) in challenges.iter().enumerate() {
        let Some(l) = crate::curve::CompressedRistretto(proof.l_points[round]).decompress() else {
            return false;
        };
        let Some(r) = crate::curve::CompressedRistretto(proof.r_points[round]).decompress() else {
            return false;
        };
        let u_inv = u.invert();
        let n_half = g.len() / 2;
        let (g_lo, g_hi) = g.split_at(n_half);
        let (h_lo, h_hi) = h.split_at(n_half);
        let new_g: Vec<RistrettoPoint> =
            (0..n_half).map(|i| g_lo[i] * u_inv + g_hi[i] * u).collect();
        let new_h: Vec<RistrettoPoint> =
            (0..n_half).map(|i| h_lo[i] * u + h_hi[i] * u_inv).collect();
        p += l * (u * u) + r * (u_inv * u_inv);
        g = new_g;
        h = new_h;
    }

    let arr_a: [u8; 32] = proof.a;
    let arr_b: [u8; 32] = proof.b;
    let a = Scalar::from_bytes_mod_order(arr_a);
    let b = Scalar::from_bytes_mod_order(arr_b);
    let expected = g[0] * a + h[0] * b + (a * b) * q;
    expected == p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{basepoint, hash_to_point, random_scalar};

    fn setup(n: usize) -> (Vec<RistrettoPoint>, Vec<RistrettoPoint>, RistrettoPoint) {
        let g: Vec<_> = (0..n)
            .map(|i| hash_to_point(&[b"ipa-test-g", &(i as u64).to_be_bytes()]))
            .collect();
        let h: Vec<_> = (0..n)
            .map(|i| hash_to_point(&[b"ipa-test-h", &(i as u64).to_be_bytes()]))
            .collect();
        (g, h, basepoint())
    }

    fn commit(
        g: &[RistrettoPoint],
        h: &[RistrettoPoint],
        q: RistrettoPoint,
        a: &[Scalar],
        b: &[Scalar],
    ) -> RistrettoPoint {
        multiscalar_mul(a, g) + multiscalar_mul(b, h) + inner_product(a, b) * q
    }

    #[test]
    fn a_valid_proof_verifies_for_various_sizes() {
        for n in [1usize, 2, 4, 8, 16, 64] {
            let (g, h, q) = setup(n);
            let a: Vec<Scalar> = (0..n).map(|_| random_scalar().unwrap()).collect();
            let b: Vec<Scalar> = (0..n).map(|_| random_scalar().unwrap()).collect();
            let p = commit(&g, &h, q, &a, &b);

            let proof = prove(g.clone(), h.clone(), q, a, b, b"test-transcript");
            assert!(
                verify(g, h, q, p, &proof, b"test-transcript"),
                "failed for n={n}"
            );
        }
    }

    #[test]
    fn a_wrong_commitment_fails_verification() {
        let (g, h, q) = setup(4);
        let a: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let b: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let p = commit(&g, &h, q, &a, &b);
        let proof = prove(g.clone(), h.clone(), q, a, b, b"transcript");

        let wrong_p = p + basepoint();
        assert!(!verify(g, h, q, wrong_p, &proof, b"transcript"));
    }

    #[test]
    fn a_tampered_final_scalar_fails_verification() {
        let (g, h, q) = setup(4);
        let a: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let b: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let p = commit(&g, &h, q, &a, &b);
        let mut proof = prove(g.clone(), h.clone(), q, a, b, b"transcript");
        proof.a = random_scalar().unwrap().to_bytes();

        assert!(!verify(g, h, q, p, &proof, b"transcript"));
    }

    #[test]
    fn a_tampered_l_point_fails_verification() {
        let (g, h, q) = setup(4);
        let a: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let b: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let p = commit(&g, &h, q, &a, &b);
        let mut proof = prove(g.clone(), h.clone(), q, a, b, b"transcript");
        proof.l_points[0] = basepoint().compress().to_bytes();

        assert!(!verify(g, h, q, p, &proof, b"transcript"));
    }

    #[test]
    fn a_different_transcript_seed_fails_verification() {
        let (g, h, q) = setup(4);
        let a: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let b: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let p = commit(&g, &h, q, &a, &b);
        let proof = prove(g.clone(), h.clone(), q, a, b, b"transcript-a");

        assert!(!verify(g, h, q, p, &proof, b"transcript-b"));
    }

    #[test]
    fn mismatched_l_r_lengths_are_rejected_without_panicking() {
        let (g, h, q) = setup(4);
        let a: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let b: Vec<Scalar> = (0..4).map(|_| random_scalar().unwrap()).collect();
        let p = commit(&g, &h, q, &a, &b);
        let mut proof = prove(g.clone(), h.clone(), q, a, b, b"transcript");
        proof.r_points.pop();

        assert!(!verify(g, h, q, p, &proof, b"transcript"));
    }
}

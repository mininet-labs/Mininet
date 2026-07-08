//! A single-value Bulletproofs range proof: given a Pedersen commitment
//! `V = blinding*G + value*H`, prove `value ∈ [0, 2^64)` without revealing
//! `value` or `blinding`, in `O(log n)` proof size via [`crate::bp_ipa`].
//!
//! ## The construction, and why each piece is there
//!
//! `value`'s bits are `a_L`; `a_R = a_L - 1` (so `a_L ∘ a_R = 0` exactly
//! when every entry of `a_L` is `0` or `1` — this is what makes the proof
//! a *range* proof rather than an unconstrained commitment opening).
//! Blinded vector commitments `A`, `S` hide `a_L`/`a_R` and randomizers
//! `s_L`/`s_R`; challenges `y`, `z` fold the bit-constraint and the
//! bit-reconstruction constraint (`<a_L, 2^n> = value`) into one
//! polynomial `t(X) = <l(X), r(X)>`; `T1`, `T2` commit to `t(X)`'s
//! coefficients; a final challenge `x` evaluates everything at one point,
//! and the inner-product argument compresses the otherwise `O(n)`-sized
//! opening of `l(x)`/`r(x)` down to `O(log n)`.
//!
//! Two identities make verification work, both hand-derived and checked
//! term-by-term before implementation (not taken on faith from memory of
//! the original paper):
//!
//! - `t(X)`'s constant term is `t0 = value*z² + delta(y,z)` for a public
//!   `delta(y,z) = (z - z²)·Σyⁱ - z³·Σ2ⁱ` — so the verifier can check
//!   `tau_x*G + t_hat*H == z²*V + delta*H + x*T1 + x²*T2` without ever
//!   learning `value`.
//! - The IPA's target commitment is
//!   `A + x*S - z*Σ Gᵢ + Σ(z*yⁱ + z²*2ⁱ)*H'ᵢ - mu*G`, where `H'ᵢ = Hᵢ*y⁻ⁱ`
//!   — the "prime" generators that let the `y`-weighted Hadamard product
//!   inside `r(X)` fold correctly into a plain inner-product argument.
//!
//! [FREEZE reminder — D-0036/D-0037] A founder-overridden, AI-authored
//! prototype pending external cryptography audit. Do not treat this as
//! production-ready.

use curve25519_dalek::traits::Identity;

use crate::bp_generators::{
    blinding_generator, g_vec, h_vec, ipa_generator, value_generator, BIT_LENGTH,
};
use crate::bp_ipa::{self, inner_product, multiscalar_mul, InnerProductProof};
use crate::curve::{hash_to_scalar, CompressedRistretto, RistrettoPoint, Scalar};
use crate::error::Result;

fn powers(base: Scalar, n: usize) -> Vec<Scalar> {
    let mut out = Vec::with_capacity(n);
    let mut current = Scalar::ONE;
    for _ in 0..n {
        out.push(current);
        current *= base;
    }
    out
}

/// A Bulletproofs range proof for one value committed elsewhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeProof {
    a: [u8; 32],
    s: [u8; 32],
    t1: [u8; 32],
    t2: [u8; 32],
    tau_x: [u8; 32],
    mu: [u8; 32],
    t_hat: [u8; 32],
    ipa: InnerProductProof,
}

/// Commit to `value` with `blinding`, and prove `value ∈ [0, 2^64)`.
/// Returns the compressed commitment and the proof. `None` only on a
/// local CSPRNG failure.
pub fn prove_range(value: u64, blinding: Scalar) -> Result<([u8; 32], RangeProof)> {
    let n = BIT_LENGTH;
    let g_blind = blinding_generator();
    let h_val = value_generator();
    let q = ipa_generator();
    let g = g_vec();
    let h = h_vec();

    let v_point = blinding * g_blind + Scalar::from(value) * h_val;
    let v_bytes = v_point.compress().to_bytes();

    let a_l: Vec<Scalar> = (0..n)
        .map(|i| {
            if (value >> i) & 1 == 1 {
                Scalar::ONE
            } else {
                Scalar::ZERO
            }
        })
        .collect();
    let a_r: Vec<Scalar> = a_l.iter().map(|bit| bit - Scalar::ONE).collect();

    let alpha = crate::curve::random_scalar()?;
    let a_commit = alpha * g_blind + multiscalar_mul(&a_l, &g) + multiscalar_mul(&a_r, &h);

    let s_l: Vec<Scalar> = (0..n)
        .map(|_| crate::curve::random_scalar())
        .collect::<Result<_>>()?;
    let s_r: Vec<Scalar> = (0..n)
        .map(|_| crate::curve::random_scalar())
        .collect::<Result<_>>()?;
    let rho = crate::curve::random_scalar()?;
    let s_commit = rho * g_blind + multiscalar_mul(&s_l, &g) + multiscalar_mul(&s_r, &h);

    let mut transcript = Vec::new();
    transcript.extend_from_slice(&v_bytes);
    transcript.extend_from_slice(a_commit.compress().as_bytes());
    transcript.extend_from_slice(s_commit.compress().as_bytes());
    let y = hash_to_scalar(&[&transcript, b"y"]);
    transcript.extend_from_slice(&y.to_bytes());
    let z = hash_to_scalar(&[&transcript, b"z"]);
    transcript.extend_from_slice(&z.to_bytes());

    let y_pow = powers(y, n);
    let two_pow = powers(Scalar::from(2u64), n);
    let z_sq = z * z;

    let l0: Vec<Scalar> = (0..n).map(|i| a_l[i] - z).collect();
    let r0: Vec<Scalar> = (0..n)
        .map(|i| y_pow[i] * (a_r[i] + z) + z_sq * two_pow[i])
        .collect();
    let l1 = s_l;
    let r1: Vec<Scalar> = (0..n).map(|i| y_pow[i] * s_r[i]).collect();

    let t1 = inner_product(&l0, &r1) + inner_product(&l1, &r0);
    let t2 = inner_product(&l1, &r1);

    let tau1 = crate::curve::random_scalar()?;
    let tau2 = crate::curve::random_scalar()?;
    let t1_commit = tau1 * g_blind + t1 * h_val;
    let t2_commit = tau2 * g_blind + t2 * h_val;

    transcript.extend_from_slice(t1_commit.compress().as_bytes());
    transcript.extend_from_slice(t2_commit.compress().as_bytes());
    let x = hash_to_scalar(&[&transcript, b"x"]);
    transcript.extend_from_slice(&x.to_bytes());

    let l: Vec<Scalar> = (0..n).map(|i| l0[i] + x * l1[i]).collect();
    let r: Vec<Scalar> = (0..n).map(|i| r0[i] + x * r1[i]).collect();
    let t_hat = inner_product(&l, &r);
    let tau_x = tau2 * x * x + tau1 * x + z_sq * blinding;
    let mu = alpha + rho * x;

    let y_inv_pow = powers(y.invert(), n);
    let h_prime: Vec<RistrettoPoint> = (0..n).map(|i| h[i] * y_inv_pow[i]).collect();

    transcript.extend_from_slice(&t_hat.to_bytes());
    transcript.extend_from_slice(&tau_x.to_bytes());
    transcript.extend_from_slice(&mu.to_bytes());
    let ipa = bp_ipa::prove(g, h_prime, q, l, r, &transcript);

    Ok((
        v_bytes,
        RangeProof {
            a: a_commit.compress().to_bytes(),
            s: s_commit.compress().to_bytes(),
            t1: t1_commit.compress().to_bytes(),
            t2: t2_commit.compress().to_bytes(),
            tau_x: tau_x.to_bytes(),
            mu: mu.to_bytes(),
            t_hat: t_hat.to_bytes(),
            ipa,
        },
    ))
}

/// Verify a [`RangeProof`] against a compressed commitment.
pub fn verify_range(commitment: [u8; 32], proof: &RangeProof) -> bool {
    let n = BIT_LENGTH;
    let g_blind = blinding_generator();
    let h_val = value_generator();
    let q = ipa_generator();
    let g = g_vec();
    let h = h_vec();

    let Some(v_point) = CompressedRistretto(commitment).decompress() else {
        return false;
    };
    let Some(a_commit) = CompressedRistretto(proof.a).decompress() else {
        return false;
    };
    let Some(s_commit) = CompressedRistretto(proof.s).decompress() else {
        return false;
    };
    let Some(t1_commit) = CompressedRistretto(proof.t1).decompress() else {
        return false;
    };
    let Some(t2_commit) = CompressedRistretto(proof.t2).decompress() else {
        return false;
    };

    let mut transcript = Vec::new();
    transcript.extend_from_slice(&commitment);
    transcript.extend_from_slice(&proof.a);
    transcript.extend_from_slice(&proof.s);
    let y = hash_to_scalar(&[&transcript, b"y"]);
    transcript.extend_from_slice(&y.to_bytes());
    let z = hash_to_scalar(&[&transcript, b"z"]);
    transcript.extend_from_slice(&z.to_bytes());

    transcript.extend_from_slice(&proof.t1);
    transcript.extend_from_slice(&proof.t2);
    let x = hash_to_scalar(&[&transcript, b"x"]);
    transcript.extend_from_slice(&x.to_bytes());

    let t_hat = Scalar::from_bytes_mod_order(proof.t_hat);
    let tau_x = Scalar::from_bytes_mod_order(proof.tau_x);
    let mu = Scalar::from_bytes_mod_order(proof.mu);

    let y_pow = powers(y, n);
    let two_pow = powers(Scalar::from(2u64), n);
    let z_sq = z * z;
    let sum_y = y_pow.iter().fold(Scalar::ZERO, |acc, v| acc + v);
    let sum_2 = two_pow.iter().fold(Scalar::ZERO, |acc, v| acc + v);
    let delta = (z - z_sq) * sum_y - z * z_sq * sum_2;

    let lhs = tau_x * g_blind + t_hat * h_val;
    let rhs = z_sq * v_point + delta * h_val + x * t1_commit + x * x * t2_commit;
    if lhs != rhs {
        return false;
    }

    let y_inv_pow = powers(y.invert(), n);
    let h_prime: Vec<RistrettoPoint> = (0..n).map(|i| h[i] * y_inv_pow[i]).collect();

    let sum_g = g
        .iter()
        .fold(RistrettoPoint::identity(), |acc, gi| acc + gi);
    let mut p_ipa = a_commit + x * s_commit - z * sum_g;
    for i in 0..n {
        p_ipa += (z * y_pow[i] + z_sq * two_pow[i]) * h_prime[i];
    }
    p_ipa -= mu * g_blind;
    let ipa_target = p_ipa + t_hat * q;

    transcript.extend_from_slice(&proof.t_hat);
    transcript.extend_from_slice(&proof.tau_x);
    transcript.extend_from_slice(&proof.mu);

    bp_ipa::verify(g, h_prime, q, ipa_target, &proof.ipa, &transcript)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_proof_verifies_for_various_values() {
        for value in [0u64, 1, 2, 42, 1_000_000, u32::MAX as u64, u64::MAX] {
            let blinding = crate::curve::random_scalar().unwrap();
            let (commitment, proof) = prove_range(value, blinding).unwrap();
            assert!(
                verify_range(commitment, &proof),
                "failed to verify value={value}"
            );
        }
    }

    #[test]
    fn a_wrong_commitment_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (_, proof) = prove_range(100, blinding).unwrap();
        let (other_commitment, _) = prove_range(200, blinding).unwrap();
        assert!(!verify_range(other_commitment, &proof));
    }

    #[test]
    fn a_tampered_t_hat_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.t_hat = crate::curve::random_scalar().unwrap().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn a_tampered_tau_x_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.tau_x = crate::curve::random_scalar().unwrap().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn a_tampered_mu_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.mu = crate::curve::random_scalar().unwrap().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn a_tampered_a_commitment_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.a = crate::curve::basepoint().compress().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn a_tampered_t1_or_t2_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.t2 = crate::curve::basepoint().compress().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn a_tampered_ipa_component_fails_verification() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, mut proof) = prove_range(100, blinding).unwrap();
        proof.ipa.a = crate::curve::random_scalar().unwrap().to_bytes();
        assert!(!verify_range(commitment, &proof));
    }

    #[test]
    fn malformed_commitment_bytes_are_rejected_without_panicking() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (_, proof) = prove_range(100, blinding).unwrap();
        assert!(!verify_range([0xffu8; 32], &proof));
    }

    #[test]
    fn different_blindings_for_the_same_value_produce_unlinkable_commitments() {
        let a = crate::curve::random_scalar().unwrap();
        let b = crate::curve::random_scalar().unwrap();
        let (commit_a, proof_a) = prove_range(500, a).unwrap();
        let (commit_b, proof_b) = prove_range(500, b).unwrap();
        assert_ne!(commit_a, commit_b);
        assert!(verify_range(commit_a, &proof_a));
        assert!(verify_range(commit_b, &proof_b));
    }

    #[test]
    fn commitments_are_additively_homomorphic() {
        // Sanity check that the underlying Pedersen commitment really is
        // homomorphic -- the property mini-value::confidential's
        // verify_balance relies on: C(v1,b1) + C(v2,b2) == C(v1+v2, b1+b2).
        let b1 = crate::curve::random_scalar().unwrap();
        let b2 = crate::curve::random_scalar().unwrap();
        let (c1, _) = prove_range(30, b1).unwrap();
        let (c2, _) = prove_range(12, b2).unwrap();
        let p1 = CompressedRistretto(c1).decompress().unwrap();
        let p2 = CompressedRistretto(c2).decompress().unwrap();
        let g_blind = blinding_generator();
        let h_val = value_generator();
        let expected_sum = (b1 + b2) * g_blind + Scalar::from(42u64) * h_val;
        assert_eq!((p1 + p2).compress(), expected_sum.compress());
    }
}

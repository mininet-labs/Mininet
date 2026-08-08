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

/// Rounds in the inner-product argument for [`BIT_LENGTH`] bits: one per
/// halving, so `log2(64) = 6`.
pub const IPA_ROUNDS: usize = BIT_LENGTH.trailing_zeros() as usize;

/// Exact wire size of a [`RangeProof`]: seven 32-byte field elements, then
/// `IPA_ROUNDS` L points, `IPA_ROUNDS` R points, and the two folded
/// scalars. Fixed-width throughout — a range proof for a 64-bit value has
/// exactly one length, so the encoding carries no length prefixes and a
/// decoder needs no bound beyond this constant.
pub const RANGE_PROOF_BYTES: usize = 32 * (7 + 2 * IPA_ROUNDS + 2);

impl RangeProof {
    /// Canonical fixed-width encoding, exactly [`RANGE_PROOF_BYTES`] long.
    ///
    /// A proof that cannot cross a wire cannot hide an amount from anyone
    /// but its author, so this is load-bearing rather than a convenience:
    /// without it `ConfidentialAmountScheme` can only be used inside one
    /// process.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(RANGE_PROOF_BYTES);
        for field in [
            &self.a,
            &self.s,
            &self.t1,
            &self.t2,
            &self.tau_x,
            &self.mu,
            &self.t_hat,
        ] {
            out.extend_from_slice(field);
        }
        // Round counts are fixed by BIT_LENGTH, so they are not encoded;
        // a decoder that disagrees about the round count would disagree
        // about the whole proof system, not about this one message.
        for point in &self.ipa.l_points {
            out.extend_from_slice(point);
        }
        for point in &self.ipa.r_points {
            out.extend_from_slice(point);
        }
        out.extend_from_slice(&self.ipa.a);
        out.extend_from_slice(&self.ipa.b);
        debug_assert_eq!(out.len(), RANGE_PROOF_BYTES);
        out
    }

    /// Decode a [`RangeProof`] from exactly [`RANGE_PROOF_BYTES`] bytes.
    ///
    /// Rejects any other length outright rather than accepting a prefix:
    /// a short read here would silently verify a different proof than the
    /// one the sender wrote. Decoding validates length and nothing else —
    /// a well-formed proof is not a *valid* one, and
    /// [`verify_range`] remains the only thing that decides that.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != RANGE_PROOF_BYTES {
            return None;
        }
        let mut cursor = 0usize;
        let mut take = || {
            let field: [u8; 32] = bytes[cursor..cursor + 32]
                .try_into()
                .expect("checked length");
            cursor += 32;
            field
        };
        let (a, s, t1, t2) = (take(), take(), take(), take());
        let (tau_x, mu, t_hat) = (take(), take(), take());
        let l_points = (0..IPA_ROUNDS).map(|_| take()).collect();
        let r_points = (0..IPA_ROUNDS).map(|_| take()).collect();
        let (ipa_a, ipa_b) = (take(), take());
        Some(Self {
            a,
            s,
            t1,
            t2,
            tau_x,
            mu,
            t_hat,
            ipa: InnerProductProof {
                l_points,
                r_points,
                a: ipa_a,
                b: ipa_b,
            },
        })
    }
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

    #[test]
    fn a_range_proof_survives_a_wire_round_trip_and_still_verifies() {
        // A proof that cannot be encoded cannot hide an amount from anyone
        // but its author -- the whole confidential-amount scheme is
        // single-process until this holds.
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, proof) = prove_range(7_777, blinding).unwrap();
        let bytes = proof.to_bytes();
        assert_eq!(bytes.len(), RANGE_PROOF_BYTES);
        let decoded = RangeProof::from_bytes(&bytes).expect("well-formed");
        assert_eq!(decoded, proof);
        assert!(verify_range(commitment, &decoded));
    }

    #[test]
    fn a_range_proof_of_the_wrong_length_is_refused_rather_than_truncated() {
        let blinding = crate::curve::random_scalar().unwrap();
        let (_, proof) = prove_range(1, blinding).unwrap();
        let bytes = proof.to_bytes();
        assert!(RangeProof::from_bytes(&bytes[..bytes.len() - 1]).is_none());
        assert!(RangeProof::from_bytes(&[bytes.clone(), vec![0]].concat()).is_none());
        assert!(RangeProof::from_bytes(&[]).is_none());
    }

    #[test]
    fn a_decodable_proof_is_not_necessarily_a_valid_one() {
        // from_bytes checks length and nothing else, on purpose: a decoder
        // that also verified would make "well-formed" and "true" the same
        // word, and every caller would stop checking the second one.
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, proof) = prove_range(9, blinding).unwrap();
        let mut bytes = proof.to_bytes();
        bytes[0] ^= 0x01;
        let tampered = RangeProof::from_bytes(&bytes).expect("still well-formed");
        assert!(!verify_range(commitment, &tampered));
    }

    #[test]
    fn the_encoding_is_fixed_width_for_every_value() {
        // No length prefixes anywhere: a 64-bit range proof has exactly one
        // size, so nothing in the encoding is attacker-steerable.
        for value in [0u64, 1, 4_294_967_296, u64::MAX] {
            let blinding = crate::curve::random_scalar().unwrap();
            let (_, proof) = prove_range(value, blinding).unwrap();
            assert_eq!(proof.to_bytes().len(), RANGE_PROOF_BYTES);
        }
    }
}

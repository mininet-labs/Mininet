//! A real (Bulletproofs) [`crate::confidential::ConfidentialAmountScheme`]
//! implementation. Founder-overridden, AI-authored prototype — see
//! [`crate::confidential`]'s honest limit and D-0036/D-0037. Do not treat
//! this as production-ready.
//!
//! The actual range-proof math lives in [`crate::bp_range`]/[`crate::bp_ipa`]/
//! [`crate::bp_generators`]; this module is the thin adapter to the
//! [`crate::confidential::ConfidentialAmountScheme`] trait, plus the
//! balance check: Pedersen commitments are additively homomorphic
//! (`C(v1,b1) + C(v2,b2) == C(v1+v2, b1+b2)`), so verifying inputs balance
//! outputs is exactly checking the summed commitment points are equal —
//! no separate proof needed for that part.

use curve25519_dalek::traits::Identity;

use crate::bp_range::{self, RangeProof};
use crate::confidential::ConfidentialAmountScheme;
use crate::curve::{CompressedRistretto, RistrettoPoint, Scalar};

/// The prototype [`ConfidentialAmountScheme`] implementation (D-0036/D-0037).
#[derive(Debug, Clone, Copy, Default)]
pub struct MininetConfidentialAmount;

impl ConfidentialAmountScheme for MininetConfidentialAmount {
    fn commit_with_proof(
        &mut self,
        amount: u64,
        blinding_factor: &[u8],
    ) -> Option<(Vec<u8>, RangeProof)> {
        let arr: [u8; 32] = blinding_factor.try_into().ok()?;
        let blinding = Scalar::from_bytes_mod_order(arr);
        let (commitment, proof) = bp_range::prove_range(amount, blinding).ok()?;
        Some((commitment.to_vec(), proof))
    }

    fn verify_range_proof(&self, commitment: &[u8], proof: &RangeProof) -> bool {
        let Ok(arr) = <[u8; 32]>::try_from(commitment) else {
            return false;
        };
        bp_range::verify_range(arr, proof)
    }

    fn verify_balance(
        &self,
        input_commitments: &[Vec<u8>],
        output_commitments: &[Vec<u8>],
    ) -> bool {
        let (Some(sum_in), Some(sum_out)) = (
            sum_commitments(input_commitments),
            sum_commitments(output_commitments),
        ) else {
            return false;
        };
        sum_in == sum_out
    }
}

/// Sum a list of compressed commitment points, `None` if any is malformed.
/// An empty list sums to the identity, so `verify_balance(&[], &[])` is
/// `true` — vacuously balanced.
fn sum_commitments(commitments: &[Vec<u8>]) -> Option<RistrettoPoint> {
    let mut sum = RistrettoPoint::identity();
    for c in commitments {
        let arr: [u8; 32] = c.as_slice().try_into().ok()?;
        let point = CompressedRistretto(arr).decompress()?;
        sum += point;
    }
    Some(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_committed_amount_verifies_its_own_range_proof() {
        let mut scheme = MininetConfidentialAmount;
        let blinding = crate::curve::random_scalar().unwrap().to_bytes();
        let (commitment, proof) = scheme.commit_with_proof(1_000, &blinding).unwrap();
        assert!(scheme.verify_range_proof(&commitment, &proof));
    }

    #[test]
    fn malformed_blinding_factor_is_rejected_without_panicking() {
        let mut scheme = MininetConfidentialAmount;
        assert_eq!(scheme.commit_with_proof(100, b"too-short"), None);
    }

    #[test]
    fn malformed_commitment_bytes_fail_verification_without_panicking() {
        let mut scheme = MininetConfidentialAmount;
        let blinding = crate::curve::random_scalar().unwrap().to_bytes();
        let (_, proof) = scheme.commit_with_proof(100, &blinding).unwrap();
        assert!(!scheme.verify_range_proof(b"not-a-valid-commitment", &proof));
    }

    #[test]
    fn balanced_inputs_and_outputs_verify() {
        let mut scheme = MininetConfidentialAmount;
        let b_in1 = crate::curve::random_scalar().unwrap();
        let b_in2 = crate::curve::random_scalar().unwrap();
        let b_out = b_in1 + b_in2; // blinding factors must also balance
        let (in1, _) = scheme.commit_with_proof(30, &b_in1.to_bytes()).unwrap();
        let (in2, _) = scheme.commit_with_proof(12, &b_in2.to_bytes()).unwrap();
        let (out1, _) = scheme.commit_with_proof(42, &b_out.to_bytes()).unwrap();

        assert!(scheme.verify_balance(&[in1, in2], &[out1]));
    }

    #[test]
    fn unbalanced_inputs_and_outputs_fail_verification() {
        let mut scheme = MininetConfidentialAmount;
        let b_in = crate::curve::random_scalar().unwrap();
        let b_out = crate::curve::random_scalar().unwrap(); // unrelated blinding
        let (input, _) = scheme.commit_with_proof(50, &b_in.to_bytes()).unwrap();
        // Same claimed amount, but unrelated blinding -> different point,
        // and even a genuinely different amount would also fail.
        let (output, _) = scheme.commit_with_proof(50, &b_out.to_bytes()).unwrap();

        assert!(!scheme.verify_balance(&[input], &[output]));
    }

    #[test]
    fn empty_inputs_and_outputs_are_vacuously_balanced() {
        let scheme = MininetConfidentialAmount;
        assert!(scheme.verify_balance(&[], &[]));
    }

    #[test]
    fn malformed_commitment_in_balance_check_fails_without_panicking() {
        let scheme = MininetConfidentialAmount;
        assert!(!scheme.verify_balance(&[vec![0u8; 4]], &[vec![0u8; 32]]));
    }
}

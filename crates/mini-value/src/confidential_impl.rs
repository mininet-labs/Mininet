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

/// The Pedersen commitment `b·G_blind + v·H_val`, with **no range proof**.
///
/// The commitment is a public value: anyone holding `(v, b)` computes the
/// same point, which is what lets a recipient check that the opening sealed
/// in their memo really opens the output they were paid. Producing one is a
/// pair of scalar multiplications; producing the accompanying Bulletproof is
/// several orders of magnitude more work, and there are callers that need
/// only the point.
///
/// **A commitment on its own proves nothing.** It does not show the
/// committed value is in range, so anything entering a claim as an *output*
/// still needs [`ConfidentialAmountScheme::commit_with_proof`] — a bare
/// commitment there would let a "negative" amount balance the equation while
/// minting value. Use this only where the proof is genuinely not part of
/// what is being checked.
pub fn pedersen_commitment(amount: u64, blinding_factor: &[u8]) -> Option<[u8; 32]> {
    let arr: [u8; 32] = blinding_factor.try_into().ok()?;
    let blinding = Scalar::from_bytes_mod_order(arr);
    let point = blinding * crate::bp_generators::blinding_generator()
        + Scalar::from(amount) * crate::bp_generators::value_generator();
    Some(point.compress().to_bytes())
}

/// A commitment to a **publicly known** amount: `amount · H_val`, with a
/// zero blinding factor.
///
/// Hiding is the point of every other commitment in this module; this one
/// deliberately hides nothing, because it commits to a value everyone can
/// already read off the wire. Its use is the transaction fee: a fee must
/// enter the balance equation as a commitment so the sums line up, and it
/// must be publicly checkable so a verifier can confirm the fee actually
/// charged is the fee declared. A blinded fee would be a fee nobody could
/// audit.
pub fn public_amount_commitment(amount: u64) -> [u8; 32] {
    (Scalar::from(amount) * crate::bp_generators::value_generator())
        .compress()
        .to_bytes()
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
    fn a_bare_commitment_is_the_same_point_the_proving_path_produces() {
        // The property that makes `pedersen_commitment` safe to use in place
        // of the proving path wherever the proof is not what is being
        // checked. If these ever diverged, every caller that mixes the two
        // -- a ledger holding bare commitments, a claim proving new ones --
        // would compute a balance that does not cancel, and the failure
        // would look like unbalanced amounts rather than like this.
        let mut scheme = MininetConfidentialAmount;
        for amount in [0u64, 1, 1_000, u64::MAX] {
            let blinding = crate::curve::random_scalar().unwrap().to_bytes();
            let (proven, _) = scheme.commit_with_proof(amount, &blinding).unwrap();
            let bare = pedersen_commitment(amount, &blinding).unwrap();
            assert_eq!(proven.as_slice(), &bare[..], "amount {amount}");
        }
    }

    #[test]
    fn a_bare_commitment_rejects_a_malformed_blinding_factor() {
        assert_eq!(pedersen_commitment(1, b"too-short"), None);
    }

    #[test]
    fn bare_commitments_balance_exactly_as_proven_ones_do() {
        let scheme = MininetConfidentialAmount;
        let b_in1 = crate::curve::random_scalar().unwrap();
        let b_in2 = crate::curve::random_scalar().unwrap();
        let b_out = b_in1 + b_in2;
        let in1 = pedersen_commitment(30, &b_in1.to_bytes()).unwrap();
        let in2 = pedersen_commitment(12, &b_in2.to_bytes()).unwrap();
        let out = pedersen_commitment(42, &b_out.to_bytes()).unwrap();
        assert!(scheme.verify_balance(&[in1.to_vec(), in2.to_vec()], &[out.to_vec()]));
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

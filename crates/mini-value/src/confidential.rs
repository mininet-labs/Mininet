//! Confidential amounts: hiding how much a transaction moves while still
//! letting the network verify no value was created from nothing. Amounts
//! are hidden inside Pedersen commitments (additively homomorphic, so
//! committed values can be summed/compared without being revealed),
//! accompanied by a [`crate::bp_range::RangeProof`] that each hidden
//! amount is non-negative and within bounds — without a range proof, a
//! hidden "negative" amount could mint value out of thin air.
//!
//! [`crate::confidential_impl::MininetConfidentialAmount`] is a real
//! (Bulletproofs) implementation of this trait, per the founder override
//! recorded in D-0036/D-0037. [`NoConfidentialAmount`] remains available
//! as the fail-closed reference for anyone not opting into the prototype.
//!
//! ## Honest limit [D-0036/D-0037]
//!
//! Range-proof soundness is exactly the kind of property that is either
//! provably correct or silently exploitable, with no safe middle ground —
//! in the whitepaper's own words about the consensus layer, "the most
//! demanding engineering in the value layer." Treat
//! [`crate::confidential_impl::MininetConfidentialAmount`] as a founder-
//! reviewed prototype pending a specialized external audit before any
//! real value depends on it.

use crate::bp_range::RangeProof;

/// A source of confidential-amount commitment and verification.
pub trait ConfidentialAmountScheme {
    /// Commit to `amount`, blinded by `blinding_factor` (a 32-byte scalar)
    /// so the commitment reveals nothing about `amount` on its own, and
    /// produce a range proof that the committed amount lies in
    /// `[0, 2^64)`. `None` means no real implementation is available, or
    /// `blinding_factor` was malformed.
    fn commit_with_proof(
        &mut self,
        amount: u64,
        blinding_factor: &[u8],
    ) -> Option<(Vec<u8>, RangeProof)>;

    /// Verify `proof` shows `commitment` hides a non-negative, in-bounds
    /// amount.
    fn verify_range_proof(&self, commitment: &[u8], proof: &RangeProof) -> bool;

    /// Verify that the sum of `input_commitments` equals the sum of
    /// `output_commitments` — the homomorphic balance check that value was
    /// conserved, without revealing any individual amount.
    fn verify_balance(&self, input_commitments: &[Vec<u8>], output_commitments: &[Vec<u8>])
        -> bool;
}

/// The reference [`ConfidentialAmountScheme`]: never commits, never
/// verifies a range proof or balance as valid. Correct, permanent behavior
/// for anyone not opting into the D-0036/D-0037 prototype — accepting an
/// unproven balance claim would be trusting that value was conserved with
/// no evidence at all.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConfidentialAmount;

impl ConfidentialAmountScheme for NoConfidentialAmount {
    fn commit_with_proof(
        &mut self,
        _amount: u64,
        _blinding_factor: &[u8],
    ) -> Option<(Vec<u8>, RangeProof)> {
        None
    }

    fn verify_range_proof(&self, _commitment: &[u8], _proof: &RangeProof) -> bool {
        false
    }

    fn verify_balance(
        &self,
        _input_commitments: &[Vec<u8>],
        _output_commitments: &[Vec<u8>],
    ) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_confidential_amount_never_commits() {
        let mut scheme = NoConfidentialAmount;
        assert_eq!(scheme.commit_with_proof(100, &[0u8; 32]), None);
    }

    #[test]
    fn no_confidential_amount_never_verifies_a_real_range_proof() {
        let scheme = NoConfidentialAmount;
        let blinding = crate::curve::random_scalar().unwrap();
        let (commitment, proof) = crate::bp_range::prove_range(100, blinding).unwrap();
        assert!(!scheme.verify_range_proof(&commitment, &proof));
    }

    #[test]
    fn no_confidential_amount_never_verifies_balance() {
        let scheme = NoConfidentialAmount;
        assert!(!scheme.verify_balance(&[vec![1, 2, 3]], &[vec![1, 2, 3]]));
    }
}

//! The seam a real confidential-amount scheme (RingCT-style) fills in.
//!
//! A confidential amount hides how much a transaction moves while still
//! letting the network verify no value was created from nothing: amounts
//! are hidden inside homomorphic commitments (so committed values can be
//! added/compared without being revealed) accompanied by a range proof
//! that each hidden amount is non-negative and within bounds — without a
//! range proof, a hidden "negative" amount could mint value out of thin
//! air.
//!
//! ## Honest limit — do not implement this without a human cryptographer
//!
//! This is, in the whitepaper's own words about the consensus layer,
//! "the most demanding engineering in the value layer" — range-proof
//! soundness is exactly the kind of property that is either provably
//! correct or silently exploitable, with no safe middle ground. D-0035
//! point 5 applies here in full. [`NoConfidentialAmount`] is the only
//! implementation here: it commits to nothing and verifies nothing as
//! valid, fail-closed — accepting an unproven balance claim would be
//! trusting that value was conserved with no evidence at all.

/// A source of confidential-amount commitment and verification.
pub trait ConfidentialAmountScheme {
    /// Commit to `amount`, blinded by `blinding_factor` so the commitment
    /// reveals nothing about `amount` on its own. `None` means no real
    /// implementation is available.
    fn commit(&self, amount: u64, blinding_factor: &[u8]) -> Option<Vec<u8>>;

    /// Verify `range_proof` shows `commitment` hides a non-negative,
    /// in-bounds amount.
    fn verify_range_proof(&self, commitment: &[u8], range_proof: &[u8]) -> bool;

    /// Verify that the sum of `input_commitments` equals the sum of
    /// `output_commitments` — the homomorphic balance check that value was
    /// conserved, without revealing any individual amount.
    fn verify_balance(&self, input_commitments: &[Vec<u8>], output_commitments: &[Vec<u8>])
        -> bool;
}

/// The reference [`ConfidentialAmountScheme`]: never commits, never
/// verifies a range proof or balance as valid. Correct, permanent behavior
/// until the human-authored, externally-audited implementation described
/// above exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoConfidentialAmount;

impl ConfidentialAmountScheme for NoConfidentialAmount {
    fn commit(&self, _amount: u64, _blinding_factor: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn verify_range_proof(&self, _commitment: &[u8], _range_proof: &[u8]) -> bool {
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
        let scheme = NoConfidentialAmount;
        assert_eq!(scheme.commit(100, b"blinding"), None);
    }

    #[test]
    fn no_confidential_amount_never_verifies_a_range_proof() {
        let scheme = NoConfidentialAmount;
        assert!(!scheme.verify_range_proof(b"commitment", b"proof"));
    }

    #[test]
    fn no_confidential_amount_never_verifies_balance() {
        let scheme = NoConfidentialAmount;
        assert!(!scheme.verify_balance(&[vec![1, 2, 3]], &[vec![1, 2, 3]]));
    }
}

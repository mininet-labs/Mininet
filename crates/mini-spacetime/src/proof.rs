//! The seam a proof-of-space-time protocol fills in — challenge-response
//! evidence that a node genuinely holds `capacity_units` of data across a
//! span of time, not just at one instant.
//!
//! [`crate::storage_proof::MerkleStorageProof`] is a real (Merkle/PDP-
//! style) implementation, per D-0037/D-0038's founder direction: start
//! with the simpler, well-documented construction now, treat full
//! proof-of-replication (the stronger guarantee the whitepaper's
//! egalitarian "thousand cheap machines beat one warehouse" thesis
//! actually needs) as a separate, later, dedicated project. See that
//! module's own honest limit for exactly what this interim scheme does
//! and does not defend against. [`NoProof`] remains available as the
//! fail-closed reference for anyone not opting into the interim scheme.

use crate::storage_proof::ProvenCapacity;

/// A source of proof-of-space-time evidence for one identity's committed
/// storage.
pub trait ProofOfSpaceTimeSource {
    /// This identity's currently-proven capacity at `now_ms`. `None` means
    /// no valid proof is currently held — a normal, unremarkable outcome
    /// for a node that hasn't completed a challenge-response round yet, or
    /// whose proof window has lapsed.
    ///
    /// Returns a [`ProvenCapacity`] rather than a `u64` so an implementation
    /// cannot hand a weighting layer a number it merely decided on. The
    /// type has no numeric constructor; see its own documentation for the
    /// hole that closes.
    fn proven_capacity(&mut self, now_ms: u64) -> Option<ProvenCapacity>;
}

/// The reference [`ProofOfSpaceTimeSource`]: no protocol backs it, so
/// proven capacity is always absent. Correct, permanent behavior for
/// anyone not opting into [`crate::storage_proof::MerkleStorageProof`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProof;

impl ProofOfSpaceTimeSource for NoProof {
    fn proven_capacity(&mut self, _now_ms: u64) -> Option<ProvenCapacity> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_proof_always_returns_none() {
        let mut source = NoProof;
        assert_eq!(source.proven_capacity(0), None);
    }
}

//! The seam a real proof-of-space-time / proof-of-replication protocol
//! fills in — challenge-response evidence that a node genuinely holds
//! `capacity_units` of data across a span of time, not just at one instant.
//!
//! ## Honest limit — do not implement this without a human cryptographer
//!
//! This is the same class of commitment as `mini-uniqueness`'s behavioral-
//! entropy signal and, per the whitepaper (SS8.1: "the most demanding
//! engineering in the value layer... implemented human-only and externally
//! audited") and D-0035 point 5, requires human authorship and external
//! audit before real deployment — not AI-authored code. [`NoProof`] is the
//! only implementation in this repo, and it is the correct, permanent
//! choice until that human-led work exists: [`crate::weight::proposer_weight`]
//! is a pure function of *already-proven* capacity, and has no opinion on
//! how that capacity gets proven — a node with no real proof contributes
//! zero weight, which is exactly what `NoProof` returning `None` expresses.

/// A source of proof-of-space-time evidence for one identity's committed
/// storage. Implementations of the real protocol live outside this crate.
pub trait ProofOfSpaceTimeSource {
    /// This identity's currently-proven capacity, in whatever unit the
    /// caller's protocol measures (e.g. GiB held continuously across the
    /// challenge period). `None` means no valid proof is currently held —
    /// a normal, unremarkable outcome for a node that hasn't completed a
    /// challenge-response round yet.
    fn proven_capacity(&mut self) -> Option<u64>;
}

/// The reference [`ProofOfSpaceTimeSource`]: no real protocol exists here,
/// so proven capacity is always absent. Every node is a `NoProof` node
/// until the human-authored, externally-audited implementation described
/// above lands.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProof;

impl ProofOfSpaceTimeSource for NoProof {
    fn proven_capacity(&mut self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_proof_always_returns_none() {
        let mut source = NoProof;
        assert_eq!(source.proven_capacity(), None);
    }
}

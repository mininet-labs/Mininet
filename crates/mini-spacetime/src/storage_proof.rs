//! A real (Merkle/PDP-style) [`crate::proof::ProofOfSpaceTimeSource`]
//! implementation — the founder-directed "start simple" interim scheme
//! (D-0037/D-0038): periodic random-challenge, Merkle-proof responses,
//! well-documented (Ateniese et al.'s Provable Data Possession), achievable
//! now.
//!
//! ## What this proves, and what it does not
//!
//! A storage node commits to a [`StorageCommitment`] (a Merkle root over
//! its stored blocks). A verifier challenges a specific block index; the
//! node must return that block's *actual bytes* plus a Merkle proof
//! ([`verify_storage_challenge`]) — producing a valid response requires
//! having genuinely retained the real data, not just the previously-
//! published root. Repeating this over time
//! ([`ProofHistory`]/[`StorageWindowPolicy`]) demonstrates *continuous*
//! possession, the "time" half of proof-of-space-**time**.
//!
//! **What it does not prove: replication uniqueness.** This scheme cannot
//! tell the difference between a thousand honest small devices each
//! holding their own copy and one well-resourced server holding a single
//! copy and answering every challenge on their behalf — exactly the
//! warehouse-consolidation attack the whitepaper's egalitarian thesis
//! ("a thousand cheap, slow, scattered machines genuinely outcompete a
//! single warehouse," §7) depends on resisting. Real proof-of-replication
//! (Filecoin-style sequential/time-locked encoding) is the construction
//! that closes that gap, and is deliberately treated as a separate, later,
//! dedicated project rather than compressed into this pass.

use crate::merkle::MerkleProof;
use crate::proof::ProofOfSpaceTimeSource;

/// A storage node's public commitment: the Merkle root over its claimed
/// blocks, and how many blocks it claims to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageCommitment {
    /// Root of the Merkle tree over the committed blocks.
    pub merkle_root: [u8; 32],
    /// How many blocks the commitment covers.
    pub block_count: usize,
}

/// A verifier's challenge: prove possession of the block at this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageChallenge {
    /// The challenged block index.
    pub leaf_index: usize,
}

/// A node's response to a [`StorageChallenge`]: the actual block bytes,
/// plus a Merkle proof they belong at the challenged index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageChallengeResponse {
    /// Must match the challenge's index.
    pub leaf_index: usize,
    /// The actual bytes of the challenged block.
    pub block_bytes: Vec<u8>,
    /// Merkle membership proof for `block_bytes` at `leaf_index`.
    pub proof: MerkleProof,
}

/// Verify a challenge response against a commitment: the response's
/// claimed index must match its proof's index, and the proof must verify
/// the block's actual bytes against the committed root.
pub fn verify_storage_challenge(
    commitment: &StorageCommitment,
    response: &StorageChallengeResponse,
) -> bool {
    response.leaf_index == response.proof.leaf_index
        && response.proof.verify(
            &response.block_bytes,
            commitment.merkle_root,
            commitment.block_count,
        )
}

/// How tightly spaced successful challenge responses must be, and how
/// much continuous coverage counts as "proven over time." Tunable, not
/// frozen — the whitepaper specifies the shape (continuous re-proof over
/// a real span of time), not these exact numbers.
#[derive(Debug, Clone, Copy)]
pub struct StorageWindowPolicy {
    /// A gap between successive successful responses larger than this
    /// breaks the streak — the proof has lapsed.
    pub max_interval_ms: u64,
    /// Minimum unbroken coverage required before capacity counts as
    /// currently proven.
    pub min_window_ms: u64,
}

impl StorageWindowPolicy {
    /// A month-scale default: must answer at least every two days, and
    /// needs roughly a month of unbroken coverage.
    pub fn month_scale_default() -> Self {
        StorageWindowPolicy {
            max_interval_ms: 2 * 86_400_000,
            min_window_ms: 30 * 86_400_000,
        }
    }
}

/// A record of successful challenge-response timestamps for one storage
/// commitment.
#[derive(Debug, Clone, Default)]
pub struct ProofHistory {
    /// Kept sorted ascending.
    successes: Vec<u64>,
}

impl ProofHistory {
    /// A new, empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful challenge response at `at_ms`.
    pub fn record_success(&mut self, at_ms: u64) {
        match self.successes.binary_search(&at_ms) {
            Ok(_) => {}
            Err(pos) => self.successes.insert(pos, at_ms),
        }
    }

    /// The longest unbroken run of successes ending at or before `now_ms`
    /// — zero if the most recent success is itself already stale (older
    /// than `policy.max_interval_ms`), since a streak from long ago does
    /// not establish *current* possession.
    pub fn covered_window_ms(&self, policy: &StorageWindowPolicy, now_ms: u64) -> u64 {
        let relevant: Vec<u64> = self
            .successes
            .iter()
            .copied()
            .filter(|&t| t <= now_ms)
            .collect();
        let Some(&last) = relevant.last() else {
            return 0;
        };
        if now_ms.saturating_sub(last) > policy.max_interval_ms {
            return 0;
        }
        let mut start = relevant.len() - 1;
        while start > 0 {
            let gap = relevant[start] - relevant[start - 1];
            if gap > policy.max_interval_ms {
                break;
            }
            start -= 1;
        }
        last - relevant[start]
    }

    /// Whether this history currently demonstrates continuous possession
    /// per `policy` at `now_ms`.
    pub fn proven_space_time(&self, policy: &StorageWindowPolicy, now_ms: u64) -> bool {
        self.covered_window_ms(policy, now_ms) >= policy.min_window_ms
    }
}

/// The interim [`ProofOfSpaceTimeSource`] implementation: proven capacity
/// tracks whether this commitment's [`ProofHistory`] currently satisfies
/// its [`StorageWindowPolicy`].
#[derive(Debug, Clone)]
pub struct MerkleStorageProof {
    commitment: StorageCommitment,
    capacity_units: u64,
    policy: StorageWindowPolicy,
    history: ProofHistory,
}

impl MerkleStorageProof {
    /// A fresh proof tracker for `commitment`, declaring `capacity_units`
    /// of storage under `policy`.
    pub fn new(
        commitment: StorageCommitment,
        capacity_units: u64,
        policy: StorageWindowPolicy,
    ) -> Self {
        MerkleStorageProof {
            commitment,
            capacity_units,
            policy,
            history: ProofHistory::new(),
        }
    }

    /// The commitment this tracker is proving.
    pub fn commitment(&self) -> &StorageCommitment {
        &self.commitment
    }

    /// Verify `response` and, if valid, record a successful proof at
    /// `now_ms`. Returns whether the response was valid.
    pub fn submit_response(&mut self, response: &StorageChallengeResponse, now_ms: u64) -> bool {
        if verify_storage_challenge(&self.commitment, response) {
            self.history.record_success(now_ms);
            true
        } else {
            false
        }
    }
}

impl ProofOfSpaceTimeSource for MerkleStorageProof {
    fn proven_capacity(&mut self, now_ms: u64) -> Option<u64> {
        if self.history.proven_space_time(&self.policy, now_ms) {
            Some(self.capacity_units)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merkle::MerkleTree;

    fn blocks(n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|i| vec![i as u8; 16]).collect()
    }

    fn commitment_and_tree(n: usize) -> (StorageCommitment, MerkleTree, Vec<Vec<u8>>) {
        let data = blocks(n);
        let tree = MerkleTree::from_blocks(&data).unwrap();
        let commitment = StorageCommitment {
            merkle_root: tree.root(),
            block_count: tree.leaf_count(),
        };
        (commitment, tree, data)
    }

    fn respond(tree: &MerkleTree, data: &[Vec<u8>], index: usize) -> StorageChallengeResponse {
        StorageChallengeResponse {
            leaf_index: index,
            block_bytes: data[index].clone(),
            proof: tree.prove(index).unwrap(),
        }
    }

    #[test]
    fn a_valid_response_verifies() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let response = respond(&tree, &data, 2);
        assert!(verify_storage_challenge(&commitment, &response));
    }

    #[test]
    fn tampered_block_bytes_fail_verification() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let mut response = respond(&tree, &data, 2);
        response.block_bytes = b"fabricated".to_vec();
        assert!(!verify_storage_challenge(&commitment, &response));
    }

    #[test]
    fn mismatched_index_fails_verification() {
        let (commitment, tree, data) = commitment_and_tree(5);
        let mut response = respond(&tree, &data, 2);
        response.leaf_index = 3;
        assert!(!verify_storage_challenge(&commitment, &response));
    }

    #[test]
    fn a_single_success_does_not_establish_a_time_window() {
        let mut history = ProofHistory::new();
        history.record_success(0);
        let policy = StorageWindowPolicy::month_scale_default();
        assert!(!history.proven_space_time(&policy, 0));
    }

    #[test]
    fn sustained_close_successes_establish_the_window() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut t = 0u64;
        while t <= policy.min_window_ms {
            history.record_success(t);
            t += policy.max_interval_ms / 2;
        }
        assert!(history.proven_space_time(&policy, t));
    }

    #[test]
    fn a_gap_larger_than_max_interval_breaks_the_streak() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        history.record_success(0);
        // Huge gap, then a fresh streak that alone isn't long enough.
        history.record_success(policy.min_window_ms * 10);
        history.record_success(policy.min_window_ms * 10 + 1_000);
        assert!(!history.proven_space_time(&policy, policy.min_window_ms * 10 + 1_000));
    }

    #[test]
    fn a_stale_last_success_counts_as_no_current_proof() {
        let mut history = ProofHistory::new();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut t = 0u64;
        while t <= policy.min_window_ms {
            history.record_success(t);
            t += policy.max_interval_ms / 2;
        }
        // Long after the last success, well past max_interval_ms.
        let stale_now = t + policy.max_interval_ms * 10;
        assert!(!history.proven_space_time(&policy, stale_now));
    }

    #[test]
    fn merkle_storage_proof_reports_capacity_only_once_the_window_is_proven() {
        let (commitment, tree, data) = commitment_and_tree(4);
        let policy = StorageWindowPolicy::month_scale_default();
        let mut proof = MerkleStorageProof::new(commitment, 1_000, policy);

        assert_eq!(proof.proven_capacity(0), None);

        let mut t = 0u64;
        while t <= policy.min_window_ms {
            let response = respond(&tree, &data, (t % 4) as usize);
            assert!(proof.submit_response(&response, t));
            t += policy.max_interval_ms / 2;
        }
        assert_eq!(proof.proven_capacity(t), Some(1_000));
    }

    #[test]
    fn an_invalid_response_is_rejected_and_not_recorded() {
        let (commitment, tree, data) = commitment_and_tree(4);
        let policy = StorageWindowPolicy::month_scale_default();
        let mut proof = MerkleStorageProof::new(commitment, 1_000, policy);

        let mut bad_response = respond(&tree, &data, 0);
        bad_response.block_bytes = b"wrong".to_vec();
        assert!(!proof.submit_response(&bad_response, 0));
        assert_eq!(proof.proven_capacity(0), None);
    }
}

//! Ongoing challenge-response: reuses `mini_spacetime`'s existing PDP-style
//! Merkle challenge machinery directly against the *sealed replica's* Merkle
//! root, instead of duplicating it. The registration-time [`crate::audit`]
//! is what proves the replica was genuinely sealed through real sequential
//! layered work in the first place, distinguishing a holder who actually
//! did the sealing work from one who didn't; once that is established,
//! proving *continued* possession of that same sealed replica over time is
//! exactly the same storage-risk problem `mini_spacetime::storage_proof`
//! already solves (Ateniese et al. PDP), so this module composes it rather
//! than reinventing it.

use crate::seal::SealedReplica;
use mini_spacetime::{
    MerkleStorageProof, ProofOfSpaceTimeSource, StorageChallenge, StorageChallengeResponse,
    StorageCommitment, StorageWindowPolicy,
};

/// The storage commitment a verifier should record for ongoing possession
/// challenges: the sealed replica's root, not the original data's --
/// answering a challenge against this root requires holding the sealed
/// replica, not merely the plain data it was sealed from.
pub fn replica_commitment(replica: &SealedReplica) -> StorageCommitment {
    StorageCommitment {
        merkle_root: replica.replica_root(),
        block_count: replica.node_count(),
    }
}

/// Answer a possession challenge against the sealed replica's bytes.
/// `None` if `challenge.leaf_index` is out of range.
pub fn respond(
    replica: &SealedReplica,
    challenge: &StorageChallenge,
) -> Option<StorageChallengeResponse> {
    let leaf = replica.replica_leaf(challenge.leaf_index)?;
    let proof = replica.replica_tree().prove(challenge.leaf_index)?;
    Some(StorageChallengeResponse {
        leaf_index: challenge.leaf_index,
        block_bytes: leaf.to_vec(),
        proof,
    })
}

/// A [`ProofOfSpaceTimeSource`] sourced from a genuinely sealed replica:
/// the registration-time audit already proved this replica required real
/// sequential work to produce, so continued possession of it (not just of
/// the original data) is what this tracker proves over time. A thin
/// wrapper composing [`MerkleStorageProof`] against [`replica_commitment`]
/// -- `mini-spacetime`'s own weight formula
/// (`mini_spacetime::proposer_weight`) needs no changes to consume this,
/// since it already only depends on the trait, not the mechanism behind it.
#[derive(Debug, Clone)]
pub struct PorepStorageProof(MerkleStorageProof);

impl PorepStorageProof {
    /// A fresh proof tracker for `replica`, declaring `capacity_units` of
    /// proven-replicated storage under `policy`.
    pub fn new(replica: &SealedReplica, capacity_units: u64, policy: StorageWindowPolicy) -> Self {
        PorepStorageProof(MerkleStorageProof::new(
            replica_commitment(replica),
            capacity_units,
            policy,
        ))
    }

    /// The commitment this tracker is proving.
    pub fn commitment(&self) -> &StorageCommitment {
        self.0.commitment()
    }

    /// Verify `response` and, if valid, record a successful proof at
    /// `now_ms`. Returns whether the response was valid.
    pub fn submit_response(&mut self, response: &StorageChallengeResponse, now_ms: u64) -> bool {
        self.0.submit_response(response, now_ms)
    }
}

impl ProofOfSpaceTimeSource for PorepStorageProof {
    fn proven_capacity(&mut self, now_ms: u64) -> Option<u64> {
        self.0.proven_capacity(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::{seal, SealParams, NODE_SIZE};

    fn sealed_replica() -> SealedReplica {
        let params = SealParams::new([2u8; 32], 3).unwrap();
        let data: Vec<u8> = (0..16 * NODE_SIZE).map(|i| (i % 251) as u8).collect();
        seal(&params, &data).unwrap()
    }

    #[test]
    fn a_valid_response_verifies_against_the_replica_root() {
        let replica = sealed_replica();
        let commitment = replica_commitment(&replica);
        let challenge = StorageChallenge { leaf_index: 4 };
        let response = respond(&replica, &challenge).unwrap();
        assert!(mini_spacetime::MerkleProof::verify(
            &response.proof,
            &response.block_bytes,
            commitment.merkle_root,
            commitment.block_count,
        ));
    }

    #[test]
    fn an_out_of_range_challenge_returns_none() {
        let replica = sealed_replica();
        assert!(respond(&replica, &StorageChallenge { leaf_index: 999 }).is_none());
    }

    #[test]
    fn proven_capacity_tracks_a_sustained_challenge_streak() {
        let replica = sealed_replica();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut tracker = PorepStorageProof::new(&replica, 500, policy);

        assert_eq!(tracker.proven_capacity(0), None);

        let mut t = 0u64;
        let mut leaf = 0usize;
        while t <= policy.min_window_ms {
            let response = respond(
                &replica,
                &StorageChallenge {
                    leaf_index: leaf % 16,
                },
            )
            .unwrap();
            assert!(tracker.submit_response(&response, t));
            t += policy.max_interval_ms / 2;
            leaf += 1;
        }
        assert_eq!(tracker.proven_capacity(t), Some(500));
    }

    #[test]
    fn an_invalid_response_does_not_advance_proven_capacity() {
        let replica = sealed_replica();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut tracker = PorepStorageProof::new(&replica, 500, policy);

        let mut bad = respond(&replica, &StorageChallenge { leaf_index: 0 }).unwrap();
        bad.block_bytes = b"fabricated".to_vec();
        assert!(!tracker.submit_response(&bad, 0));
        assert_eq!(tracker.proven_capacity(0), None);
    }
}

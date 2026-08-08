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
        // Every replica node is exactly NODE_SIZE bytes by construction, so
        // this is a fact about the sealing format rather than a claim the
        // provider makes -- and `verify_storage_challenge` re-checks it on
        // every answered challenge regardless.
        block_size_bytes: crate::NODE_SIZE as u32,
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
    /// A fresh proof tracker for `replica` under `policy`, counting
    /// capacity at `units`.
    ///
    /// Takes a conversion policy rather than a capacity figure: the figure
    /// is derived from the replica's own node count and node size, both of
    /// which are enforced when challenges are answered. This previously
    /// accepted `capacity_units: u64` straight from the caller.
    pub fn new(
        replica: &SealedReplica,
        units: mini_spacetime::StorageUnitPolicy,
        policy: StorageWindowPolicy,
    ) -> Self {
        PorepStorageProof(MerkleStorageProof::new(
            replica_commitment(replica),
            units,
            policy,
        ))
    }

    /// The commitment this tracker is proving.
    pub fn commitment(&self) -> &StorageCommitment {
        self.0.commitment()
    }

    /// Verify `response` against the `challenge` it answers and, if valid,
    /// record a successful proof at `now_ms`.
    ///
    /// The challenge is forwarded rather than inferred: a possession proof
    /// that does not bind the question lets a prover answer whichever leaf
    /// it kept, which is not possession of anything.
    pub fn submit_response(
        &mut self,
        challenge: &StorageChallenge,
        response: &StorageChallengeResponse,
        now_ms: u64,
    ) -> bool {
        self.0.submit_response(challenge, response, now_ms)
    }
}

impl ProofOfSpaceTimeSource for PorepStorageProof {
    fn proven_capacity(&mut self, now_ms: u64) -> Option<mini_spacetime::ProvenCapacity> {
        self.0.proven_capacity(now_ms)
    }
}

#[cfg(test)]
mod tests {
    /// One capacity unit per committed byte, so a replica's unit count
    /// equals its byte count and the arithmetic stays hand-checkable.
    fn units() -> mini_spacetime::StorageUnitPolicy {
        mini_spacetime::StorageUnitPolicy::new(1).unwrap()
    }
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
        let mut tracker = PorepStorageProof::new(&replica, units(), policy);

        assert_eq!(tracker.proven_capacity(0), None);

        let mut t = 0u64;
        let mut leaf = 0usize;
        while t <= policy.min_window_ms {
            let challenge = StorageChallenge {
                leaf_index: leaf % 16,
            };
            let response = respond(&replica, &challenge).unwrap();
            assert!(tracker.submit_response(&challenge, &response, t));
            t += policy.max_interval_ms / 2;
            leaf += 1;
        }
        assert_eq!(
            tracker.proven_capacity(t).map(|c| c.units()),
            Some(replica_commitment(&replica).committed_bytes())
        );
    }

    #[test]
    fn an_invalid_response_does_not_advance_proven_capacity() {
        let replica = sealed_replica();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut tracker = PorepStorageProof::new(&replica, units(), policy);

        let challenge = StorageChallenge { leaf_index: 0 };
        let mut bad = respond(&replica, &challenge).unwrap();
        bad.block_bytes = b"fabricated".to_vec();
        assert!(!tracker.submit_response(&challenge, &bad, 0));
        assert_eq!(tracker.proven_capacity(0), None);
    }

    #[test]
    fn answering_a_leaf_other_than_the_one_challenged_is_refused() {
        // Keeping one leaf plus its Merkle path used to be enough to hold a
        // proven-capacity streak open indefinitely.
        let replica = sealed_replica();
        let policy = StorageWindowPolicy::month_scale_default();
        let mut tracker = PorepStorageProof::new(&replica, units(), policy);

        let asked = StorageChallenge { leaf_index: 9 };
        let kept = respond(&replica, &StorageChallenge { leaf_index: 0 }).unwrap();
        assert!(!tracker.submit_response(&asked, &kept, 0));
        assert_eq!(tracker.proven_capacity(0), None);
    }
}

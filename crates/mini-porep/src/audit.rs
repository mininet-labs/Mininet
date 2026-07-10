//! Registration-time probabilistic audit: the honest, explicitly-documented
//! substitute for a zk-SNARK sealing circuit -- building and verifying a
//! succinct proof of the entire layered labeling from scratch was judged
//! far too large and too risky to get right in this pass. Instead, a
//! verifier draws random `(layer, node)` challenges across the full
//! layered structure; the prover reveals each challenged node's label,
//! its DRG parent labels, its cross-layer predecessor label, and the
//! original data node it all traces back to, each with a Merkle inclusion
//! proof against a root the prover published *before* the challenge was
//! drawn; the verifier recomputes the labeling hash directly and checks it
//! matches.
//!
//! This is a real, well-established "spot-check a random subgraph" audit
//! technique -- not zero-knowledge, since it reveals plaintext intermediate
//! labels (and the underlying data node) for every challenged index. That
//! is an accepted tradeoff here: sealing is not trying to keep the data
//! confidential, only to prove that the sequential layered work claimed by
//! [`crate::seal::SealCommitment`] was genuinely performed once, at
//! registration time. Sampling enough challenges makes skipping any
//! meaningful fraction of the real sealing work exponentially unlikely to
//! go undetected, the same probabilistic argument PDP/spot-check storage
//! audits rely on generally.

use crate::drg::parents;
use crate::error::{PorepError, Result};
use crate::seal::{hash_layer, hash_seed_layer, xor, SealCommitment, SealedReplica, NODE_SIZE};
use mini_spacetime::MerkleProof;

/// One registration-time audit challenge: prove the labeling relationship
/// at a specific `(layer, node)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditChallenge {
    pub layer: u32,
    pub node: usize,
}

/// The prover's answer to one [`AuditChallenge`]: the challenged label plus
/// every value its labeling formula depends on, each with a Merkle
/// inclusion proof against the commitment's roots.
#[derive(Debug, Clone)]
pub struct AuditResponse {
    pub label: [u8; NODE_SIZE],
    pub label_proof: MerkleProof,
    pub data_node: [u8; NODE_SIZE],
    pub data_proof: MerkleProof,
    /// Empty for layer 0 (the seed layer has no DRG parents).
    pub parent_labels: Vec<(usize, [u8; NODE_SIZE], MerkleProof)>,
    /// `None` only for layer 0, which has no previous layer to link to.
    pub prev_layer_label: Option<([u8; NODE_SIZE], MerkleProof)>,
    /// `Some` only when `layer == num_layers` -- the final-layer challenge
    /// additionally proves the XOR encoding step against the replica root.
    pub replica_leaf: Option<([u8; NODE_SIZE], MerkleProof)>,
}

/// Deterministically derive `count` audit challenges from `commitment` and
/// a verifier-supplied `seed` (e.g. a fresh nonce or a recent block hash) --
/// no local randomness source needed, and reproducible for tests.
pub fn sample_challenges(
    commitment: &SealCommitment,
    seed: &[u8],
    count: usize,
) -> Vec<AuditChallenge> {
    (0..count)
        .map(|i| {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"mini-porep/audit-challenge");
            hasher.update(&commitment.replica_id);
            hasher.update(seed);
            hasher.update(&(i as u64).to_le_bytes());
            let digest = hasher.finalize();
            let bytes = digest.as_bytes();
            let layer_raw = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
            let node_raw = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
            AuditChallenge {
                layer: (layer_raw % (commitment.num_layers as u64 + 1)) as u32,
                node: (node_raw % commitment.node_count as u64) as usize,
            }
        })
        .collect()
}

/// Answer `challenge` against a fully sealed `replica`.
pub fn answer_challenge(
    replica: &SealedReplica,
    challenge: &AuditChallenge,
) -> Result<AuditResponse> {
    let AuditChallenge { layer, node } = *challenge;
    if node >= replica.node_count() {
        return Err(PorepError::NodeOutOfRange {
            index: node,
            node_count: replica.node_count(),
        });
    }
    if layer > replica.params().num_layers {
        return Err(PorepError::LayerOutOfRange {
            layer,
            num_layers: replica.params().num_layers,
        });
    }

    let label = replica.layer_label(layer, node);
    let label_proof = replica
        .layer_tree(layer)
        .prove(node)
        .ok_or(PorepError::MerkleProofFailed)?;

    let data_node = replica.data_node(node);
    let data_proof = replica
        .data_tree()
        .prove(node)
        .ok_or(PorepError::MerkleProofFailed)?;

    let parent_labels = if layer == 0 {
        Vec::new()
    } else {
        parents(&replica.params().replica_id, layer, node)
            .into_iter()
            .map(|p| {
                let pl = replica.layer_label(layer, p);
                let proof = replica
                    .layer_tree(layer)
                    .prove(p)
                    .ok_or(PorepError::MerkleProofFailed)?;
                Ok((p, pl, proof))
            })
            .collect::<Result<Vec<_>>>()?
    };

    let prev_layer_label = if layer == 0 {
        None
    } else {
        let pl = replica.layer_label(layer - 1, node);
        let proof = replica
            .layer_tree(layer - 1)
            .prove(node)
            .ok_or(PorepError::MerkleProofFailed)?;
        Some((pl, proof))
    };

    let replica_leaf = if layer == replica.params().num_layers {
        let leaf = replica
            .replica_leaf(node)
            .ok_or(PorepError::MerkleProofFailed)?;
        let proof = replica
            .replica_tree()
            .prove(node)
            .ok_or(PorepError::MerkleProofFailed)?;
        Some((leaf, proof))
    } else {
        None
    };

    Ok(AuditResponse {
        label,
        label_proof,
        data_node,
        data_proof,
        parent_labels,
        prev_layer_label,
        replica_leaf,
    })
}

/// Verify `response` against `commitment` for `challenge`, recomputing the
/// labeling hash directly rather than trusting any claimed value.
pub fn verify_audit_response(
    commitment: &SealCommitment,
    challenge: &AuditChallenge,
    response: &AuditResponse,
) -> bool {
    let AuditChallenge { layer, node } = *challenge;

    if layer > commitment.num_layers || node >= commitment.node_count {
        return false;
    }
    let Some(&layer_root) = commitment.layer_roots.get(layer as usize) else {
        return false;
    };

    if response.label_proof.leaf_index != node
        || !response
            .label_proof
            .verify(&response.label, layer_root, commitment.node_count)
    {
        return false;
    }
    if response.data_proof.leaf_index != node
        || !response.data_proof.verify(
            &response.data_node,
            commitment.data_root,
            commitment.node_count,
        )
    {
        return false;
    }

    let recomputed_ok = if layer == 0 {
        if !response.parent_labels.is_empty() || response.prev_layer_label.is_some() {
            return false;
        }
        hash_seed_layer(&commitment.replica_id, node, &response.data_node) == response.label
    } else {
        let expected_parents = parents(&commitment.replica_id, layer, node);
        if response.parent_labels.len() != expected_parents.len() {
            return false;
        }
        for (expected_idx, (got_idx, got_label, proof)) in
            expected_parents.iter().zip(&response.parent_labels)
        {
            if expected_idx != got_idx {
                return false;
            }
            if proof.leaf_index != *got_idx
                || !proof.verify(got_label, layer_root, commitment.node_count)
            {
                return false;
            }
        }

        let Some((prev_label, prev_proof)) = &response.prev_layer_label else {
            return false;
        };
        let Some(&prev_root) = commitment.layer_roots.get((layer - 1) as usize) else {
            return false;
        };
        if prev_proof.leaf_index != node
            || !prev_proof.verify(prev_label, prev_root, commitment.node_count)
        {
            return false;
        }

        let parent_label_values: Vec<[u8; NODE_SIZE]> =
            response.parent_labels.iter().map(|(_, l, _)| *l).collect();
        hash_layer(
            &commitment.replica_id,
            layer,
            node,
            &parent_label_values,
            prev_label,
        ) == response.label
    };
    if !recomputed_ok {
        return false;
    }

    if layer == commitment.num_layers {
        let Some((replica_leaf, replica_proof)) = &response.replica_leaf else {
            return false;
        };
        if replica_proof.leaf_index != node
            || !replica_proof.verify(replica_leaf, commitment.replica_root, commitment.node_count)
        {
            return false;
        }
        xor(response.label, response.data_node) == *replica_leaf
    } else {
        response.replica_leaf.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seal::{seal, SealParams};

    fn sealed(node_count: usize, num_layers: u32) -> SealedReplica {
        sealed_with_id([5u8; 32], node_count, num_layers)
    }

    fn sealed_with_id(replica_id: [u8; 32], node_count: usize, num_layers: u32) -> SealedReplica {
        let params = SealParams::new(replica_id, num_layers).unwrap();
        let data: Vec<u8> = (0..node_count * NODE_SIZE)
            .map(|i| (i % 251) as u8)
            .collect();
        seal(&params, &data).unwrap()
    }

    #[test]
    fn every_layer_and_node_answers_and_verifies() {
        let replica = sealed(12, 3);
        let commitment = replica.commitment();
        for layer in 0..=3u32 {
            for node in 0..12usize {
                let challenge = AuditChallenge { layer, node };
                let response = answer_challenge(&replica, &challenge).unwrap();
                assert!(
                    verify_audit_response(&commitment, &challenge, &response),
                    "layer {layer} node {node} failed to verify"
                );
            }
        }
    }

    #[test]
    fn sampled_challenges_are_deterministic_and_verify() {
        let replica = sealed(20, 4);
        let commitment = replica.commitment();
        let challenges = sample_challenges(&commitment, b"epoch-1-beacon", 30);
        let again = sample_challenges(&commitment, b"epoch-1-beacon", 30);
        assert_eq!(challenges, again);
        for challenge in &challenges {
            let response = answer_challenge(&replica, challenge).unwrap();
            assert!(verify_audit_response(&commitment, challenge, &response));
        }
    }

    #[test]
    fn a_tampered_label_fails_verification() {
        let replica = sealed(10, 2);
        let commitment = replica.commitment();
        let challenge = AuditChallenge { layer: 1, node: 5 };
        let mut response = answer_challenge(&replica, &challenge).unwrap();
        response.label[0] ^= 0xff;
        assert!(!verify_audit_response(&commitment, &challenge, &response));
    }

    #[test]
    fn a_tampered_parent_label_fails_verification() {
        let replica = sealed(10, 2);
        let commitment = replica.commitment();
        let challenge = AuditChallenge { layer: 1, node: 5 };
        let mut response = answer_challenge(&replica, &challenge).unwrap();
        response.parent_labels[0].1[0] ^= 0xff;
        assert!(!verify_audit_response(&commitment, &challenge, &response));
    }

    #[test]
    fn a_tampered_data_node_fails_verification() {
        let replica = sealed(10, 2);
        let commitment = replica.commitment();
        let challenge = AuditChallenge { layer: 0, node: 3 };
        let mut response = answer_challenge(&replica, &challenge).unwrap();
        response.data_node[0] ^= 0xff;
        assert!(!verify_audit_response(&commitment, &challenge, &response));
    }

    #[test]
    fn a_response_from_a_different_replica_fails_verification() {
        let replica_a = sealed_with_id([5u8; 32], 10, 2);
        let replica_b = sealed_with_id([6u8; 32], 10, 2);
        let commitment_a = replica_a.commitment();
        let challenge = AuditChallenge { layer: 1, node: 4 };
        let response_b = answer_challenge(&replica_b, &challenge).unwrap();
        assert!(!verify_audit_response(
            &commitment_a,
            &challenge,
            &response_b
        ));
    }

    #[test]
    fn claiming_a_replica_leaf_below_the_top_layer_fails_verification() {
        let replica = sealed(10, 3);
        let commitment = replica.commitment();
        let challenge = AuditChallenge { layer: 1, node: 2 };
        let mut response = answer_challenge(&replica, &challenge).unwrap();
        let top_challenge = AuditChallenge { layer: 3, node: 2 };
        let top_response = answer_challenge(&replica, &top_challenge).unwrap();
        response.replica_leaf = top_response.replica_leaf;
        assert!(!verify_audit_response(&commitment, &challenge, &response));
    }

    #[test]
    fn a_lazy_prover_who_fabricates_labels_without_doing_the_sequential_work_fails_verification() {
        // Simulates the exact attack the audit exists to catch: a prover who
        // never actually ran the stacked-labeling hash chain, and instead
        // just commits to made-up "labels" hoping nobody checks. Even
        // though the fabricated labels are internally Merkle-consistent
        // (real trees, real proofs), they don't satisfy the labeling
        // formula, so a random spot check catches it with overwhelming
        // probability -- exactly what makes the audit probabilistic rather
        // than exhaustive, and still sound.
        let honest = sealed(6, 2);
        let real_commitment = honest.commitment();

        let node_count = 6usize;
        let fake_layers: Vec<Vec<[u8; NODE_SIZE]>> = (0..=2u32)
            .map(|l| {
                (0..node_count)
                    .map(|i| {
                        let mut h = blake3::Hasher::new();
                        h.update(b"not-the-real-labeling-formula");
                        h.update(&l.to_le_bytes());
                        h.update(&(i as u64).to_le_bytes());
                        h.finalize().into()
                    })
                    .collect()
            })
            .collect();
        let fake_trees: Vec<mini_spacetime::MerkleTree> = fake_layers
            .iter()
            .map(|layer| {
                mini_spacetime::MerkleTree::from_blocks(
                    &layer.iter().map(|l| l.to_vec()).collect::<Vec<_>>(),
                )
                .unwrap()
            })
            .collect();

        let fake_commitment = SealCommitment {
            replica_id: real_commitment.replica_id,
            num_layers: real_commitment.num_layers,
            node_count,
            data_root: real_commitment.data_root,
            layer_roots: fake_trees.iter().map(|t| t.root()).collect(),
            replica_root: real_commitment.replica_root,
        };

        let challenge = AuditChallenge { layer: 1, node: 3 };
        let fake_response = AuditResponse {
            label: fake_layers[1][3],
            label_proof: fake_trees[1].prove(3).unwrap(),
            data_node: honest.data_node(3),
            data_proof: honest.data_tree().prove(3).unwrap(),
            parent_labels: parents(&real_commitment.replica_id, 1, 3)
                .into_iter()
                .map(|p| (p, fake_layers[1][p], fake_trees[1].prove(p).unwrap()))
                .collect(),
            prev_layer_label: Some((fake_layers[0][3], fake_trees[0].prove(3).unwrap())),
            replica_leaf: None,
        };

        assert!(!verify_audit_response(
            &fake_commitment,
            &challenge,
            &fake_response
        ));
    }

    #[test]
    fn changing_an_early_data_node_changes_downstream_labels() {
        // The sequential-dependency property the whole construction rests
        // on: a node's label transitively depends on earlier nodes in the
        // graph, so it cannot be produced without having propagated their
        // values through the hash chain first.
        let params = SealParams::new([11u8; 32], 3).unwrap();
        let data_a: Vec<u8> = (0..12 * NODE_SIZE).map(|i| (i % 251) as u8).collect();
        let mut data_b = data_a.clone();
        data_b[0] ^= 0xff;

        let sealed_a = seal(&params, &data_a).unwrap();
        let sealed_b = seal(&params, &data_b).unwrap();

        assert_ne!(sealed_a.replica_root(), sealed_b.replica_root());
        let top = params.num_layers;
        assert_ne!(
            sealed_a.layer_label(top, 11),
            sealed_b.layer_label(top, 11),
            "changing data node 0 must ripple through to the last node's top-layer label"
        );
    }

    #[test]
    fn out_of_range_challenges_are_rejected_up_front() {
        let replica = sealed(4, 2);
        assert!(matches!(
            answer_challenge(&replica, &AuditChallenge { layer: 0, node: 10 }),
            Err(PorepError::NodeOutOfRange { .. })
        ));
        assert!(matches!(
            answer_challenge(&replica, &AuditChallenge { layer: 9, node: 0 }),
            Err(PorepError::LayerOutOfRange { .. })
        ));
    }
}

//! Sealing: stacked layered labeling + final XOR encoding, the SDR
//! ("Stacked Depth-Robust Graph") construction Filecoin's production
//! proof-of-replication is built on, coded here in-house (D-0063) rather
//! than depended on as a library.
//!
//! Layer 0 seeds directly from the raw data: `label(0, i) = H(replica_id,
//! 0, i, D_i)`. Each subsequent layer's label at node `i` is a hash of the
//! replica id, layer index, node index, that layer's [`crate::drg`] parent
//! labels (same layer, always strictly earlier indices), and the
//! *previous* layer's label at the same index `i` -- the cross-layer
//! "identity" edge that gives stacking its depth. Shortcutting layer `L`
//! requires already having computed all of layer `L - 1`, transitively
//! down to layer 0, so the total sequential work an honest sealer performs
//! is `num_layers * node_count` hash steps, not just one layer's worth --
//! the property that makes fast, storage-light replica generation
//! infeasible and so distinguishes genuinely holding a sealed copy from
//! cheaply deriving one on demand.
//!
//! The final replica is `R_i = label(num_layers, i) XOR D_i`, the
//! Filecoin-style "encoding" step: same size as the original data, but
//! unrecoverable without every layer's sequential labeling work already
//! done.

use crate::drg::parents;
use crate::error::{PorepError, Result};
use mini_spacetime::MerkleTree;

/// Bytes per node label / data block. Matches a BLAKE3 digest exactly, so
/// every label is used as both a hash output and the next hash's input
/// with no padding or truncation.
pub const NODE_SIZE: usize = 32;

const SEED_TAG: &[u8] = b"mini-porep/seed-layer";
const LAYER_TAG: &[u8] = b"mini-porep/stacked-layer";

pub(crate) fn hash_seed_layer(
    replica_id: &[u8; 32],
    node_index: usize,
    data_node: &[u8; NODE_SIZE],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(SEED_TAG);
    hasher.update(replica_id);
    hasher.update(&0u32.to_le_bytes());
    hasher.update(&(node_index as u64).to_le_bytes());
    hasher.update(data_node);
    hasher.finalize().into()
}

pub(crate) fn hash_layer(
    replica_id: &[u8; 32],
    layer_index: u32,
    node_index: usize,
    parent_labels: &[[u8; NODE_SIZE]],
    prev_layer_label: &[u8; NODE_SIZE],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(LAYER_TAG);
    hasher.update(replica_id);
    hasher.update(&layer_index.to_le_bytes());
    hasher.update(&(node_index as u64).to_le_bytes());
    for parent_label in parent_labels {
        hasher.update(parent_label);
    }
    hasher.update(prev_layer_label);
    hasher.finalize().into()
}

pub(crate) fn xor(a: [u8; NODE_SIZE], b: [u8; NODE_SIZE]) -> [u8; NODE_SIZE] {
    let mut out = [0u8; NODE_SIZE];
    for i in 0..NODE_SIZE {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Parameters for one sealing run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealParams {
    /// A unique identifier for this replica -- binds the sealed labels to
    /// one specific (holder, piece) pair so the same raw data sealed for
    /// two different replica ids produces two unrelated sets of labels.
    pub replica_id: [u8; 32],
    /// How many stacked layers to seal through. Deeper is more sequential
    /// work required per seal, and more registration-audit coverage
    /// per random challenge; tunable per deployment, not frozen.
    pub num_layers: u32,
}

impl SealParams {
    /// A new set of sealing parameters. Errors if `num_layers` is zero --
    /// there is no stacking depth to seal through with no layers at all.
    pub fn new(replica_id: [u8; 32], num_layers: u32) -> Result<Self> {
        if num_layers == 0 {
            return Err(PorepError::ZeroLayers);
        }
        Ok(SealParams {
            replica_id,
            num_layers,
        })
    }
}

/// The public commitment a prover publishes before being challenged: every
/// Merkle root needed to check a registration-time [`crate::audit`]
/// response or an ongoing [`crate::challenge`] response, without needing
/// the labels or the original data themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealCommitment {
    pub replica_id: [u8; 32],
    pub num_layers: u32,
    pub node_count: usize,
    /// Root of the Merkle tree over the original (pre-seal) data nodes.
    pub data_root: [u8; 32],
    /// `layer_roots[l]` is the root of the Merkle tree over layer `l`'s
    /// labels, for `l` in `0..=num_layers`.
    pub layer_roots: Vec<[u8; 32]>,
    /// Root of the Merkle tree over the final encoded replica.
    pub replica_root: [u8; 32],
}

/// A fully sealed replica: every layer's labels, the final encoded
/// replica, and a Merkle tree over each -- everything a prover needs to
/// answer both registration-time audit challenges and ongoing possession
/// challenges.
#[derive(Debug, Clone)]
pub struct SealedReplica {
    params: SealParams,
    node_count: usize,
    data_nodes: Vec<[u8; NODE_SIZE]>,
    data_tree: MerkleTree,
    /// `layer_labels[l][i]` = label(l, i), for `l` in `0..=num_layers`.
    layer_labels: Vec<Vec<[u8; NODE_SIZE]>>,
    layer_trees: Vec<MerkleTree>,
    replica: Vec<[u8; NODE_SIZE]>,
    replica_tree: MerkleTree,
}

impl SealedReplica {
    /// This replica's sealing parameters.
    pub fn params(&self) -> &SealParams {
        &self.params
    }

    /// How many nodes (of [`NODE_SIZE`] bytes each) the sealed data spans.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Root of the Merkle tree over the final encoded replica.
    pub fn replica_root(&self) -> [u8; 32] {
        self.replica_tree.root()
    }

    /// The final encoded replica bytes, flattened.
    pub fn replica_bytes(&self) -> Vec<u8> {
        self.replica.iter().flatten().copied().collect()
    }

    /// The public commitment for this replica -- what a prover publishes
    /// before any challenge is drawn.
    pub fn commitment(&self) -> SealCommitment {
        SealCommitment {
            replica_id: self.params.replica_id,
            num_layers: self.params.num_layers,
            node_count: self.node_count,
            data_root: self.data_tree.root(),
            layer_roots: self.layer_trees.iter().map(MerkleTree::root).collect(),
            replica_root: self.replica_tree.root(),
        }
    }

    pub(crate) fn layer_label(&self, layer: u32, node: usize) -> [u8; NODE_SIZE] {
        self.layer_labels[layer as usize][node]
    }

    pub(crate) fn layer_tree(&self, layer: u32) -> &MerkleTree {
        &self.layer_trees[layer as usize]
    }

    pub(crate) fn data_node(&self, node: usize) -> [u8; NODE_SIZE] {
        self.data_nodes[node]
    }

    pub(crate) fn data_tree(&self) -> &MerkleTree {
        &self.data_tree
    }

    pub(crate) fn replica_leaf(&self, node: usize) -> Option<[u8; NODE_SIZE]> {
        self.replica.get(node).copied()
    }

    pub(crate) fn replica_tree(&self) -> &MerkleTree {
        &self.replica_tree
    }
}

/// Seal `data` under `params`: build every stacked layer's labels, then
/// the final XOR-encoded replica. `data.len()` must be a positive multiple
/// of [`NODE_SIZE`].
pub fn seal(params: &SealParams, data: &[u8]) -> Result<SealedReplica> {
    if data.is_empty() || data.len() % NODE_SIZE != 0 {
        return Err(PorepError::InvalidDataLength { len: data.len() });
    }
    let node_count = data.len() / NODE_SIZE;
    let data_nodes: Vec<[u8; NODE_SIZE]> = data
        .chunks_exact(NODE_SIZE)
        .map(|c| c.try_into().unwrap())
        .collect();
    let data_tree =
        MerkleTree::from_blocks(&data_nodes.iter().map(|n| n.to_vec()).collect::<Vec<_>>())
            .expect("node_count is positive, checked above");

    let mut layer_labels: Vec<Vec<[u8; NODE_SIZE]>> =
        Vec::with_capacity(params.num_layers as usize + 1);

    let layer0: Vec<[u8; NODE_SIZE]> = (0..node_count)
        .map(|i| hash_seed_layer(&params.replica_id, i, &data_nodes[i]))
        .collect();
    layer_labels.push(layer0);

    for l in 1..=params.num_layers {
        let prev = layer_labels[(l - 1) as usize].clone();
        let mut layer: Vec<[u8; NODE_SIZE]> = Vec::with_capacity(node_count);
        for (i, prev_label) in prev.iter().enumerate() {
            let parent_indices = parents(&params.replica_id, l, i);
            let parent_labels: Vec<[u8; NODE_SIZE]> =
                parent_indices.iter().map(|&p| layer[p]).collect();
            layer.push(hash_layer(
                &params.replica_id,
                l,
                i,
                &parent_labels,
                prev_label,
            ));
        }
        layer_labels.push(layer);
    }

    let layer_trees: Vec<MerkleTree> = layer_labels
        .iter()
        .map(|layer| {
            MerkleTree::from_blocks(&layer.iter().map(|l| l.to_vec()).collect::<Vec<_>>())
                .expect("node_count is positive, checked above")
        })
        .collect();

    let final_layer = &layer_labels[params.num_layers as usize];
    let replica: Vec<[u8; NODE_SIZE]> = (0..node_count)
        .map(|i| xor(final_layer[i], data_nodes[i]))
        .collect();
    let replica_tree =
        MerkleTree::from_blocks(&replica.iter().map(|r| r.to_vec()).collect::<Vec<_>>())
            .expect("node_count is positive, checked above");

    Ok(SealedReplica {
        params: params.clone(),
        node_count,
        data_nodes,
        data_tree,
        layer_labels,
        layer_trees,
        replica,
        replica_tree,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(node_count: usize) -> Vec<u8> {
        (0..node_count * NODE_SIZE)
            .map(|i| (i % 251) as u8)
            .collect()
    }

    #[test]
    fn zero_layers_is_rejected() {
        assert_eq!(SealParams::new([1u8; 32], 0), Err(PorepError::ZeroLayers));
    }

    #[test]
    fn non_node_aligned_data_is_rejected() {
        let params = SealParams::new([1u8; 32], 2).unwrap();
        assert!(matches!(
            seal(&params, &[0u8; 10]),
            Err(PorepError::InvalidDataLength { len: 10 })
        ));
    }

    #[test]
    fn empty_data_is_rejected() {
        let params = SealParams::new([1u8; 32], 2).unwrap();
        assert!(matches!(
            seal(&params, &[]),
            Err(PorepError::InvalidDataLength { len: 0 })
        ));
    }

    #[test]
    fn sealing_is_deterministic() {
        let params = SealParams::new([3u8; 32], 4).unwrap();
        let d = data(16);
        let a = seal(&params, &d).unwrap();
        let b = seal(&params, &d).unwrap();
        assert_eq!(a.commitment(), b.commitment());
    }

    #[test]
    fn different_replica_ids_seal_to_different_replicas() {
        let d = data(16);
        let a = seal(&SealParams::new([3u8; 32], 4).unwrap(), &d).unwrap();
        let b = seal(&SealParams::new([9u8; 32], 4).unwrap(), &d).unwrap();
        assert_ne!(a.replica_root(), b.replica_root());
    }

    #[test]
    fn replica_is_not_the_plain_data() {
        let params = SealParams::new([3u8; 32], 4).unwrap();
        let d = data(16);
        let sealed = seal(&params, &d).unwrap();
        assert_ne!(sealed.replica_bytes(), d);
    }

    #[test]
    fn replica_and_data_are_the_same_length() {
        let params = SealParams::new([3u8; 32], 4).unwrap();
        let d = data(16);
        let sealed = seal(&params, &d).unwrap();
        assert_eq!(sealed.replica_bytes().len(), d.len());
    }

    #[test]
    fn layer_count_matches_num_layers_plus_one() {
        let params = SealParams::new([3u8; 32], 5).unwrap();
        let d = data(8);
        let sealed = seal(&params, &d).unwrap();
        assert_eq!(sealed.layer_labels.len(), 6);
        assert_eq!(sealed.layer_trees.len(), 6);
    }
}

//! A domain-separated Merkle tree over content blocks — the building
//! block [`crate::storage_proof`]'s challenge-response scheme is built on.
//! Leaf and internal-node hashes use different prefix bytes (the
//! Certificate-Transparency-style hardening, RFC 6962) so a leaf's hash
//! can never be replayed as a valid internal node and vice versa.

const LEAF_PREFIX: u8 = 0x00;
const NODE_PREFIX: u8 = 0x01;

fn leaf_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[LEAF_PREFIX]);
    hasher.update(data);
    hasher.finalize().into()
}

fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[NODE_PREFIX]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// A Merkle tree built over content blocks, each block hashed as a leaf.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    leaf_count: usize,
    /// `levels[0]` is the leaf-hash level; `levels.last()` is `[root]`.
    levels: Vec<Vec<[u8; 32]>>,
}

impl MerkleTree {
    /// Build a tree over `blocks`. `None` for an empty block list — there
    /// is no meaningful root for zero blocks.
    pub fn from_blocks(blocks: &[Vec<u8>]) -> Option<Self> {
        if blocks.is_empty() {
            return None;
        }
        let leaves: Vec<[u8; 32]> = blocks.iter().map(|b| leaf_hash(b)).collect();
        Some(Self::from_leaf_hashes(leaves))
    }

    fn from_leaf_hashes(leaves: Vec<[u8; 32]>) -> Self {
        let leaf_count = leaves.len();
        let mut levels = vec![leaves];
        while levels.last().unwrap().len() > 1 {
            let current = levels.last().unwrap();
            let mut next = Vec::with_capacity(current.len().div_ceil(2));
            let mut i = 0;
            while i < current.len() {
                if i + 1 < current.len() {
                    next.push(node_hash(&current[i], &current[i + 1]));
                } else {
                    // Odd one out: promoted unchanged rather than
                    // duplicated, avoiding the classic duplicate-node
                    // ambiguity some naive Merkle trees suffer from.
                    next.push(current[i]);
                }
                i += 2;
            }
            levels.push(next);
        }
        MerkleTree { leaf_count, levels }
    }

    /// The tree's root hash.
    pub fn root(&self) -> [u8; 32] {
        self.levels.last().unwrap()[0]
    }

    /// How many blocks this tree was built over.
    pub fn leaf_count(&self) -> usize {
        self.leaf_count
    }

    /// A membership proof for the block at `index`. `None` if out of range.
    pub fn prove(&self, index: usize) -> Option<MerkleProof> {
        if index >= self.leaf_count {
            return None;
        }
        let mut siblings = Vec::with_capacity(self.levels.len() - 1);
        let mut idx = index;
        for level in &self.levels[..self.levels.len() - 1] {
            let is_right = idx % 2 == 1;
            let sibling_idx = if is_right { idx - 1 } else { idx + 1 };
            siblings.push(level.get(sibling_idx).copied());
            idx /= 2;
        }
        Some(MerkleProof {
            leaf_index: index,
            siblings,
        })
    }
}

/// A proof that a specific block belongs at a specific leaf index of a
/// tree with a given root, without needing the rest of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// Which leaf this proof is for.
    pub leaf_index: usize,
    /// Sibling hashes from the leaf level up to (not including) the root,
    /// bottom to top. `None` at a level where this node was promoted
    /// unchanged (an unpaired odd-length level) rather than combined with
    /// a sibling.
    siblings: Vec<Option<[u8; 32]>>,
}

impl MerkleProof {
    /// Verify that `leaf_data` hashes into `root` at [`Self::leaf_index`],
    /// for a tree of `leaf_count` total leaves.
    pub fn verify(&self, leaf_data: &[u8], root: [u8; 32], leaf_count: usize) -> bool {
        if self.leaf_index >= leaf_count {
            return false;
        }
        let mut hash = leaf_hash(leaf_data);
        let mut idx = self.leaf_index;
        for sibling in &self.siblings {
            hash = match sibling {
                Some(sib) => {
                    if idx % 2 == 1 {
                        node_hash(sib, &hash)
                    } else {
                        node_hash(&hash, sib)
                    }
                }
                None => hash,
            };
            idx /= 2;
        }
        hash == root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(n: usize) -> Vec<Vec<u8>> {
        (0..n).map(|i| vec![i as u8; 8]).collect()
    }

    #[test]
    fn empty_blocks_has_no_tree() {
        assert!(MerkleTree::from_blocks(&[]).is_none());
    }

    #[test]
    fn single_block_root_is_its_own_leaf_hash() {
        let data = vec![vec![1, 2, 3]];
        let tree = MerkleTree::from_blocks(&data).unwrap();
        let proof = tree.prove(0).unwrap();
        assert!(proof.verify(&data[0], tree.root(), 1));
    }

    #[test]
    fn every_leaf_proves_for_various_sizes() {
        for n in [1, 2, 3, 4, 5, 7, 8, 9, 16] {
            let data = blocks(n);
            let tree = MerkleTree::from_blocks(&data).unwrap();
            for (i, block) in data.iter().enumerate() {
                let proof = tree.prove(i).unwrap();
                assert!(
                    proof.verify(block, tree.root(), n),
                    "failed to verify leaf {i} of {n}"
                );
            }
        }
    }

    #[test]
    fn tree_root_is_deterministic() {
        let data = blocks(6);
        let a = MerkleTree::from_blocks(&data).unwrap();
        let b = MerkleTree::from_blocks(&data).unwrap();
        assert_eq!(a.root(), b.root());
    }

    #[test]
    fn tampered_block_data_fails_verification() {
        let data = blocks(5);
        let tree = MerkleTree::from_blocks(&data).unwrap();
        let proof = tree.prove(2).unwrap();
        assert!(!proof.verify(b"not the real block", tree.root(), 5));
    }

    #[test]
    fn proof_for_out_of_range_index_is_none() {
        let data = blocks(4);
        let tree = MerkleTree::from_blocks(&data).unwrap();
        assert!(tree.prove(4).is_none());
        assert!(tree.prove(100).is_none());
    }

    #[test]
    fn verify_against_wrong_leaf_count_fails() {
        let data = blocks(4);
        let tree = MerkleTree::from_blocks(&data).unwrap();
        // leaf_index 0 would be within a *smaller* claimed leaf_count only
        // incidentally valid; use an index that is clearly out of range
        // for the wrong count instead.
        let proof_at_3 = tree.prove(3).unwrap();
        assert!(!proof_at_3.verify(&data[3], tree.root(), 2));
    }

    #[test]
    fn a_proof_from_a_different_tree_does_not_verify() {
        let data_a = blocks(4);
        let data_b = blocks(5);
        let tree_a = MerkleTree::from_blocks(&data_a).unwrap();
        let tree_b = MerkleTree::from_blocks(&data_b).unwrap();
        let proof = tree_a.prove(1).unwrap();
        assert!(!proof.verify(&data_a[1], tree_b.root(), 5));
    }
}

//! Depth-robust graph (DRG) parent selection for stacked sealing.
//!
//! Each node in a layer has [`DRG_DEGREE`] parents drawn from strictly
//! earlier nodes in the *same* layer: the sequential predecessor
//! (`node_index - 1`), which alone forces full in-layer sequentiality --
//! computing label `i` requires already having label `i - 1` -- plus
//! several pseudorandom long-range back-edges into `[0, node_index)`. The
//! long-range edges are what make the graph *depth-robust*: without them a
//! dishonest prover could get away with storing only a thin sequential
//! spine and cheaply recomputing everything else on challenge, since a
//! purely sequential chain has no structure forcing wide replication.
//! Parent selection is deterministic given `(replica_id, layer_index,
//! node_index)`, so a sealer and a verifier who both know the replica id
//! always compute the identical graph -- no shared randomness needed.
//!
//! This is a deliberately simplified construction, not a byte-for-byte
//! reimplementation of Filecoin's production `BucketGraph` (which draws
//! parents from a specific probability-weighted "bucket sampling"
//! distribution tuned by prior published depth-robustness analysis).
//! Reproducing that exact distribution from memory was judged too much
//! precision risk for a from-scratch implementation to get right; this
//! graph is structurally similar -- a sequential edge plus pseudorandom
//! long-range edges -- but not parameter-identical, and should be read as
//! such rather than as a claim of matching Filecoin's own analyzed
//! parameters.

/// Parents per node (one sequential predecessor plus five long-range
/// edges), except near the start of a layer where fewer than this many
/// earlier nodes even exist.
pub const DRG_DEGREE: usize = 6;

/// The parent node indices (within the same layer) that `node_index`'s
/// label depends on. Always a subset of `[0, node_index)`, so parents are
/// always already computed by the time a sealer reaches `node_index` while
/// processing a layer in increasing order. Node `0` has no parents -- its
/// label seeds directly from the layer's own base material instead (the
/// raw data for layer 0, the previous layer's label for layer >= 1).
pub fn parents(replica_id: &[u8; 32], layer_index: u32, node_index: usize) -> Vec<usize> {
    if node_index == 0 {
        return Vec::new();
    }
    let degree = DRG_DEGREE.min(node_index);
    let mut out = Vec::with_capacity(degree);
    let mut seen = std::collections::BTreeSet::new();

    out.push(node_index - 1);
    seen.insert(node_index - 1);

    let mut counter: u64 = 0;
    while out.len() < degree {
        let candidate = deterministic_index(replica_id, layer_index, node_index, counter);
        counter += 1;
        if seen.insert(candidate) {
            out.push(candidate);
        }
    }
    out
}

fn deterministic_index(
    replica_id: &[u8; 32],
    layer_index: u32,
    node_index: usize,
    counter: u64,
) -> usize {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mini-porep/drg-parent");
    hasher.update(replica_id);
    hasher.update(&layer_index.to_le_bytes());
    hasher.update(&(node_index as u64).to_le_bytes());
    hasher.update(&counter.to_le_bytes());
    let digest = hasher.finalize();
    let raw = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
    (raw % node_index as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const RID: [u8; 32] = [7u8; 32];

    #[test]
    fn node_zero_has_no_parents() {
        assert!(parents(&RID, 0, 0).is_empty());
        assert!(parents(&RID, 3, 0).is_empty());
    }

    #[test]
    fn parents_are_always_strictly_earlier() {
        for layer in 0..3u32 {
            for node in 1..200usize {
                for &p in &parents(&RID, layer, node) {
                    assert!(p < node, "layer {layer} node {node} had parent {p} >= node");
                }
            }
        }
    }

    #[test]
    fn sequential_predecessor_is_always_included() {
        for node in 1..50usize {
            assert!(parents(&RID, 1, node).contains(&(node - 1)));
        }
    }

    #[test]
    fn degree_caps_at_available_earlier_nodes() {
        assert_eq!(parents(&RID, 0, 1).len(), 1);
        assert_eq!(parents(&RID, 0, 2).len(), 2);
        assert_eq!(parents(&RID, 0, 3).len(), 3);
        assert_eq!(parents(&RID, 0, 100).len(), DRG_DEGREE);
    }

    #[test]
    fn parent_sets_have_no_duplicates() {
        for node in 1..200usize {
            let p = parents(&RID, 2, node);
            let unique: std::collections::BTreeSet<_> = p.iter().collect();
            assert_eq!(p.len(), unique.len());
        }
    }

    #[test]
    fn parent_selection_is_deterministic() {
        for node in 1..100usize {
            assert_eq!(parents(&RID, 5, node), parents(&RID, 5, node));
        }
    }

    #[test]
    fn different_layers_yield_different_graphs() {
        // Not a hard guarantee for every node (small parent sets can
        // coincide by chance), but across many nodes some must differ --
        // otherwise the per-layer graph would be pointless.
        let mut any_different = false;
        for node in 10..100usize {
            if parents(&RID, 0, node) != parents(&RID, 1, node) {
                any_different = true;
                break;
            }
        }
        assert!(any_different);
    }

    #[test]
    fn different_replica_ids_yield_different_graphs() {
        let other = [9u8; 32];
        let mut any_different = false;
        for node in 10..100usize {
            if parents(&RID, 0, node) != parents(&other, 0, node) {
                any_different = true;
                break;
            }
        }
        assert!(any_different);
    }
}

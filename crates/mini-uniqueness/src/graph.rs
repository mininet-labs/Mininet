//! Signal (a): the social-vouching graph, and a SybilRank-style trust
//! propagation over it (whitepaper SS5: "a well-studied family of
//! techniques... because authentic human social graphs have a shape that
//! manufactured ones struggle to imitate").
//!
//! The graph is built from verified [`crate::verify::VouchVerdict`]s: an
//! edge exists between two identity roots only once **both** devices signed
//! one mutual transcript (there is no wire shape for a one-sided "I vouch
//! for you but you never agreed" claim — see [`crate::vouch`]), which
//! structurally rules out unilaterally fabricating vouches for real humans
//! who never participated.
//!
//! ## Why trust propagation, not just an edge count
//!
//! A Sybil farm can manufacture arbitrarily many internal edges among its
//! own fake identities, but it cannot manufacture edges *from* genuine
//! humans who never met it. Propagating trust outward from a small seed set
//! (the founding cohort, whitepaper SS12) for a small, bounded number of
//! rounds means a node's score reflects how well-connected it is *to the
//! trusted region*, not how many edges it has in total — a dense inbred
//! Sybil cluster with only one or two edges into the honest graph receives
//! almost no trust no matter how large it grows internally. This is the
//! well-known SybilRank technique (Cao et al., 2012); what follows is a
//! from-scratch, integer-only reimplementation of that algorithm's shape,
//! not a port of any specific codebase.
//!
//! ## Honest limits
//!
//! This is the core propagation primitive, not a calibrated production
//! defense: seed-set governance (who counts as a trusted seed, and how that
//! set changes as the founding cohort's position dilutes — whitepaper SS5),
//! a real acceptance threshold, and the exact iteration count for a
//! network of real size are all `pending` tuning decisions, deliberately
//! left as caller-supplied parameters rather than hardcoded here.

use std::collections::{HashMap, HashSet};

use did_mini::Did;

use crate::verify::VouchVerdict;

/// Fixed-point scale for trust mass. All arithmetic here is integer, so a
/// computation over the same graph always produces exactly the same scores
/// on every device — the same "all integer, so exactly reproducible"
/// convention `mini-reward` uses for accrual.
pub const TRUST_SCALE: u64 = 1_000_000;

/// An undirected graph of verified mutual vouches between identity roots.
#[derive(Debug, Default)]
pub struct VouchGraph {
    edges: HashMap<Did, HashSet<Did>>,
}

impl VouchGraph {
    /// A new, empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a verified vouch as one undirected edge.
    pub fn add_verdict(&mut self, verdict: &VouchVerdict) {
        self.edges
            .entry(verdict.a_root.clone())
            .or_default()
            .insert(verdict.b_root.clone());
        self.edges
            .entry(verdict.b_root.clone())
            .or_default()
            .insert(verdict.a_root.clone());
    }

    /// How many distinct identity roots have vouched for `node`.
    pub fn degree(&self, node: &Did) -> usize {
        self.edges.get(node).map(HashSet::len).unwrap_or(0)
    }

    /// Every identity root with at least one recorded vouch.
    pub fn nodes(&self) -> impl Iterator<Item = &Did> {
        self.edges.keys()
    }

    /// The identity roots `node` has a mutual vouch with.
    pub fn neighbors(&self, node: &Did) -> impl Iterator<Item = &Did> {
        self.edges.get(node).into_iter().flatten()
    }

    /// Total distinct identity roots in the graph.
    pub fn node_count(&self) -> usize {
        self.edges.len()
    }
}

/// A reasonable default iteration count for a graph of `node_count` nodes,
/// following SybilRank's O(log n) guidance. Callers may always supply their
/// own — this is a starting point, not a frozen parameter.
pub fn recommended_iterations(node_count: usize) -> u32 {
    if node_count < 2 {
        return 1;
    }
    (node_count as f64).log2().ceil() as u32
}

/// Propagate trust outward from `seeds` for `iterations` rounds, returning
/// every reached node's accumulated trust mass. Nodes with no path to any
/// seed within `iterations` hops score zero.
///
/// Each round, every node's current trust splits evenly across its edges and
/// flows to its neighbors (integer division truncates, which only ever
/// under-counts — never fabricates trust). Seed nodes are not re-injected
/// each round (this is a bounded power iteration, not a random walk with
/// restart): after few rounds, distance from the trusted seed set dominates
/// the score, which is exactly the property that discounts Sybil clusters.
pub fn trust_scores(graph: &VouchGraph, seeds: &[Did], iterations: u32) -> HashMap<Did, u64> {
    let mut trust: HashMap<Did, u64> = HashMap::new();
    for seed in seeds {
        trust.insert(seed.clone(), TRUST_SCALE);
    }

    for _ in 0..iterations {
        let mut next: HashMap<Did, u64> = HashMap::new();
        for (node, mass) in &trust {
            let degree = graph.degree(node) as u64;
            if degree == 0 || *mass == 0 {
                continue;
            }
            let share = mass / degree;
            if share == 0 {
                continue;
            }
            for neighbor in graph.neighbors(node) {
                *next.entry(neighbor.clone()).or_insert(0) += share;
            }
        }
        trust = next;
    }

    trust
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did(seed: u8) -> Did {
        let c = seed;
        Controller::incept_single_from_seeds(&[c; 32], &[c.wrapping_add(1); 32])
            .unwrap()
            .did()
    }

    fn verdict(a: &Did, b: &Did) -> VouchVerdict {
        VouchVerdict {
            a_root: a.clone(),
            b_root: b.clone(),
            at_ms: 0,
        }
    }

    #[test]
    fn add_verdict_creates_an_undirected_edge() {
        let a = did(1);
        let b = did(2);
        let mut graph = VouchGraph::new();
        graph.add_verdict(&verdict(&a, &b));

        assert_eq!(graph.degree(&a), 1);
        assert_eq!(graph.degree(&b), 1);
        assert!(graph.neighbors(&a).any(|n| n == &b));
        assert!(graph.neighbors(&b).any(|n| n == &a));
    }

    #[test]
    fn degree_of_unknown_node_is_zero() {
        let graph = VouchGraph::new();
        assert_eq!(graph.degree(&did(9)), 0);
    }

    #[test]
    fn trust_propagates_outward_from_seed_along_a_chain() {
        // seed -- b -- c
        let seed = did(1);
        let b = did(2);
        let c = did(3);
        let mut graph = VouchGraph::new();
        graph.add_verdict(&verdict(&seed, &b));
        graph.add_verdict(&verdict(&b, &c));

        let scores = trust_scores(&graph, std::slice::from_ref(&seed), 2);
        // Round 1: all of seed's mass (degree 1) flows entirely to b, so
        // `trust` after round 1 is `{b: TRUST_SCALE}` (mass fully moves each
        // round, it does not accumulate at the sender — see trust_scores'
        // docs on why this is a bounded power iteration, not a restart walk).
        // Round 2: b (degree 2: seed and c) splits that mass evenly between
        // its two neighbors, so b itself ends this round with zero, while
        // seed and c each receive half.
        assert_eq!(scores.get(&b).copied().unwrap_or(0), 0);
        assert_eq!(scores.get(&seed).copied().unwrap_or(0), TRUST_SCALE / 2);
        assert_eq!(scores.get(&c).copied().unwrap_or(0), TRUST_SCALE / 2);
    }

    #[test]
    fn a_sybil_cluster_with_one_bridge_edge_scores_far_below_the_honest_region() {
        // Honest region: seed densely connected to h1..h4.
        let seed = did(1);
        let honest: Vec<Did> = (2..=5).map(did).collect();
        let mut graph = VouchGraph::new();
        for h in &honest {
            graph.add_verdict(&verdict(&seed, h));
        }
        for i in 0..honest.len() {
            for j in (i + 1)..honest.len() {
                graph.add_verdict(&verdict(&honest[i], &honest[j]));
            }
        }

        // Sybil cluster: densely connected among themselves, with exactly
        // one bridge edge from the honest region (h1 -- sy1).
        let sybils: Vec<Did> = (10..=19).map(did).collect();
        for i in 0..sybils.len() {
            for j in (i + 1)..sybils.len() {
                graph.add_verdict(&verdict(&sybils[i], &sybils[j]));
            }
        }
        graph.add_verdict(&verdict(&honest[0], &sybils[0]));

        let iterations = recommended_iterations(graph.node_count());
        let scores = trust_scores(&graph, std::slice::from_ref(&seed), iterations);

        let honest_avg: u64 = honest
            .iter()
            .map(|h| scores.get(h).copied().unwrap_or(0))
            .sum::<u64>()
            / honest.len() as u64;
        let sybil_avg: u64 = sybils
            .iter()
            .map(|s| scores.get(s).copied().unwrap_or(0))
            .sum::<u64>()
            / sybils.len() as u64;

        assert!(
            sybil_avg < honest_avg / 4,
            "sybil cluster should score far below the honest region: honest_avg={honest_avg} sybil_avg={sybil_avg}"
        );
    }

    #[test]
    fn recommended_iterations_grows_with_graph_size() {
        assert_eq!(recommended_iterations(0), 1);
        assert_eq!(recommended_iterations(1), 1);
        assert!(recommended_iterations(1000) > recommended_iterations(10));
    }
}

//! Reproducible adversarial simulation of the vouching-graph defence.
//!
//! Deterministic (hash-derived identities, a fixed LCG), offline, and
//! graph-level: signatures and replay are covered by `tests/vouch.rs`; this
//! asks the Sybil question — how much seed-anchored trust can a cluster of
//! fake identities collect when `k` colluding *mature* credentials each vouch
//! for it, under the per-root vouch quota and degree cap?
//!
//! Metric: a sybil "passes" if its trust reaches half the honest median.
//! This is a model of *one* signal (the vouching graph), not of the whole
//! score; the full policy additionally needs presence evidence, age and
//! several distinct live sources, so a graph pass is necessary, not
//! sufficient. Findings here are limits to report, not promises.

use did_mini::{Controller, Did};
use mini_uniqueness::{recommended_iterations, trust_scores, VouchGraph, VouchVerdict};
use std::collections::HashSet;

const HONEST: usize = 300;
const SEEDS: usize = 10;
const SYBILS: usize = 400;
const HONEST_VOUCHES: usize = 8;
/// Vouches one colluding mature credential can spend on the sybil cluster
/// (the early-stage per-epoch quota, spent in one epoch).
const QUOTA: usize = 10;
const DEGREE_CAP: usize = 150;

fn did(i: usize) -> Did {
    let a = mini_crypto::HashAlgorithm::Blake3.digest(&(i as u64).to_be_bytes());
    let b = mini_crypto::HashAlgorithm::Blake3.digest(&(i as u64 ^ 0xA5A5).to_be_bytes());
    Controller::incept_single_from_seeds(&a, &b).unwrap().did()
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

fn edge(g: &mut VouchGraph, a: &Did, b: &Did) -> bool {
    g.add_verdict_capped(
        &VouchVerdict {
            a_root: a.clone(),
            b_root: b.clone(),
            at_ms: 0,
        },
        DEGREE_CAP,
    )
}

struct Outcome {
    colluders: usize,
    attack_edges: usize,
    passing_sybils: usize,
    max_sybil_over_median_pct: u64,
}

fn run(colluders: usize) -> Outcome {
    let honest: Vec<Did> = (0..HONEST).map(did).collect();
    let sybils: Vec<Did> = (HONEST..HONEST + SYBILS).map(did).collect();
    let seeds = &honest[..SEEDS];
    let mut g = VouchGraph::new();
    let mut rng = Lcg(7);

    // Honest region: a ring (connectivity) plus random vouches.
    for i in 0..HONEST {
        edge(&mut g, &honest[i], &honest[(i + 1) % HONEST]);
        for _ in 0..HONEST_VOUCHES {
            let j = rng.next(HONEST);
            if j != i {
                edge(&mut g, &honest[i], &honest[j]);
            }
        }
    }
    // Sybil region: densely interconnected (best case for the attacker).
    for i in 0..SYBILS {
        for _ in 0..40 {
            let j = rng.next(SYBILS);
            if j != i {
                edge(&mut g, &sybils[i], &sybils[j]);
            }
        }
    }
    // Attack edges: each colluding mature credential spends its quota on the
    // sybil cluster, spread over distinct sybils.
    let mut attack_edges = 0;
    let mut used = HashSet::new();
    for c in 0..colluders {
        let colluder = &honest[SEEDS + (c * 7) % (HONEST - SEEDS)];
        let mut spent = 0;
        let mut tries = 0;
        while spent < QUOTA && tries < 10 * SYBILS {
            tries += 1;
            let s = rng.next(SYBILS);
            if used.insert((colluder.as_str().to_string(), s)) && edge(&mut g, colluder, &sybils[s])
            {
                spent += 1;
                attack_edges += 1;
            }
        }
    }

    let iters = recommended_iterations(g.node_count());
    let trust = trust_scores(&g, seeds, iters);
    let mut honest_t: Vec<u64> = honest
        .iter()
        .map(|d| trust.get(d).copied().unwrap_or(0))
        .collect();
    honest_t.sort_unstable();
    let median = honest_t[HONEST / 2].max(1);
    let sybil_t: Vec<u64> = sybils
        .iter()
        .map(|d| trust.get(d).copied().unwrap_or(0))
        .collect();
    Outcome {
        colluders,
        attack_edges,
        passing_sybils: sybil_t.iter().filter(|t| **t * 2 >= median).count(),
        max_sybil_over_median_pct: sybil_t.iter().max().copied().unwrap_or(0) * 100 / median,
    }
}

#[test]
fn few_colluding_mature_credentials_cannot_lift_a_sybil_cluster() {
    let results: Vec<Outcome> = [0, 1, 6, 24, 60, 100, 200]
        .iter()
        .map(|k| run(*k))
        .collect();
    for r in &results {
        println!(
            "colluders={:>3} attack_edges={:>4} passing_sybils={:>3}/{} max_sybil/median={:>4}%",
            r.colluders, r.attack_edges, r.passing_sybils, SYBILS, r.max_sybil_over_median_pct
        );
    }
    // With no colluders nothing reaches the cluster.
    assert_eq!(results[0].passing_sybils, 0);
    assert_eq!(results[0].max_sybil_over_median_pct, 0);
    // One and six colluders (the bar the audit research names) cannot make
    // any sybil look like a typical honest account.
    for r in &results[1..3] {
        assert_eq!(r.passing_sybils, 0, "k={}", r.colluders);
    }
    // Trust collected by the cluster never decreases as collusion grows.
    for w in results.windows(2) {
        assert!(w[1].max_sybil_over_median_pct >= w[0].max_sybil_over_median_pct);
    }
    // Attack edges are bounded by colluders x quota (the defence's lever).
    for r in &results {
        assert!(r.attack_edges <= r.colluders * QUOTA);
    }
}

#[test]
fn mass_collusion_is_a_documented_limit_of_the_graph_signal_alone() {
    // 200 colluding mature credentials = 2000 attack edges for 400 sybils.
    // This test records, honestly, that the graph signal alone does not stop
    // that: the other signals (presence, age, distinct sources) and the
    // economics must. If this ever starts failing because the number fell to
    // zero, the model changed — update the design note, do not delete it.
    let r = run(200);
    println!(
        "mass collusion: {} sybils pass the graph signal alone",
        r.passing_sybils
    );
    assert!(r.passing_sybils > 0);
}

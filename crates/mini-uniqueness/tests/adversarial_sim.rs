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
use mini_uniqueness::{
    recommended_iterations, trust_scores, GateOutcome, HumanRecord, PersonhoodPolicy,
    SignalEvidence, SignalSource, VouchGraph, VouchVerdict,
};
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
    /// Each sybil's vouching strength 0..=100 (trust relative to the honest median).
    strengths: Vec<u32>,
    colluders: usize,
    attack_edges: usize,
    passing_sybils: usize,
    max_sybil_over_median_pct: u64,
}

fn run(colluders: usize) -> Outcome {
    run_with(colluders, HONEST, QUOTA)
}

fn run_with(colluders: usize, honest_n: usize, quota: usize) -> Outcome {
    let honest: Vec<Did> = (0..honest_n).map(did).collect();
    let sybils: Vec<Did> = (honest_n..honest_n + SYBILS).map(did).collect();
    let seeds = &honest[..SEEDS];
    let mut g = VouchGraph::new();
    let mut rng = Lcg(7);

    // Honest region: a ring (connectivity) plus random vouches.
    for i in 0..honest_n {
        edge(&mut g, &honest[i], &honest[(i + 1) % honest_n]);
        for _ in 0..HONEST_VOUCHES {
            let j = rng.next(honest_n);
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
        let colluder = &honest[SEEDS + (c * 7) % (honest_n - SEEDS)];
        let mut spent = 0;
        let mut tries = 0;
        while spent < quota && tries < 10 * SYBILS {
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
    let median = honest_t[honest_n / 2].max(1);
    let sybil_t: Vec<u64> = sybils
        .iter()
        .map(|d| trust.get(d).copied().unwrap_or(0))
        .collect();
    let strengths = sybil_t
        .iter()
        .map(|t| (*t * 100 / median).min(100) as u32)
        .collect();
    Outcome {
        strengths,
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

/// Smallest number of colluding mature credentials (scanned in steps of 4)
/// for which at least a tenth of the cluster passes the graph signal; this is
/// the "price of a Sybil cluster" measured in bought or stolen witnesses.
fn colluders_to_lift_cluster(honest_n: usize, quota: usize) -> usize {
    (0..=honest_n / 2)
        .step_by(4)
        .find(|k| run_with(*k, honest_n, quota).passing_sybils * 10 >= SYBILS)
        .unwrap_or(usize::MAX)
}

#[test]
fn price_of_a_cluster_rises_with_a_stricter_quota_and_a_larger_honest_network() {
    // Paid real-human witnesses and stolen credentials both reduce, for the
    // graph signal, to "colluding mature credentials"; what differs is what
    // each costs the attacker, which the audit and economics must price.
    let early = colluders_to_lift_cluster(300, 10); // early stage: 10 vouches/epoch
    let mature = colluders_to_lift_cluster(300, 5); // mature stage (epoch >= 1000): 5
    let larger = colluders_to_lift_cluster(600, 10);
    println!(
        "colluders needed to lift 10% of the cluster: early(300,q10)={early} \
         mature(300,q5)={mature} larger(600,q10)={larger}"
    );
    assert!(early >= 4 && early != usize::MAX);
    // Halving the quota (the stricter stage) cannot make the attack cheaper,
    // and a larger honest network cannot either.
    assert!(mature >= early, "mature {mature} < early {early}");
    assert!(larger >= early, "larger {larger} < early {early}");
}

fn record(vouch: Option<u32>, presence: u32) -> HumanRecord {
    let mut r = HumanRecord::new();
    r.record(SignalEvidence {
        source: SignalSource::PhysicalPresence,
        strength: presence,
        recorded_at_ms: 1_000,
    });
    if let Some(v) = vouch {
        r.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: v,
            recorded_at_ms: 1_000,
        });
    }
    r
}

#[test]
fn a_farm_with_forged_or_relayed_presence_but_no_graph_trust_never_even_reaches_audit() {
    // Forged devices attesting each other, or an active proximity relay, can
    // make presence evidence look perfect. Without live vouching evidence the
    // policy caps the score below the audit window, however many identities
    // the farm mints.
    let policy = PersonhoodPolicy::starting_point();
    for _ in 0..1_000 {
        let score = policy.score(&record(None, 100), 2_000);
        assert_eq!(policy.gate().evaluate(score), GateOutcome::Insufficient);
    }
}

#[test]
fn forged_presence_cannot_turn_a_trace_of_graph_trust_into_a_pass() {
    // Colluding mature credentials give the cluster some vouching strength;
    // the attacker also forges perfect presence for every sybil. The policy
    // lets presence lift a score only PRESENCE_LIFT points above the vouching
    // strength, so a small amount of graph trust no longer passes.
    let policy = PersonhoodPolicy::starting_point();
    let mut table = Vec::new();
    for k in [0usize, 1, 6, 24] {
        let o = run(k);
        let count = |want: GateOutcome| {
            o.strengths
                .iter()
                .filter(|v| {
                    let score = policy.score(&record(Some(**v), 100), 2_000);
                    policy.gate().evaluate(score) == want
                })
                .count()
        };
        let (pass, audit) = (count(GateOutcome::Pass), count(GateOutcome::NeedsAudit));
        table.push((k, pass, audit));
        println!(
            "forged presence + {k:>2} colluders: {pass} pass, {audit} reach audit, of {SYBILS}"
        );
    }
    // No graph trust: nothing passes. One and six colluders (the audit
    // research's bar) cannot pass a single sybil even with perfect presence.
    assert_eq!(table[0].1, 0);
    assert_eq!(table[1].1, 0);
    assert_eq!(table[2].1, 0);
    // More collusion never helps the defender.
    assert!(table.windows(2).all(|w| w[1].1 >= w[0].1));
}

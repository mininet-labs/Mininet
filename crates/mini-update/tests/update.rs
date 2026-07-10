//! Integration tests: the local adoption state machine over a real governed
//! release (project → PR → approve → merge → release → attest), exactly the
//! chain `mini-forge`'s own governance tests build — proving `mini-update`
//! adds a state machine on top without weakening any gate underneath it.

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_forge::{
    approve, attest, commit, merge, project, propose, put_file, put_tree, release, KelDirectory,
    Policy, ReleasePolicy, TreeEntry,
};
use mini_media::publish_media;
use mini_objects::{Object, ObjectId};
use mini_provenance::BuildProvenance;
use mini_store::{MemoryBackend, Store};
use mini_update::{
    AdoptionDecision, AdoptionState, FreshnessPolicy, NotYetReason, ProvenancePolicy,
};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn a_commit(
    store: &mut Store<MemoryBackend>,
    h: &Did,
    d: &Controller,
    text: &[u8],
    seq: u64,
) -> Object {
    let f = put_file(store, h, d, text).unwrap();
    let t = put_tree(
        store,
        h,
        d,
        &[TreeEntry {
            name: "f.rs".into(),
            is_dir: false,
            target: f,
        }],
    )
    .unwrap();
    commit(store, h, d, "change", &t, &[], 100, seq).unwrap()
}

/// A fully governed, released, and (optionally) attested world: two
/// maintainers, a merged PR, and a release naming the merged commit as its
/// source. `attestations` controls how many *distinct* identity roots attest
/// the artifact digest (0..=2 for these tests).
struct World {
    store: Store<MemoryBackend>,
    oracle: KelDirectory,
    proj: ObjectId,
    release_id: ObjectId,
    release_ts: u64,
}

fn governed_release(attestations: u32) -> World {
    let (author, author_dev) = human(10);
    let (m1, m1_dev) = human(20);
    let (m2, m2_dev) = human(30);
    let mut store = Store::new(MemoryBackend::new());

    let policy = Policy {
        min_approvals: 2,
        maintainers: vec![m1.did(), m2.did()],
    };
    let proj = project(&mut store, &m1.did(), &m1_dev, "core", &policy)
        .unwrap()
        .id()
        .clone();

    let head = a_commit(&mut store, &author.did(), &author_dev, b"work", 1);
    let pr = propose(
        &mut store,
        &author.did(),
        &author_dev,
        &proj,
        "main",
        "add work",
        head.id(),
        &proj,
        100,
        1,
    )
    .unwrap();
    approve(
        &mut store,
        &m1.did(),
        &m1_dev,
        pr.id(),
        head.id(),
        true,
        200,
        1,
    )
    .unwrap();
    approve(
        &mut store,
        &m2.did(),
        &m2_dev,
        pr.id(),
        head.id(),
        true,
        300,
        1,
    )
    .unwrap();
    merge(
        &mut store,
        &m1.did(),
        &m1_dev,
        &proj,
        &proj,
        pr.id(),
        400,
        1,
    )
    .unwrap();

    let artifact = b"reproducible binary bytes".to_vec();
    let digest = HashAlgorithm::Blake3.digest(&artifact);
    let manifest = publish_media(
        &mut store,
        &author.did(),
        &author_dev,
        "application/octet-stream",
        &artifact,
        500,
        1,
    )
    .unwrap();
    let release_ts = 600;
    let rel = release(
        &mut store,
        &author.did(),
        &author_dev,
        "1.0.0",
        &proj,
        "main",
        head.id(),
        &manifest.id,
        digest,
        [9u8; 32],
        release_ts,
        2,
    )
    .unwrap();

    let mut oracle = KelDirectory::new();
    for c in [&author, &author_dev, &m1, &m1_dev, &m2, &m2_dev] {
        oracle.insert(c.kel());
    }
    for (i, seed) in [50u8, 90]
        .into_iter()
        .take(attestations as usize)
        .enumerate()
    {
        let (r, d) = human(seed);
        attest(
            &mut store,
            &r.did(),
            &d,
            rel.id(),
            digest,
            700 + i as u64,
            1,
        )
        .unwrap();
        oracle.insert(r.kel());
        oracle.insert(d.kel());
    }

    World {
        store,
        oracle,
        proj,
        release_id: rel.id().clone(),
        release_ts,
    }
}

fn passing_policy(w: &World) -> ReleasePolicy {
    ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: w.release_ts + 3_600_001,
    }
}

/// A generous freshness policy plus "just synced" `last_synced_ms`, for
/// tests that are not exercising the freshness gate itself.
fn passing_freshness(policy: &ReleasePolicy) -> (FreshnessPolicy, u64) {
    (
        FreshnessPolicy {
            max_staleness_ms: mini_update::FRESHNESS_MAX_ALLOWED_STALENESS_MS,
        },
        policy.now_ms,
    )
}

#[test]
fn a_fully_governed_release_is_adoptable() {
    let w = governed_release(2);
    let state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    match decision {
        AdoptionDecision::Adoptable(v) => assert_eq!(v.version, "1.0.0"),
        other => panic!("expected Adoptable, got {other:?}"),
    }
}

#[test]
fn timelock_still_active_is_not_yet_adoptable_not_rejected() {
    let w = governed_release(2);
    let mut policy = passing_policy(&w);
    policy.now_ms = w.release_ts + 1; // far short of the timelock
    let state = AdoptionState::new();
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    assert_eq!(
        decision,
        AdoptionDecision::NotYetAdoptable(NotYetReason::TimelockActive)
    );
}

#[test]
fn too_few_attestations_is_not_yet_adoptable_not_rejected() {
    let w = governed_release(1); // only one attester so far
    let state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    assert_eq!(
        decision,
        AdoptionDecision::NotYetAdoptable(NotYetReason::NotEnoughAttestations {
            needed: 2,
            got: 1
        })
    );
}

#[test]
fn adopt_recomputes_from_scratch_and_updates_running() {
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    assert!(state.running.is_none());

    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let verified = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .unwrap();
    assert_eq!(verified.version, "1.0.0");
    assert_eq!(state.running, Some(w.release_id.clone()));
}

#[test]
fn adopt_fails_when_the_release_is_not_actually_adoptable() {
    let w = governed_release(1); // not enough attestations
    let mut state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    assert!(state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .is_err());
    // Nothing was installed/marked running on failure.
    assert!(state.running.is_none());
}

#[test]
fn refusing_a_release_never_blocks_ordinary_operation_and_evaluate_reports_it() {
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    state.refuse(&w.release_id);

    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    assert_eq!(decision, AdoptionDecision::Refused);
    // The device is still on whatever it was running before (nothing here
    // ever forces a state change).
    assert!(state.running.is_none());
}

#[test]
fn a_refused_release_can_still_be_explicitly_adopted_later() {
    // Refusal is a note for evaluate()'s UI-facing shortcut, never a hard
    // block underneath: choosing to adopt is just calling adopt() directly,
    // which re-checks everything fresh regardless of any prior refusal.
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    state.refuse(&w.release_id);

    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let verified = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .unwrap();
    assert_eq!(verified.version, "1.0.0");
    assert_eq!(state.running, Some(w.release_id.clone()));
}

#[test]
fn a_release_from_a_never_merged_commit_is_rejected_not_deferred() {
    let w = governed_release(2);
    let (author, author_dev) = human(10);

    // A release naming a commit that never went through PR/approve/merge
    // governance, built in the same governed project's store.
    let mut store = w.store;
    let rogue = a_commit(&mut store, &author.did(), &author_dev, b"rogue", 50);
    let artifact = b"rogue binary".to_vec();
    let digest = HashAlgorithm::Blake3.digest(&artifact);
    let manifest = publish_media(
        &mut store,
        &author.did(),
        &author_dev,
        "application/octet-stream",
        &artifact,
        800,
        1,
    )
    .unwrap();
    let rogue_rel = release(
        &mut store,
        &author.did(),
        &author_dev,
        "1.0.1",
        &w.proj,
        "main",
        rogue.id(),
        &manifest.id,
        digest,
        [1u8; 32],
        900,
        3,
    )
    .unwrap();
    for seed in [50u8, 90] {
        let (r, d) = human(seed);
        attest(&mut store, &r.did(), &d, rogue_rel.id(), digest, 950, 1).unwrap();
    }

    let state = AdoptionState::new();
    let policy = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: 900 + 3_600_001,
    };
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    let decision = state.evaluate(
        &store,
        &w.oracle,
        rogue_rel.id(),
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    match decision {
        AdoptionDecision::Rejected(_) => {}
        other => panic!("expected Rejected, got {other:?}"),
    }
}

#[test]
fn a_stale_view_is_refused_before_any_governance_check_runs() {
    let w = governed_release(2);
    let state = AdoptionState::new();
    let policy = passing_policy(&w);
    // Claims to have synced far longer ago than the freshness ceiling allows.
    let freshness = FreshnessPolicy {
        max_staleness_ms: 60_000,
    };
    let last_synced_ms = 0;
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    assert_eq!(
        decision,
        AdoptionDecision::ViewTooStale {
            last_synced_ms: 0,
            max_staleness_ms: 60_000,
        }
    );
}

#[test]
fn adopt_also_refuses_a_stale_view() {
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    let policy = passing_policy(&w);
    let freshness = FreshnessPolicy {
        max_staleness_ms: 60_000,
    };
    let err = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            0,
        )
        .unwrap_err();
    assert!(matches!(err, mini_update::AdoptError::ViewTooStale { .. }));
    assert!(state.running.is_none());
}

/// Build a second, fully governed release under `w.proj`/"main" at
/// `version`, reusing the same maintainers/author `governed_release` set
/// up (deterministic seeds 10/20/30 reproduce the identical identities the
/// world's oracle already trusts).
#[allow(clippy::too_many_arguments)]
fn second_governed_release(
    store: &mut Store<MemoryBackend>,
    oracle: &KelDirectory,
    proj: &ObjectId,
    parent_commit: &ObjectId,
    version: &str,
    commit_seed: u8,
    ts_base: u64,
    seq_base: u64,
) -> (ObjectId, u64) {
    let (author, author_dev) = human(10);
    let (m1, m1_dev) = human(20);
    let (m2, m2_dev) = human(30);

    // Builds on `parent_commit` (the current canonical head) rather than a
    // fresh root -- two merged PRs whose commits don't chain from each
    // other would be a genuine governance fork, which `verify_governed_release`
    // correctly refuses regardless of version ordering.
    let f = put_file(store, &author.did(), &author_dev, &[commit_seed]).unwrap();
    let t = put_tree(
        store,
        &author.did(),
        &author_dev,
        &[TreeEntry {
            name: "f.rs".into(),
            is_dir: false,
            target: f,
        }],
    )
    .unwrap();
    let head = commit(
        store,
        &author.did(),
        &author_dev,
        "second change",
        &t,
        std::slice::from_ref(parent_commit),
        ts_base,
        seq_base,
    )
    .unwrap();
    // A merge entry's validity requires `pr.base == entry.prev` (lineage:
    // the PR must have been built against the exact chain position it is
    // being merged onto) -- so `base` here must be the CURRENT governance
    // chain tip, computed fresh, not the project genesis id.
    let tip = mini_forge::resolve_project(store, oracle, proj)
        .unwrap()
        .tip;
    let pr = propose(
        store,
        &author.did(),
        &author_dev,
        proj,
        "main",
        "second change",
        head.id(),
        &tip,
        ts_base,
        seq_base + 1,
    )
    .unwrap();
    approve(
        store,
        &m1.did(),
        &m1_dev,
        pr.id(),
        head.id(),
        true,
        ts_base + 100,
        seq_base + 1,
    )
    .unwrap();
    approve(
        store,
        &m2.did(),
        &m2_dev,
        pr.id(),
        head.id(),
        true,
        ts_base + 200,
        seq_base + 1,
    )
    .unwrap();
    // `merge`'s `prev` must be the same chain tip the PR's `base` named
    // above -- reusing `tip` here (rather than resolving again) also
    // guarantees that, since nothing else touched the chain in between.
    merge(
        store,
        &m1.did(),
        &m1_dev,
        proj,
        &tip,
        pr.id(),
        ts_base + 300,
        seq_base + 2,
    )
    .unwrap();

    let artifact = format!("binary for {version}").into_bytes();
    let digest = HashAlgorithm::Blake3.digest(&artifact);
    let manifest = publish_media(
        store,
        &author.did(),
        &author_dev,
        "application/octet-stream",
        &artifact,
        ts_base + 400,
        seq_base + 2,
    )
    .unwrap();
    let release_ts = ts_base + 500;
    let rel = release(
        store,
        &author.did(),
        &author_dev,
        version,
        proj,
        "main",
        head.id(),
        &manifest.id,
        digest,
        [commit_seed; 32],
        release_ts,
        seq_base + 3,
    )
    .unwrap();
    for (i, seed) in [50u8, 90].into_iter().enumerate() {
        let (r, d) = human(seed);
        attest(
            store,
            &r.did(),
            &d,
            rel.id(),
            digest,
            release_ts + 600 + i as u64,
            seq_base + 4,
        )
        .unwrap();
    }
    (rel.id().clone(), release_ts)
}

#[test]
fn a_rollback_candidate_is_rejected_even_though_every_other_gate_passes() {
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .unwrap();
    assert_eq!(state.running, Some(w.release_id.clone()));

    let mut store = w.store;
    let parent = store
        .get(&w.release_id)
        .unwrap()
        .links
        .iter()
        .find(|l| l.rel == "commit")
        .unwrap()
        .target
        .clone();
    let (rel2_id, rel2_ts) = second_governed_release(
        &mut store, &w.oracle, &w.proj, &parent, "0.9.0", 50, 10_000, 10,
    );
    let policy2 = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: rel2_ts + 3_600_001,
    };
    let (freshness2, last_synced_ms2) = passing_freshness(&policy2);

    let decision = state.evaluate(
        &store,
        &w.oracle,
        &rel2_id,
        &w.proj,
        "main",
        &policy2,
        &freshness2,
        last_synced_ms2,
    );
    assert!(
        matches!(
            decision,
            AdoptionDecision::Rejected(mini_forge::ForgeError::RollbackRejected)
        ),
        "expected RollbackRejected, got {decision:?}"
    );

    assert!(state
        .adopt(
            &store,
            &w.oracle,
            &rel2_id,
            &w.proj,
            "main",
            &policy2,
            &freshness2,
            last_synced_ms2,
        )
        .is_err());
    // The device stays on the higher version it already adopted.
    assert_eq!(state.running, Some(w.release_id.clone()));
}

#[test]
fn a_genuine_upgrade_after_adopting_is_accepted() {
    let w = governed_release(2);
    let mut state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);
    state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .unwrap();

    let mut store = w.store;
    let parent = store
        .get(&w.release_id)
        .unwrap()
        .links
        .iter()
        .find(|l| l.rel == "commit")
        .unwrap()
        .target
        .clone();
    let (rel2_id, rel2_ts) = second_governed_release(
        &mut store, &w.oracle, &w.proj, &parent, "1.1.0", 60, 10_000, 10,
    );
    let policy2 = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 3_600_000,
        now_ms: rel2_ts + 3_600_001,
    };
    let (freshness2, last_synced_ms2) = passing_freshness(&policy2);

    let verified = state
        .adopt(
            &store,
            &w.oracle,
            &rel2_id,
            &w.proj,
            "main",
            &policy2,
            &freshness2,
            last_synced_ms2,
        )
        .unwrap();
    assert_eq!(verified.version, "1.1.0");
    assert_eq!(state.running, Some(rel2_id));
}

/// The release's source commit, the subject `mini-provenance` claims are
/// recorded against (same lookup the rollback tests already use).
fn release_source_commit(store: &Store<MemoryBackend>, release_id: &ObjectId) -> ObjectId {
    store
        .get(release_id)
        .unwrap()
        .links
        .iter()
        .find(|l| l.rel == "commit")
        .unwrap()
        .target
        .clone()
}

fn a_build_claim(output_digest: [u8; 32]) -> BuildProvenance {
    BuildProvenance {
        environment_digest: [1u8; 32],
        commands_digest: [2u8; 32],
        output_digests: vec![output_digest],
        reproducibility_group: "linux-x86_64-rustc".into(),
        network_enabled: false,
        started_ms: 100,
        finished_ms: 200,
    }
}

#[test]
fn too_few_independent_provenance_agreements_is_not_yet_adoptable_not_rejected() {
    let mut w = governed_release(2);
    let state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);

    // Establish the expected artifact digest via the plain gate, exactly as
    // a real caller would before layering the provenance gate on top.
    let plain = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    let AdoptionDecision::Adoptable(verified) = plain else {
        panic!("expected Adoptable from the plain gate, got {plain:?}");
    };
    let digest = verified.artifact.digest;
    let source_commit = release_source_commit(&w.store, &w.release_id);

    // Only one independent builder (excluding the release's own author,
    // seed 10) records a matching claim.
    let (b1, b1_dev) = human(70);
    w.oracle.insert(b1.kel());
    w.oracle.insert(b1_dev.kel());
    mini_provenance::record_provenance(
        &mut w.store,
        &b1.did(),
        &b1_dev,
        &source_commit,
        &a_build_claim(digest),
        800,
        1,
    )
    .unwrap();

    let provenance_policy = ProvenancePolicy {
        min_independent_builders: 2,
    };
    let decision = state.evaluate_with_provenance(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
        &provenance_policy,
    );
    assert_eq!(
        decision,
        AdoptionDecision::NotYetAdoptable(NotYetReason::NotEnoughProvenanceAgreement {
            needed: 2,
            got: 1,
        })
    );
}

#[test]
fn enough_independent_provenance_agreement_makes_the_release_adoptable() {
    let mut w = governed_release(2);
    let mut state = AdoptionState::new();
    let policy = passing_policy(&w);
    let (freshness, last_synced_ms) = passing_freshness(&policy);

    let plain = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
    );
    let AdoptionDecision::Adoptable(verified) = plain else {
        panic!("expected Adoptable from the plain gate, got {plain:?}");
    };
    let digest = verified.artifact.digest;
    let source_commit = release_source_commit(&w.store, &w.release_id);

    // Two distinct, independent builders (neither the release's own author,
    // seed 10) both agree on the artifact digest.
    for (i, seed) in [70u8, 80].into_iter().enumerate() {
        let (b, b_dev) = human(seed);
        w.oracle.insert(b.kel());
        w.oracle.insert(b_dev.kel());
        mini_provenance::record_provenance(
            &mut w.store,
            &b.did(),
            &b_dev,
            &source_commit,
            &a_build_claim(digest),
            800 + i as u64,
            1,
        )
        .unwrap();
    }

    let provenance_policy = ProvenancePolicy {
        min_independent_builders: 2,
    };
    let decision = state.evaluate_with_provenance(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &policy,
        &freshness,
        last_synced_ms,
        &provenance_policy,
    );
    match decision {
        AdoptionDecision::Adoptable(v) => assert_eq!(v.version, "1.0.0"),
        other => panic!("expected Adoptable, got {other:?}"),
    }

    // The provenance gate is additive: `adopt()` itself is unaffected by it
    // (it only wires the mini-forge gates), so ordinary adoption still
    // succeeds once evaluate_with_provenance has confirmed the quorum.
    let verified = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &policy,
            &freshness,
            last_synced_ms,
        )
        .unwrap();
    assert_eq!(verified.version, "1.0.0");
}

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
use mini_store::{MemoryBackend, Store};
use mini_update::{AdoptionDecision, AdoptionState, NotYetReason};

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

#[test]
fn a_fully_governed_release_is_adoptable() {
    let w = governed_release(2);
    let state = AdoptionState::new();
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &passing_policy(&w),
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
    let decision = state.evaluate(&w.store, &w.oracle, &w.release_id, &w.proj, "main", &policy);
    assert_eq!(
        decision,
        AdoptionDecision::NotYetAdoptable(NotYetReason::TimelockActive)
    );
}

#[test]
fn too_few_attestations_is_not_yet_adoptable_not_rejected() {
    let w = governed_release(1); // only one attester so far
    let state = AdoptionState::new();
    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &passing_policy(&w),
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

    let verified = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &passing_policy(&w),
        )
        .unwrap();
    assert_eq!(verified.version, "1.0.0");
    assert_eq!(state.running, Some(w.release_id.clone()));
}

#[test]
fn adopt_fails_when_the_release_is_not_actually_adoptable() {
    let w = governed_release(1); // not enough attestations
    let mut state = AdoptionState::new();
    assert!(state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &passing_policy(&w)
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

    let decision = state.evaluate(
        &w.store,
        &w.oracle,
        &w.release_id,
        &w.proj,
        "main",
        &passing_policy(&w),
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

    let verified = state
        .adopt(
            &w.store,
            &w.oracle,
            &w.release_id,
            &w.proj,
            "main",
            &passing_policy(&w),
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
    let decision = state.evaluate(&store, &w.oracle, rogue_rel.id(), &w.proj, "main", &policy);
    match decision {
        AdoptionDecision::Rejected(_) => {}
        other => panic!("expected Rejected, got {other:?}"),
    }
}

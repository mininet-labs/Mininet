//! Integration tests: a nested repo committed and checked out; a release gated
//! by timelock, artifact completeness, and — the guarantee that matters —
//! attestations counted per verified identity root: many devices count once, the
//! author's own attestation never counts, and balances appear nowhere.
//! (Identity root, not human: personhood is SPEC-02, unimplemented — D-0030.)

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_forge::{
    attest, checkout, commit, project, put_file, put_tree, release, resolve_branch, set_branch,
    verify_release_artifact_only, ForgeError, KelDirectory, Policy, ReleasePolicy, TreeEntry,
};
use mini_media::publish_media;
use mini_store::{MemoryBackend, Store};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn oracle_of(kels: &[&Controller]) -> KelDirectory {
    let mut dir = KelDirectory::new();
    for c in kels {
        dir.insert(c.kel());
    }
    dir
}

fn second_device(root: &mut Controller, seed: u8) -> Controller {
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed; 32], &[seed + 1; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    device
}

#[test]
fn repo_commits_branch_moves_and_checkout_roundtrips() {
    let (root, device) = human(10);
    let h = root.did();
    let mut store = Store::new(MemoryBackend::new());

    // src/lib.rs + README.md, nested.
    let lib = put_file(&mut store, &h, &device, b"pub fn hello() {}").unwrap();
    let src = put_tree(
        &mut store,
        &h,
        &device,
        &[TreeEntry {
            name: "lib.rs".into(),
            is_dir: false,
            target: lib,
        }],
    )
    .unwrap();
    let readme = put_file(&mut store, &h, &device, b"# mininet").unwrap();
    let tree_v1 = put_tree(
        &mut store,
        &h,
        &device,
        &[
            TreeEntry {
                name: "src".into(),
                is_dir: true,
                target: src.clone(),
            },
            TreeEntry {
                name: "README.md".into(),
                is_dir: false,
                target: readme,
            },
        ],
    )
    .unwrap();

    let c1 = commit(&mut store, &h, &device, "init", &tree_v1, &[], 100, 1).unwrap();
    set_branch(&mut store, &h, &device, "main", c1.id(), 1).unwrap();
    assert_eq!(
        resolve_branch(&store, &h, "main").unwrap(),
        Some(c1.id().clone())
    );

    // Second commit edits the README; branch advances.
    let readme2 = put_file(&mut store, &h, &device, b"# mininet v2").unwrap();
    let tree_v2 = put_tree(
        &mut store,
        &h,
        &device,
        &[
            TreeEntry {
                name: "src".into(),
                is_dir: true,
                target: src,
            },
            TreeEntry {
                name: "README.md".into(),
                is_dir: false,
                target: readme2,
            },
        ],
    )
    .unwrap();
    let c2 = commit(
        &mut store,
        &h,
        &device,
        "edit readme",
        &tree_v2,
        &[c1.id().clone()],
        200,
        2,
    )
    .unwrap();
    set_branch(&mut store, &h, &device, "main", c2.id(), 2).unwrap();

    let head = resolve_branch(&store, &h, "main").unwrap().unwrap();
    let files = checkout(&store, &head).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.contains(&("README.md".to_string(), b"# mininet v2".to_vec())));
    assert!(files.contains(&("src/lib.rs".to_string(), b"pub fn hello() {}".to_vec())));

    // Old commit remains checkout-able: history is content-addressed, not lost.
    let old = checkout(&store, c1.id()).unwrap();
    assert!(old.contains(&("README.md".to_string(), b"# mininet".to_vec())));
}

/// Build a store containing a release by `author` plus its artifact; return
/// (store, release id, artifact digest, release timestamp).
fn released(
    author: &Did,
    dev: &Controller,
) -> (Store<MemoryBackend>, mini_objects::ObjectId, [u8; 32], u64) {
    let mut store = Store::new(MemoryBackend::new());
    let artifact_bytes = b"the reproducible binary".to_vec();
    let digest = HashAlgorithm::Blake3.digest(&artifact_bytes);
    let manifest = publish_media(
        &mut store,
        author,
        dev,
        "application/octet-stream",
        &artifact_bytes,
        50,
        1,
    )
    .unwrap();
    let src = put_file(&mut store, author, dev, b"source").unwrap();
    let tree = put_tree(
        &mut store,
        author,
        dev,
        &[TreeEntry {
            name: "main.rs".into(),
            is_dir: false,
            target: src,
        }],
    )
    .unwrap();
    let c = commit(&mut store, author, dev, "release commit", &tree, &[], 90, 2).unwrap();
    let proj = project(
        &mut store,
        author,
        dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();
    let rel = release(
        &mut store,
        author,
        dev,
        "0.1.0",
        proj.id(),
        "main",
        c.id(),
        &manifest.id,
        digest,
        [7u8; 32],
        1_000,
        3,
    )
    .unwrap();
    (store, rel.id().clone(), digest, 1_000)
}

#[test]
fn release_verifies_with_independent_humans_and_gates_hold() {
    let (author_root, author_dev) = human(10);
    let (mut store, rel_id, digest, rel_ts) = released(&author_root.did(), &author_dev);

    let policy = |now: u64| ReleasePolicy {
        min_attestations: 3,
        timelock_ms: 10_000,
        now_ms: now,
    };

    // Three independent humans attest.
    let mut oracle = oracle_of(&[&author_root, &author_dev]);
    for seed in [50u8, 90, 130] {
        let (r, d) = human(seed);
        attest(&mut store, &r.did(), &d, &rel_id, digest, 2_000, 1).unwrap();
        oracle.insert(r.kel());
        oracle.insert(d.kel());
    }

    // Timelock still active: refused even with enough attestations.
    assert_eq!(
        verify_release_artifact_only(&store, &oracle, &rel_id, &policy(rel_ts + 9_999)),
        Err(ForgeError::TimelockActive)
    );

    // After the timelock: verified, artifact complete, 3 attesters counted.
    let v =
        verify_release_artifact_only(&store, &oracle, &rel_id, &policy(rel_ts + 10_000)).unwrap();
    assert_eq!(v.version, "0.1.0");
    assert_eq!(v.attesters, 3);
}

#[test]
fn one_human_many_devices_counts_once_and_author_never_counts() {
    let (author_root, author_dev) = human(10);
    let (mut store, rel_id, digest, rel_ts) = released(&author_root.did(), &author_dev);

    // One human attests from THREE devices — still one attestation.
    let (mut sybil_root, sybil_d1) = human(50);
    let sybil_d2 = second_device(&mut sybil_root, 70);
    let sybil_d3 = second_device(&mut sybil_root, 80);
    attest(
        &mut store,
        &sybil_root.did(),
        &sybil_d1,
        &rel_id,
        digest,
        2_000,
        1,
    )
    .unwrap();
    attest(
        &mut store,
        &sybil_root.did(),
        &sybil_d2,
        &rel_id,
        digest,
        2_001,
        2,
    )
    .unwrap();
    attest(
        &mut store,
        &sybil_root.did(),
        &sybil_d3,
        &rel_id,
        digest,
        2_002,
        3,
    )
    .unwrap();

    // The AUTHOR attests their own release — never counts.
    attest(
        &mut store,
        &author_root.did(),
        &author_dev,
        &rel_id,
        digest,
        2_003,
        4,
    )
    .unwrap();

    // A wrong-digest attestation — never counts.
    let (other_root, other_dev) = human(90);
    attest(
        &mut store,
        &other_root.did(),
        &other_dev,
        &rel_id,
        [0u8; 32],
        2_004,
        1,
    )
    .unwrap();

    let oracle = oracle_of(&[
        &author_root,
        &author_dev,
        &sybil_root,
        &sybil_d1,
        &sybil_d2,
        &sybil_d3,
        &other_root,
        &other_dev,
    ]);
    let policy = ReleasePolicy {
        min_attestations: 2,
        timelock_ms: 0,
        now_ms: rel_ts + 1,
    };
    assert_eq!(
        verify_release_artifact_only(&store, &oracle, &rel_id, &policy),
        Err(ForgeError::NotEnoughAttestations { needed: 2, got: 1 })
    );
}

#[test]
fn incomplete_artifact_blocks_adoption() {
    let (author_root, author_dev) = human(10);
    let (store, rel_id, digest, rel_ts) = released(&author_root.did(), &author_dev);

    // Copy everything EXCEPT the artifact's chunk objects to a fresh replica.
    let mut replica = Store::new(MemoryBackend::new());
    for id in store.all_ids().unwrap() {
        let obj = store.get(&id).unwrap();
        if obj.object_type != mini_objects::ObjectType::Custom("mini/chunk".to_string()) {
            replica.insert(&obj).unwrap();
        }
    }
    let (r, d) = human(50);
    attest(&mut replica, &r.did(), &d, &rel_id, digest, 2_000, 1).unwrap();

    let oracle = oracle_of(&[&author_root, &author_dev, &r, &d]);
    let policy = ReleasePolicy {
        min_attestations: 1,
        timelock_ms: 0,
        now_ms: rel_ts + 1,
    };
    assert_eq!(
        verify_release_artifact_only(&replica, &oracle, &rel_id, &policy),
        Err(ForgeError::ArtifactUnavailable)
    );
}

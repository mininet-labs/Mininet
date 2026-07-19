//! Batch 3 (D-0066): the release transparency log (`list_releases`) and
//! equivocation detection built on top of it.

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_forge::{
    commit, detect_equivocation, detect_equivocation_strict, list_releases, list_releases_strict,
    project, put_file, put_tree, release, Policy, TreeEntry,
};
use mini_media::publish_media;
use mini_objects::{ObjectBuilder, ObjectType, Payload};
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

/// Publish one release under `proj`/`branch` with the given version and
/// artifact bytes (whose digest becomes the release's artifact digest).
#[allow(clippy::too_many_arguments)]
fn publish_release(
    store: &mut Store<MemoryBackend>,
    author: &Did,
    dev: &Controller,
    proj: &mini_objects::ObjectId,
    branch: &str,
    version: &str,
    artifact_bytes: &[u8],
    seed: u8,
    timestamp_ms: u64,
    sequence: u64,
) -> mini_objects::ObjectId {
    let digest = HashAlgorithm::Blake3.digest(artifact_bytes);
    let manifest = publish_media(
        store,
        author,
        dev,
        "application/octet-stream",
        artifact_bytes,
        50,
        sequence,
    )
    .unwrap();
    let src = put_file(store, author, dev, &[seed]).unwrap();
    let tree = put_tree(
        store,
        author,
        dev,
        &[TreeEntry {
            name: "main.rs".into(),
            is_dir: false,
            target: src,
        }],
    )
    .unwrap();
    let c = commit(
        store,
        author,
        dev,
        "release commit",
        &tree,
        &[],
        timestamp_ms,
        sequence,
    )
    .unwrap();
    let rel = release(
        store,
        author,
        dev,
        version,
        proj,
        branch,
        c.id(),
        &manifest.id,
        digest,
        [seed; 32],
        timestamp_ms,
        sequence,
    )
    .unwrap();
    rel.id().clone()
}

#[test]
fn list_releases_finds_every_release_for_a_project_and_branch() {
    let (root, dev) = human(1);
    let author = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let proj = project(
        &mut store,
        &author,
        &dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();

    let r1 = publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"v1",
        10,
        1_000,
        1,
    );
    let r2 = publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.1.0",
        b"v2",
        20,
        2_000,
        2,
    );
    // A release on a different branch must not show up in "main"'s log.
    let _other_branch = publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "dev",
        "9.9.9",
        b"dev-build",
        30,
        3_000,
        3,
    );

    let releases = list_releases(&store, proj.id(), "main").unwrap();
    let ids: Vec<_> = releases.iter().map(|r| r.id().clone()).collect();
    assert_eq!(ids, vec![r1, r2]); // ordered by timestamp
}

#[test]
fn list_releases_on_an_unknown_branch_is_empty_not_an_error() {
    let (root, dev) = human(2);
    let author = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let proj = project(
        &mut store,
        &author,
        &dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"v1",
        10,
        1_000,
        1,
    );

    assert!(list_releases(&store, proj.id(), "release/1.x")
        .unwrap()
        .is_empty());
}

#[test]
fn strict_release_log_rejects_malformed_matching_entries() {
    let (root, dev) = human(22);
    let author = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let proj = project(
        &mut store,
        &author,
        &dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();

    // A validly signed object can still carry a malformed release payload.
    // The compatibility query skips it, but a transparency-log consumer must
    // be able to fail closed instead of silently constructing a partial log.
    let malformed = ObjectBuilder::new(ObjectType::RELEASE)
        .timestamp_ms(1_000)
        .sequence(1)
        .payload(Payload::Public(vec![0, 0, 0, 1, b'x']))
        .link("project", proj.id().clone())
        .sign(&author, &dev)
        .unwrap();
    store.insert(&malformed).unwrap();

    assert!(list_releases(&store, proj.id(), "main").unwrap().is_empty());
    assert!(matches!(
        list_releases_strict(&store, proj.id(), "main"),
        Err(mini_forge::ForgeError::BadObject)
    ));
    assert!(matches!(
        detect_equivocation_strict(&store, proj.id(), "main"),
        Err(mini_forge::ForgeError::BadObject)
    ));
}

#[test]
fn equivocating_releases_under_the_same_version_are_detected() {
    let (root, dev) = human(3);
    let author = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let proj = project(
        &mut store,
        &author,
        &dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();

    // Two releases both claiming "1.0.0" but with different artifact bytes
    // (hence different digests) -- the publisher (or a compromised signing
    // key) showed two different builds under the same version label.
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"build-a",
        10,
        1_000,
        1,
    );
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"build-b",
        20,
        1_000,
        2,
    );

    let found = detect_equivocation(&store, proj.id(), "main").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].version, "1.0.0");
    assert_ne!(
        found[0].first_artifact_digest,
        found[0].second_artifact_digest
    );
}

#[test]
fn non_equivocating_releases_are_never_flagged() {
    let (root, dev) = human(4);
    let author = root.did();
    let mut store = Store::new(MemoryBackend::new());
    let proj = project(
        &mut store,
        &author,
        &dev,
        "app",
        &Policy {
            min_approvals: 1,
            maintainers: vec![author.clone()],
        },
    )
    .unwrap();

    // Distinct versions, and a same-digest "re-publish" of 1.0.0 (e.g. a
    // second, honest publish of the identical artifact) -- neither is
    // equivocation.
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"build-a",
        10,
        1_000,
        1,
    );
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.1.0",
        b"build-c",
        30,
        2_000,
        2,
    );
    publish_release(
        &mut store,
        &author,
        &dev,
        proj.id(),
        "main",
        "1.0.0",
        b"build-a",
        10,
        3_000,
        3,
    );

    let found = detect_equivocation(&store, proj.id(), "main").unwrap();
    assert!(
        found.is_empty(),
        "expected no equivocation, found {found:?}"
    );
}

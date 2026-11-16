//! Adversarial tests for the exact-release retrieval selector (issue #268,
//! D-0408): the closure a serving peer may offer for one release must be the
//! release's own evidence, deterministic, bounded, and must not be expandable
//! by unrelated objects or by a seed that is not a release.

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::HashAlgorithm;
use mini_forge::{
    commit, project, put_file, put_tree, release, release_retrieval_ids, Policy, TreeEntry,
};
use mini_media::publish_media;
use mini_objects::{ObjectBuilder, ObjectId, ObjectType, Payload};
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

#[allow(clippy::too_many_arguments)]
fn publish(
    store: &mut Store<MemoryBackend>,
    author: &Did,
    dev: &Controller,
    proj: &ObjectId,
    version: &str,
    bytes: &[u8],
    seed: u8,
    seq: u64,
) -> ObjectId {
    let digest = HashAlgorithm::Blake3.digest(bytes);
    let manifest = publish_media(
        store,
        author,
        dev,
        "application/octet-stream",
        bytes,
        50,
        seq,
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
        1_000 + seq,
        seq,
    )
    .unwrap();
    release(
        store,
        author,
        dev,
        version,
        proj,
        "main",
        c.id(),
        &manifest.id,
        digest,
        [seed; 32],
        1_000 + seq,
        seq,
    )
    .unwrap()
    .id()
    .clone()
}

struct World {
    store: Store<MemoryBackend>,
    author: Did,
    dev: Controller,
    proj: ObjectId,
}

fn world() -> World {
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
    .unwrap()
    .id()
    .clone();
    World {
        store,
        author,
        dev,
        proj,
    }
}

#[test]
fn the_closure_is_the_releases_own_evidence_sorted_and_repeatable() {
    let mut w = world();
    let rel = publish(
        &mut w.store,
        &w.author,
        &w.dev,
        &w.proj,
        "1.0.0",
        b"v1",
        10,
        1,
    );
    let ids = release_retrieval_ids(&w.store, &rel).unwrap();
    assert!(ids.contains(&rel), "the release itself is selected");
    let names: Vec<&str> = ids.iter().map(ObjectId::as_str).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted, "sorted and without duplicates");
    assert_eq!(ids, release_retrieval_ids(&w.store, &rel).unwrap());
}

#[test]
fn a_second_release_and_unrelated_objects_are_not_part_of_the_closure() {
    let mut w = world();
    let first = publish(
        &mut w.store,
        &w.author,
        &w.dev,
        &w.proj,
        "1.0.0",
        b"v1",
        10,
        1,
    );
    let second = publish(
        &mut w.store,
        &w.author,
        &w.dev,
        &w.proj,
        "1.1.0",
        b"v2",
        20,
        2,
    );
    // An unrelated social object that merely links at the release under a
    // relation the release verifier never reads.
    let noise = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(5_000)
        .sequence(50)
        .payload(Payload::Public(b"chatter".to_vec()))
        .link("mentions", first.clone())
        .sign(&w.author, &w.dev)
        .unwrap();
    w.store.insert(&noise).unwrap();

    let closure = release_retrieval_ids(&w.store, &first).unwrap();
    assert!(!closure.contains(&second), "another release is not offered");
    assert!(
        !closure.contains(noise.id()),
        "an object mentioning the release under another relation cannot expand the closure"
    );
}

#[test]
fn only_a_release_can_seed_a_retrieval_and_a_missing_one_fails_closed() {
    let mut w = world();
    // A project object is not a release.
    assert!(release_retrieval_ids(&w.store, &w.proj).is_err());
    // An id the store does not hold fails rather than yielding an empty plan.
    let missing = {
        let other = publish(
            &mut w.store,
            &w.author,
            &w.dev,
            &w.proj,
            "2.0.0",
            b"v3",
            30,
            3,
        );
        let mut fresh = Store::new(MemoryBackend::new());
        let _ = &mut fresh;
        (other, fresh)
    };
    assert!(release_retrieval_ids(&missing.1, &missing.0).is_err());
}

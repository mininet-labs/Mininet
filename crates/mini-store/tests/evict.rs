//! Eviction of re-fetchable cache: only default-tier, non-private objects that
//! no retained object links to are removable, every index row goes with them,
//! and the filesystem time index is rebuilt coherent.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, CacheTier, FsBackend, MemoryBackend, Store, StoreError};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn post(human: &Did, device: &Controller, seq: u64, ts: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .sequence(seq)
        .timestamp_ms(ts)
        .payload(Payload::Public(format!("post {seq}").into_bytes()))
        .sign(human, device)
        .unwrap()
}

fn reply(human: &Did, device: &Controller, seq: u64, ts: u64, parent: &ObjectId) -> Object {
    ObjectBuilder::new(ObjectType::COMMENT)
        .sequence(seq)
        .timestamp_ms(ts)
        .link("parent", parent.clone())
        .payload(Payload::Public(b"reply".to_vec()))
        .sign(human, device)
        .unwrap()
}

fn exercise<B: Backend>(mut store: Store<B>) {
    let (root, device) = human(10);
    let a = post(&root.did(), &device, 1, 100);
    let b = post(&root.did(), &device, 2, 200);
    let c = reply(&root.did(), &device, 3, 300, a.id());
    for o in [&a, &b, &c] {
        store.insert(o).unwrap();
    }

    // `a` has a retained reply outside the set: refused, nothing removed.
    assert_eq!(
        store.evict_cached(&[a.id().clone()]),
        Err(StoreError::NotEvictable)
    );
    assert!(store.contains(a.id()).unwrap());

    // Evicting the reply and its parent together is allowed.
    let report = store
        .evict_cached(&[a.id().clone(), c.id().clone()])
        .unwrap();
    assert_eq!(report.objects, 2);
    assert!(report.bytes > 0);
    assert!(!store.contains(a.id()).unwrap() && !store.contains(c.id()).unwrap());
    assert!(store.contains(b.id()).unwrap());

    // Every index row is gone; enumeration and by-author never name them.
    let all = store.all_ids().unwrap();
    assert_eq!(all, vec![b.id().clone()]);
    assert_eq!(store.by_author(&root.did()).unwrap(), vec![b.id().clone()]);
    assert!(store.linking_to(a.id()).unwrap().is_empty());
    assert_eq!(store.recent(10).unwrap(), vec![b.id().clone()]);
    assert_eq!(store.since(0).unwrap(), vec![b.id().clone()]);
    for id in &all {
        store.get(id).unwrap();
    }

    // Pinned content is never evictable; private/encrypted either.
    store
        .set_cache_tier(b.id(), CacheTier::PinnedByOwner)
        .unwrap();
    assert_eq!(
        store.evict_cached(&[b.id().clone()]),
        Err(StoreError::NotEvictable)
    );
    let secret = ObjectBuilder::new(ObjectType::POST)
        .sequence(9)
        .payload(Payload::Encrypted(b"ciphertext".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    store.insert(&secret).unwrap();
    assert_eq!(
        store.evict_cached(&[secret.id().clone()]),
        Err(StoreError::NotEvictable)
    );

    // Re-fetching an evicted object simply stores it again.
    store.insert(&a).unwrap();
    assert!(store.contains(a.id()).unwrap());
    assert_eq!(store.first_seen_ms(a.id()).unwrap(), None);
    assert_eq!(store.note_first_seen(a.id(), 5).unwrap(), 5);
    assert_eq!(
        store.note_first_seen(a.id(), 9).unwrap(),
        5,
        "first sighting wins"
    );
}

#[test]
fn memory_backend_eviction_keeps_every_invariant() {
    exercise(Store::new(MemoryBackend::new()));
}

#[test]
fn filesystem_backend_eviction_keeps_every_invariant_and_the_time_index() {
    let root = std::env::temp_dir().join(format!("mini-store-evict-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    exercise(Store::new(FsBackend::open(&root).unwrap()));
    // A fresh open (index rebuilt from disk) agrees.
    let reopened = Store::new(FsBackend::open(&root).unwrap());
    assert!(reopened.all_ids().unwrap().len() >= 2);
    let _ = std::fs::remove_dir_all(&root);
}

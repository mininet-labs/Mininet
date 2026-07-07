//! Integration tests: persistence, deterministic indexes, head convergence,
//! want-lists, and the filesystem backend surviving a reopen.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{FsBackend, HeadState, MemoryBackend, Store, StoreError};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary()).unwrap();
    (root, device)
}

fn post(human: &Did, device: &Controller, text: &[u8], seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(seq)
        .payload(Payload::Public(text.to_vec()))
        .sign(human, device)
        .unwrap()
}

fn head(human: &Did, device: &Controller, subject: &str, target: &ObjectId, seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::HEAD)
        .sequence(seq)
        .link("target", target.clone())
        .payload(Payload::Public(subject.as_bytes().to_vec()))
        .sign(human, device)
        .unwrap()
}

#[test]
fn insert_get_and_indexes() {
    let (root, device) = human(10);
    let (root2, device2) = human(50);
    let mut store = Store::new(MemoryBackend::new());

    let p1 = post(&root.did(), &device, b"one", 1);
    let p2 = post(&root.did(), &device, b"two", 2);
    let other = post(&root2.did(), &device2, b"theirs", 1);
    let reply = ObjectBuilder::new(ObjectType::COMMENT)
        .link("re", p1.id().clone())
        .payload(Payload::Public(b"reply".to_vec()))
        .sign(&root2.did(), &device2)
        .unwrap();

    for o in [&p1, &p2, &other, &reply] {
        store.insert(o).unwrap();
    }

    assert_eq!(store.get(p1.id()).unwrap(), p1);
    assert!(matches!(
        store.get(&ObjectId::parse(head(&root.did(), &device, "x", p1.id(), 1).id().as_str()).unwrap()),
        Err(StoreError::NotFound)
    ));

    assert_eq!(store.by_author(&root.did()).unwrap().len(), 2);
    assert_eq!(store.by_author(&root2.did()).unwrap().len(), 2);
    assert_eq!(store.by_type(&ObjectType::POST).unwrap().len(), 3);
    assert_eq!(store.linking_to(p1.id()).unwrap(), vec![reply.id().clone()]);
    assert_eq!(store.all_ids().unwrap().len(), 4);
}

#[test]
fn heads_converge_regardless_of_arrival_order() {
    let (root, device) = human(10);
    let v1 = post(&root.did(), &device, b"profile v1", 1);
    let v2 = post(&root.did(), &device, b"profile v2", 2);
    let h1 = head(&root.did(), &device, "profile", v1.id(), 1);
    let h2 = head(&root.did(), &device, "profile", v2.id(), 2);

    // Replica A sees h1 then h2; replica B sees h2 then h1.
    let mut a = Store::new(MemoryBackend::new());
    let mut b = Store::new(MemoryBackend::new());
    for s in [&mut a, &mut b] {
        s.insert(&v1).unwrap();
        s.insert(&v2).unwrap();
    }
    assert_eq!(a.apply_head(&h1).unwrap(), HeadState::Applied);
    assert_eq!(a.apply_head(&h2).unwrap(), HeadState::Applied);
    assert_eq!(b.apply_head(&h2).unwrap(), HeadState::Applied);
    assert_eq!(b.apply_head(&h1).unwrap(), HeadState::Stale);

    // Both resolve to v2.
    assert_eq!(a.resolve_head(&root.did(), "profile").unwrap(), Some(v2.id().clone()));
    assert_eq!(b.resolve_head(&root.did(), "profile").unwrap(), Some(v2.id().clone()));
}

#[test]
fn head_slots_are_per_author_and_shape_checked() {
    let (root, device) = human(10);
    let (root2, device2) = human(50);
    let mut store = Store::new(MemoryBackend::new());
    let target = post(&root.did(), &device, b"t", 1);
    store.insert(&target).unwrap();

    // Another human's head lands in THEIR slot, never in root's.
    let theirs = head(&root2.did(), &device2, "profile", target.id(), 9);
    store.apply_head(&theirs).unwrap();
    assert_eq!(store.resolve_head(&root.did(), "profile").unwrap(), None);
    assert!(store.resolve_head(&root2.did(), "profile").unwrap().is_some());

    // Shape violations are rejected.
    let not_head = post(&root.did(), &device, b"x", 2);
    assert_eq!(store.apply_head(&not_head), Err(StoreError::BadHead));
    let bad_subject = head(&root.did(), &device, "../escape", target.id(), 1);
    assert_eq!(store.apply_head(&bad_subject), Err(StoreError::BadHead));
    let no_target = ObjectBuilder::new(ObjectType::HEAD)
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    assert_eq!(store.apply_head(&no_target), Err(StoreError::BadHead));
}

#[test]
fn missing_links_and_want_list_drive_sync() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let target = post(&root.did(), &device, b"not synced yet", 1);
    let reply = ObjectBuilder::new(ObjectType::COMMENT)
        .link("re", target.id().clone())
        .payload(Payload::Public(b"reply first".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();

    store.insert(&reply).unwrap(); // arrived before its parent
    assert_eq!(store.missing_links(reply.id()).unwrap(), vec![target.id().clone()]);
    assert_eq!(store.want_list().unwrap(), vec![target.id().clone()]);

    store.insert(&target).unwrap();
    assert!(store.missing_links(reply.id()).unwrap().is_empty());
    assert!(store.want_list().unwrap().is_empty());
}

#[test]
fn fs_backend_persists_across_reopen() {
    let dir = std::env::temp_dir().join(format!("mini-store-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let (root, device) = human(10);
    let p = post(&root.did(), &device, b"durable", 1);
    let h = head(&root.did(), &device, "profile", p.id(), 1);

    {
        let mut store = Store::new(FsBackend::open(&dir).unwrap());
        store.insert(&p).unwrap();
        store.apply_head(&h).unwrap();
    }
    {
        let store = Store::new(FsBackend::open(&dir).unwrap());
        assert_eq!(store.get(p.id()).unwrap(), p);
        assert_eq!(store.by_author(&root.did()).unwrap().len(), 2); // post + head
        assert_eq!(
            store.resolve_head(&root.did(), "profile").unwrap(),
            Some(p.id().clone())
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

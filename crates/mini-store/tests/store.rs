//! Integration tests: persistence, deterministic indexes, head convergence,
//! want-lists, and the filesystem backend surviving a reopen.

use did_mini::{Capabilities, Controller, Did};
use mini_crypto::{AeadKey, AeadSuite};
use mini_objects::{
    Object, ObjectBuilder, ObjectEnvelopeV2, ObjectId, ObjectType, OpaqueRoute, Payload,
    PrivateObject, RetentionClass, StorageDescriptor,
};
use mini_store::{Backend, FsBackend, HeadState, MemoryBackend, Store, StoreError};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
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
        store.get(
            &ObjectId::parse(head(&root.did(), &device, "x", p1.id(), 1).id().as_str()).unwrap()
        ),
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
    assert_eq!(
        a.resolve_head(&root.did(), "profile").unwrap(),
        Some(v2.id().clone())
    );
    assert_eq!(
        b.resolve_head(&root.did(), "profile").unwrap(),
        Some(v2.id().clone())
    );
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
    assert!(store
        .resolve_head(&root2.did(), "profile")
        .unwrap()
        .is_some());

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
    assert_eq!(
        store.missing_links(reply.id()).unwrap(),
        vec![target.id().clone()]
    );
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

#[test]
fn private_envelopes_round_trip_and_index_only_by_opaque_route() {
    let (root, device) = human(10);
    let key = AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[42; 32]).unwrap();
    let route = OpaqueRoute::from_bytes([7; 32]);
    let private = PrivateObject::new(
        ObjectType::Custom("mininet/message".to_string()),
        root.did(),
        device.did(),
        1_000,
        1,
        vec![],
        b"private metadata".to_vec(),
        b"a private hello".to_vec(),
    )
    .sign_with(&device);
    let envelope = ObjectEnvelopeV2::seal(
        &private,
        &key,
        route,
        StorageDescriptor {
            retention: RetentionClass::Standard,
        },
    )
    .unwrap();
    let mut store = Store::new(MemoryBackend::new());

    store.insert_private(&envelope).unwrap();

    assert!(store.contains_private(envelope.id()).unwrap());
    assert_eq!(store.get_private(envelope.id()).unwrap(), envelope);
    assert_eq!(
        store.private_by_route(&route).unwrap(),
        vec![envelope.id().clone()]
    );
    assert!(store.all_ids().unwrap().is_empty());
    assert!(store.by_author(&root.did()).unwrap().is_empty());
    assert!(store
        .by_type(&ObjectType::Custom("mininet/message".to_string()))
        .unwrap()
        .is_empty());
}

#[test]
fn private_envelopes_survive_filesystem_reopen() {
    let dir = std::env::temp_dir().join(format!(
        "mini-store-private-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (root, device) = human(20);
    let key = AeadKey::from_suite_bytes(AeadSuite::DEFAULT, &[11; 32]).unwrap();
    let route = OpaqueRoute::from_bytes([9; 32]);
    let private = PrivateObject::new(
        ObjectType::Custom("mininet/message".to_string()),
        root.did(),
        device.did(),
        2_000,
        1,
        vec![],
        vec![],
        b"durable private hello".to_vec(),
    )
    .sign_with(&device);
    let envelope = ObjectEnvelopeV2::seal(
        &private,
        &key,
        route,
        StorageDescriptor {
            retention: RetentionClass::Standard,
        },
    )
    .unwrap();

    {
        let mut store = Store::new(FsBackend::open(&dir).unwrap());
        store.insert_private(&envelope).unwrap();
    }
    {
        let store = Store::new(FsBackend::open(&dir).unwrap());
        assert_eq!(store.private_by_route(&route).unwrap().len(), 1);
        assert_eq!(store.get_private(envelope.id()).unwrap(), envelope);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_backend_prefix_queries_support_exact_and_partial_segments() {
    let dir = std::env::temp_dir().join(format!(
        "mini-store-prefix-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut backend = FsBackend::open(&dir).unwrap();
    backend.put_meta("idx/type/w8/aaa", b"a").unwrap();
    backend.put_meta("idx/type/w9/bbb", b"b").unwrap();
    backend.put_meta("head/alice/profile", b"head").unwrap();

    assert_eq!(
        backend.list_meta_prefix("idx/type/w8/").unwrap(),
        vec![("idx/type/w8/aaa".to_string(), b"a".to_vec())]
    );
    assert_eq!(
        backend.list_meta_prefix("idx/type/w").unwrap(),
        vec![
            ("idx/type/w8/aaa".to_string(), b"a".to_vec()),
            ("idx/type/w9/bbb".to_string(), b"b".to_vec())
        ]
    );
    assert!(backend
        .list_meta_prefix("missing/path/")
        .unwrap()
        .is_empty());
    assert!(backend.list_meta_prefix("idx/../head").is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fs_backend_direct_reads_reject_non_file_entries() {
    let dir = std::env::temp_dir().join(format!(
        "mini-store-direct-read-poison-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let backend = FsBackend::open(&dir).unwrap();

    std::fs::create_dir_all(dir.join("meta/head/alice/profile")).unwrap();
    assert!(matches!(
        backend.get_meta("head/alice/profile"),
        Err(StoreError::Io(message)) if message.contains("regular file")
    ));

    std::fs::create_dir_all(dir.join("blobs/ab/abc")).unwrap();
    assert!(matches!(
        backend.get_blob("abc"),
        Err(StoreError::Io(message)) if message.contains("regular file")
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn fs_backend_refuses_symlinks_in_a_metadata_query_subtree() {
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join(format!(
        "mini-store-symlink-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let outside = dir.with_extension("outside");
    let backend = FsBackend::open(&dir).unwrap();
    std::fs::create_dir_all(dir.join("meta/idx")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("injected"), b"not an index row").unwrap();
    symlink(&outside, dir.join("meta/idx/id")).unwrap();

    assert!(matches!(
        backend.list_meta_prefix("idx/id/"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));

    std::fs::remove_file(dir.join("meta/idx/id")).unwrap();
    std::fs::remove_dir(dir.join("meta/idx")).unwrap();
    std::fs::create_dir_all(outside.join("id")).unwrap();
    symlink(&outside, dir.join("meta/idx")).unwrap();
    assert!(matches!(
        backend.list_meta_prefix("idx/id/"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));

    std::fs::remove_file(dir.join("meta/idx")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[cfg(unix)]
#[test]
fn fs_backend_direct_reads_reject_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = std::env::temp_dir().join(format!(
        "mini-store-direct-read-symlink-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let outside = dir.with_extension("outside");
    let backend = FsBackend::open(&dir).unwrap();
    std::fs::create_dir_all(outside.join("meta")).unwrap();
    std::fs::write(outside.join("meta/profile"), b"outside").unwrap();

    std::fs::create_dir_all(dir.join("meta/head/alice")).unwrap();
    symlink(
        outside.join("meta/profile"),
        dir.join("meta/head/alice/profile"),
    )
    .unwrap();
    assert!(matches!(
        backend.get_meta("head/alice/profile"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));

    std::fs::create_dir_all(dir.join("blobs/ab")).unwrap();
    symlink(outside.join("meta/profile"), dir.join("blobs/ab/abc")).unwrap();
    assert!(matches!(
        backend.get_blob("abc"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));

    std::fs::remove_file(dir.join("meta/head/alice/profile")).unwrap();
    std::fs::remove_dir_all(dir.join("meta/head")).unwrap();
    std::fs::create_dir_all(outside.join("meta/head/alice")).unwrap();
    std::fs::write(outside.join("meta/head/alice/profile"), b"outside").unwrap();
    symlink(outside.join("meta/head"), dir.join("meta/head")).unwrap();
    assert!(matches!(
        backend.get_meta("head/alice/profile"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));

    std::fs::remove_file(dir.join("blobs/ab/abc")).unwrap();
    std::fs::remove_dir(dir.join("blobs/ab")).unwrap();
    std::fs::create_dir_all(outside.join("blobs/ab")).unwrap();
    std::fs::write(outside.join("blobs/ab/abc"), b"outside").unwrap();
    symlink(outside.join("blobs/ab"), dir.join("blobs/ab")).unwrap();
    assert!(matches!(
        backend.get_blob("abc"),
        Err(StoreError::Io(message)) if message.contains("symlink")
    ));
    assert!(backend.has_blob("abc").is_err());

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[cfg(unix)]
#[test]
fn fs_backend_narrow_query_ignores_unrelated_symlinks_and_rejects_special_files() {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let dir = std::env::temp_dir().join(format!(
        "mini-store-special-file-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let outside = dir.with_extension("outside");
    let mut backend = FsBackend::open(&dir).unwrap();
    backend.put_meta("idx/type/w8/aaa", b"a").unwrap();
    std::fs::create_dir_all(&outside).unwrap();

    // A narrow lookup must not inspect an unrelated metadata subtree.
    symlink(&outside, dir.join("meta/head")).unwrap();
    assert_eq!(
        backend.list_meta_prefix("idx/type/w8/").unwrap(),
        vec![("idx/type/w8/aaa".to_string(), b"a".to_vec())]
    );

    // Special filesystem nodes inside the selected subtree are not index rows.
    let socket_path = dir.join("meta/idx/type/w8/not-an-index-row");
    let listener = UnixListener::bind(&socket_path).unwrap();
    assert!(matches!(
        backend.list_meta_prefix("idx/type/w8/"),
        Err(StoreError::Io(message)) if message.contains("non-file")
    ));

    drop(listener);
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(dir.join("meta/head"));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

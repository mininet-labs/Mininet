//! Integration tests: two stores reconciling over the encrypted channel, with
//! verified ingest, KEL carriers, interruption + resume, and strict rejection
//! of unknown authors.

use std::thread;

use did_mini::{Capabilities, Controller, Did};
use mini_bearer::{pair, Bearer, Channel, InProcessBearer, Initiator, Responder};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};
use mini_sync::{kel_carrier, sync_bidirectional, KelCache, SyncRole};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn post(h: &Did, d: &Controller, text: &[u8], seq: u64) -> Object {
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(seq)
        .payload(Payload::Public(text.to_vec()))
        .sign(h, d)
        .unwrap()
}

/// Seed a store with a human's carriers + `n` posts; return their cache too.
fn seeded(seed: u8, n: u64) -> (Store<MemoryBackend>, KelCache, Controller, Controller) {
    let (root, device) = human(seed);
    let mut store = Store::new(MemoryBackend::new());
    let mut cache = KelCache::new();
    let rc = kel_carrier(&root.kel(), &root.did(), &device).unwrap();
    let dc = kel_carrier(&device.kel(), &root.did(), &device).unwrap();
    store.insert(&rc).unwrap();
    store.insert(&dc).unwrap();
    cache.insert_verified(root.kel());
    cache.insert_verified(device.kel());
    for i in 0..n {
        store
            .insert(&post(
                &root.did(),
                &device,
                format!("post {i}").as_bytes(),
                i,
            ))
            .unwrap();
    }
    (store, cache, root, device)
}

/// Handshake two connected bearers into channels.
fn channels(a: &mut InProcessBearer, b: &mut InProcessBearer) -> (Channel, Channel) {
    let (init, hello1) = Initiator::start().unwrap();
    a.send(&hello1).unwrap();
    let got1 = b.recv().unwrap();
    let (chan_b, hello2) = Responder::respond(&got1).unwrap();
    b.send(&hello2).unwrap();
    let got2 = a.recv().unwrap();
    (init.finish(&got2).unwrap(), chan_b)
}

/// Run a full bidirectional sync between two (store, cache) peers.
fn run_sync(
    mut a_store: Store<MemoryBackend>,
    mut a_cache: KelCache,
    mut b_store: Store<MemoryBackend>,
    mut b_cache: KelCache,
) -> (
    Store<MemoryBackend>,
    KelCache,
    mini_sync::IngestReport,
    Store<MemoryBackend>,
    KelCache,
    mini_sync::IngestReport,
) {
    let (mut ba, mut bb) = pair();
    let (mut ca, mut cb) = channels(&mut ba, &mut bb);
    let handle = thread::spawn(move || {
        let r = sync_bidirectional(
            &mut bb,
            &mut cb,
            &mut b_store,
            &mut b_cache,
            SyncRole::Responder,
        )
        .unwrap();
        (b_store, b_cache, r)
    });
    let ra = sync_bidirectional(
        &mut ba,
        &mut ca,
        &mut a_store,
        &mut a_cache,
        SyncRole::Initiator,
    )
    .unwrap();
    let (b_store, b_cache, rb) = handle.join().unwrap();
    (a_store, a_cache, ra, b_store, b_cache, rb)
}

#[test]
fn fresh_peer_pulls_everything_carriers_first() {
    // A has an identity + 200 posts; B is empty.
    let (a_store, a_cache, ..) = seeded(10, 200);
    let b_store = Store::new(MemoryBackend::new());
    let b_cache = KelCache::new();

    let (a_store, _, ra, b_store, b_cache, rb) = run_sync(a_store, a_cache, b_store, b_cache);

    // B ingested: 2 carriers + 200 verified posts, nothing rejected.
    assert_eq!(rb.carriers, 2);
    assert_eq!(rb.accepted, 200);
    assert_eq!(rb.unknown_author, 0);
    assert_eq!(rb.invalid, 0);
    assert_eq!(b_cache.len(), 2);
    // A pulled nothing (B was empty).
    assert_eq!(ra.received, 0);
    // Stores now identical.
    assert_eq!(a_store.all_ids().unwrap(), b_store.all_ids().unwrap());
}

#[test]
fn bidirectional_sync_produces_the_union() {
    let (a_store, a_cache, ..) = seeded(10, 25);
    let (b_store, b_cache, ..) = seeded(50, 40);
    let a_count = a_store.all_ids().unwrap().len();
    let b_count = b_store.all_ids().unwrap().len();

    let (a_store, a_cache, ra, b_store, b_cache, rb) = run_sync(a_store, a_cache, b_store, b_cache);

    assert_eq!(a_store.all_ids().unwrap(), b_store.all_ids().unwrap());
    assert_eq!(a_store.all_ids().unwrap().len(), a_count + b_count);
    // Each side accepted the other's content and identity.
    assert_eq!(ra.accepted, 40);
    assert_eq!(rb.accepted, 25);
    assert_eq!(ra.carriers, 2);
    assert_eq!(rb.carriers, 2);
    assert_eq!(a_cache.len(), 4);
    assert_eq!(b_cache.len(), 4);
}

#[test]
fn identical_stores_finish_immediately() {
    let (a_store, a_cache, ..) = seeded(10, 5);
    // Clone-equivalent: rebuild the same deterministic content.
    let (b_store, b_cache, ..) = seeded(10, 5);
    assert_eq!(a_store.all_ids().unwrap(), b_store.all_ids().unwrap());

    let (_, _, ra, _, _, rb) = run_sync(a_store, a_cache, b_store, b_cache);
    assert_eq!(ra.received, 0);
    assert_eq!(rb.received, 0);
}

#[test]
fn interrupted_sync_resumes_by_idempotence() {
    let (mut a_store, mut a_cache, ..) = seeded(10, 60);
    let (mut b_store, mut b_cache, ..) = seeded(50, 3);

    // First encounter dies mid-protocol: drop the responder after the
    // handshake without running its side.
    {
        let (mut ba, bb) = pair();
        let (init, hello1) = Initiator::start().unwrap();
        ba.send(&hello1).unwrap();
        drop(bb); // peer walks away
        let _ = init; // channel never established; initiator would error on recv
        assert!(ba.recv().is_err());
    }

    // Next encounter: a fresh channel, same stores — full convergence.
    let (a2, _, _, b2, _, rb) = run_sync(
        std::mem::replace(&mut a_store, Store::new(MemoryBackend::new())),
        std::mem::take(&mut a_cache),
        std::mem::replace(&mut b_store, Store::new(MemoryBackend::new())),
        std::mem::take(&mut b_cache),
    );
    assert_eq!(a2.all_ids().unwrap(), b2.all_ids().unwrap());
    assert_eq!(rb.accepted, 60);
}

#[test]
fn content_from_unknown_authors_is_rejected() {
    // A holds a post by human C but NOT C's carriers.
    let (mut a_store, a_cache, ..) = seeded(10, 2);
    let (c_root, c_dev) = human(90);
    let stray = post(&c_root.did(), &c_dev, b"who am i", 1);
    a_store.insert(&stray).unwrap();

    let b_store = Store::new(MemoryBackend::new());
    let b_cache = KelCache::new();
    let (_, _, _, b_store, _, rb) = run_sync(a_store, a_cache, b_store, b_cache);

    // B accepted A's identified content but refused the stray.
    assert_eq!(rb.unknown_author, 1);
    assert_eq!(rb.accepted, 2);
    assert!(!b_store.contains(stray.id()).unwrap());
}

#[test]
fn heads_sync_and_resolve_on_the_receiver() {
    let (mut a_store, a_cache, root, device) = seeded(10, 1);
    let v2 = post(&root.did(), &device, b"profile v2", 7);
    a_store.insert(&v2).unwrap();
    let head = ObjectBuilder::new(ObjectType::HEAD)
        .sequence(1)
        .link("target", v2.id().clone())
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    a_store.apply_head(&head).unwrap();

    let (_, _, _, b_store, _, _) = run_sync(
        a_store,
        a_cache,
        Store::new(MemoryBackend::new()),
        KelCache::new(),
    );
    assert_eq!(
        b_store.resolve_head(&root.did(), "profile").unwrap(),
        Some(v2.id().clone())
    );
}

#[test]
fn orphan_root_carrier_is_absorbed_but_not_indexed() {
    // Regression (review issue #3, root-carrier envelope provenance): a root
    // carrier is self-certifying, but its envelope is signed by some *device* of
    // that root. Absorbing the KEL is safe; indexing the object as authored
    // content before the signing device is known would pollute authorship views.
    use mini_sync::{Ingest, IngestOutcome};

    let (root, device) = human(200);
    let rc = kel_carrier(&root.kel(), &root.did(), &device).unwrap();
    let dc = kel_carrier(&device.kel(), &root.did(), &device).unwrap();

    let mut cache = KelCache::new();

    // Root carrier alone: the KEL is absorbed (identity becomes usable), but the
    // envelope's signing device is unknown, so it is KEL-only — NOT indexable.
    assert_eq!(
        Ingest::check(&mut cache, &rc),
        IngestOutcome::AcceptedKelOnly
    );
    assert!(cache.get(&root.did()).is_some());

    // Once the signing device's own carrier is absorbed, the SAME root carrier
    // becomes envelope-provable and may finally be indexed — this is what the
    // sync layer's second pass relies on.
    assert_eq!(
        Ingest::check(&mut cache, &dc),
        IngestOutcome::AcceptedCarrier
    );
    assert_eq!(
        Ingest::check(&mut cache, &rc),
        IngestOutcome::AcceptedCarrier
    );
}

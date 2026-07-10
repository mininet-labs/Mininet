//! Proves `mini_sync::sync_bidirectional`'s own claim — "reconcile two
//! `mini_store` stores over any `mini_bearer::Bearer`" — is actually true
//! of a real socket, not just the in-process `pair()` every other test in
//! this crate uses. Same protocol logic, same `Channel` handshake, real
//! `TcpBearer` on both ends over localhost. Closes the `mini-sync` half of
//! [roadmap #23](../../issues/23).

use std::net::{TcpListener, TcpStream};
use std::thread;

use did_mini::{Capabilities, Controller, Did};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
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

fn post(h: &Did, d: &Controller, text: &[u8], seq: u64) -> mini_objects::Object {
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_000)
        .sequence(seq)
        .payload(Payload::Public(text.to_vec()))
        .sign(h, d)
        .unwrap()
}

fn seeded(seed: u8, n: u64) -> (Store<MemoryBackend>, KelCache) {
    let (root, device) = human(seed);
    let mut store = Store::new(MemoryBackend::new());
    let mut cache = KelCache::new();
    store
        .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
        .unwrap();
    store
        .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
        .unwrap();
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
    (store, cache)
}

fn handshake_responder(bearer: &mut TcpBearer) -> Channel {
    let hello = bearer.recv().unwrap();
    let (chan, response) = Responder::respond(&hello).unwrap();
    bearer.send(&response).unwrap();
    chan
}

fn handshake_initiator(bearer: &mut TcpBearer) -> Channel {
    let (init, hello) = Initiator::start().unwrap();
    bearer.send(&hello).unwrap();
    let response = bearer.recv().unwrap();
    init.finish(&response).unwrap()
}

#[test]
fn a_fresh_peer_pulls_everything_over_a_real_tcp_socket() {
    let (mut a_store, mut a_cache) = seeded(50, 200); // has identity + 200 posts
    let mut b_store: Store<MemoryBackend> = Store::new(MemoryBackend::new()); // empty
    let mut b_cache = KelCache::new();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        // The responder serves the initiator's pull first, then pulls its
        // own (empty, since B has nothing A lacks) -- SyncRole::Responder.
        sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut a_store,
            &mut a_cache,
            SyncRole::Responder,
        )
        .unwrap();
    });

    let stream = TcpStream::connect(addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let mut chan = handshake_initiator(&mut bearer);
    let report = sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut b_store,
        &mut b_cache,
        SyncRole::Initiator,
    )
    .unwrap();
    server.join().unwrap();

    // 2 KEL carriers + 200 posts, all pulled over the real socket.
    assert_eq!(report.carriers, 2);
    assert_eq!(report.accepted, 200);
    assert_eq!(report.unknown_author, 0);
    assert_eq!(report.invalid, 0);

    let synced_ids: std::collections::BTreeSet<String> = b_store
        .all_ids()
        .unwrap()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    assert_eq!(synced_ids.len(), 202);
}

#[test]
fn two_peers_with_disjoint_content_converge_to_the_same_set_over_tcp() {
    let (mut a_store, mut a_cache) = seeded(60, 5);
    let (mut b_store, mut b_cache) = seeded(70, 5);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut b_store,
            &mut b_cache,
            SyncRole::Responder,
        )
        .unwrap();
        b_store
    });

    let stream = TcpStream::connect(addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let mut chan = handshake_initiator(&mut bearer);
    sync_bidirectional(
        &mut bearer,
        &mut chan,
        &mut a_store,
        &mut a_cache,
        SyncRole::Initiator,
    )
    .unwrap();
    let b_store = server.join().unwrap();

    let a_ids: std::collections::BTreeSet<String> = a_store
        .all_ids()
        .unwrap()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    let b_ids: std::collections::BTreeSet<String> = b_store
        .all_ids()
        .unwrap()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    assert_eq!(
        a_ids, b_ids,
        "two peers with disjoint content must converge to the identical set over a real socket"
    );
}

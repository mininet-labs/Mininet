//! Proves `mini_sync::sync_bidirectional`'s own claim — "reconcile two
//! `mini_store` stores over any `mini_bearer::Bearer`" — is actually true
//! of a real socket, not just the in-process `pair()` every other test in
//! this crate uses. Same protocol logic, same `Channel` handshake, real
//! `TcpBearer` on both ends over localhost. Closes the `mini-sync` half of
//! [roadmap #23](../../issues/23).

use std::net::{TcpListener, TcpStream};
use std::thread;

use did_mini::{Capabilities, Controller, Did};
use mini_bearer::{Bearer, BearerError, Channel, Initiator, Responder, TcpBearer};
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

fn handshake_responder(bearer: &mut dyn Bearer) -> Channel {
    let hello = bearer.recv().unwrap();
    let (chan, response) = Responder::respond(&hello).unwrap();
    bearer.send(&response).unwrap();
    chan
}

fn handshake_initiator(bearer: &mut dyn Bearer) -> Channel {
    let (init, hello) = Initiator::start().unwrap();
    bearer.send(&hello).unwrap();
    let response = bearer.recv().unwrap();
    init.finish(&response).unwrap()
}

/// Wraps a real [`TcpBearer`]; answers the first `remaining` calls to
/// `recv` normally, then fails every call after that as
/// [`BearerError::Closed`] -- simulating a real connection dying mid-transfer
/// from this peer's own point of view. `mini_sync`'s protocol internals
/// (`Msg`, `pull`) are private, so this is the way to land a kill partway
/// through a real pull without duplicating protocol logic in the test.
struct KillSwitchBearer {
    inner: TcpBearer,
    remaining: usize,
}

impl Bearer for KillSwitchBearer {
    fn send(&mut self, frame: &[u8]) -> mini_bearer::Result<()> {
        self.inner.send(frame)
    }

    fn recv(&mut self) -> mini_bearer::Result<Vec<u8>> {
        if self.remaining == 0 {
            return Err(BearerError::Closed);
        }
        self.remaining -= 1;
        self.inner.recv()
    }

    fn try_recv(&mut self) -> mini_bearer::Result<Option<Vec<u8>>> {
        self.inner.try_recv()
    }
}

/// Closes the gap the founder review's P1 backlog named ("resumable
/// peer-to-peer bootstrap capsule transfer"): every existing resume test
/// (`interrupted_sync_resumes_by_idempotence` in `sync.rs`) interrupts
/// *before* any content is exchanged. This test kills a *real* TCP
/// connection strictly mid-transfer -- after some, but not all, object
/// batches have crossed the wire -- and proves a second, fresh connection
/// still converges the two stores completely.
///
/// What this proves, precisely: `mini-sync`'s pull is atomic per encounter
/// -- `pull()` only ingests into the store *after* its whole want-round
/// completes, so a connection that dies mid-stream discards that attempt's
/// bytes wholesale rather than partially corrupting or duplicating the
/// receiver's store. "Resume" here means safe, idempotent retry-from-scratch
/// of the remaining diff over a new connection, not byte-offset resume
/// within one transfer -- the crate's own doc comment ("Resume =
/// idempotence") already says this; this test is what makes it true of an
/// actually-severed real socket instead of only an in-process one.
#[test]
fn a_connection_killed_mid_transfer_over_real_tcp_is_safely_resumed_by_a_fresh_connection() {
    let (mut a_store, mut a_cache) = seeded(80, 300); // 2 carriers + 300 posts
    let mut b_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut b_cache = KelCache::new();

    // --- Round 1: a real connection that dies strictly mid-transfer. ---
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    // `serve_pull` batches objects up to 64 at a time (see
    // `mini-sync/src/protocol.rs`), so 300 objects cross the wire as
    // several `Objects` messages. The server just serves normally; it has
    // no idea the client is about to vanish, exactly like a real dropped
    // Wi-Fi link looks from the sending side.
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        // The client will stop reading partway through and never send its
        // closing messages -- this is *expected* to error, the same way a
        // real peer's send eventually fails against a socket nobody is
        // draining anymore.
        let _ = sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut a_store,
            &mut a_cache,
            SyncRole::Responder,
        );
        a_store
    });

    let stream = TcpStream::connect(addr).unwrap();
    let real_bearer = TcpBearer::from_stream(stream).unwrap();
    // Allow: the handshake response, BucketDigests, Ids, and 2 Objects
    // batches (~128 of the 300 objects) through before killing the
    // connection -- strictly mid-transfer, well before the terminating
    // empty Objects marker (5 batches of up to 64 objects each are needed
    // to carry all 300).
    let mut killswitch = KillSwitchBearer {
        inner: real_bearer,
        remaining: 5,
    };
    let mut chan = handshake_initiator(&mut killswitch);
    let round_one = sync_bidirectional(
        &mut killswitch,
        &mut chan,
        &mut b_store,
        &mut b_cache,
        SyncRole::Initiator,
    );
    assert!(
        round_one.is_err(),
        "a connection killed mid-transfer must surface as an error, not a silent partial success"
    );
    // The killed attempt must not have partially or falsely populated the
    // store -- pull() only ingests after its whole want-round completes.
    assert_eq!(b_store.all_ids().unwrap().len(), 0);
    drop(killswitch); // physically close the socket, unblocking the server

    let a_store = server.join().unwrap(); // server errors too; that's expected

    // --- Round 2: a fresh connection, same two (now-persistent) stores. ---
    // A fresh, empty `KelCache` is enough here: A's own second-leg pull
    // (a `Responder` also pulls from its peer after serving) will find B
    // still offers nothing, so `Ingest::check` is never invoked for A's
    // side of round two -- there is no cache state to reconstruct.
    let listener2 = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr2 = listener2.local_addr().unwrap();
    let mut a_store = a_store;
    let mut a_cache = KelCache::new();
    let server2 = thread::spawn(move || {
        let (stream, _) = listener2.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut a_store,
            &mut a_cache,
            SyncRole::Responder,
        )
        .unwrap();
    });

    let stream2 = TcpStream::connect(addr2).unwrap();
    let mut bearer2 = TcpBearer::from_stream(stream2).unwrap();
    let mut chan2 = handshake_initiator(&mut bearer2);
    let round_two = sync_bidirectional(
        &mut bearer2,
        &mut chan2,
        &mut b_store,
        &mut b_cache,
        SyncRole::Initiator,
    )
    .unwrap();
    server2.join().unwrap();

    // Full convergence: nothing lost from the killed attempt, nothing
    // duplicated either (ingest is idempotent by content id, but round one
    // ingested zero objects, so round two must supply the entire set).
    assert_eq!(round_two.carriers, 2);
    assert_eq!(round_two.accepted, 300);
    assert_eq!(b_store.all_ids().unwrap().len(), 302);
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

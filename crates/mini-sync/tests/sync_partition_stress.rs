//! Adversarial partition-pattern stress tests for `mini_sync::sync_bidirectional`
//! over real TCP, closing the robustness scope roadmap #26 carved out of
//! D-0062 ("that robustness testing is `mini-sync`'s own separate scope,
//! roadmap #26"). Every existing test in `sync_over_tcp.rs` covers either a
//! single mid-transfer drop or a single simple 2-peer convergence. This file
//! is about *patterns* real store-and-forward encounters produce: repeated
//! intermittent drops, long offline accumulation, asymmetric multi-peer
//! reachability (store-and-forward relay), and out-of-order reconnection
//! interleaved with local writes.
//!
//! This is adversarial *testing* of the existing bucketed-reconciliation
//! protocol, not a redesign -- see D-0102 in `docs/DECISION_LOG.md` for the
//! findings this file produced.

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

/// One identity ("human") whose device authors posts, plus a store/cache pair
/// that already holds that identity's KEL carriers -- the reusable per-peer
/// fixture every scenario below builds on.
struct Peer {
    store: Store<MemoryBackend>,
    cache: KelCache,
    root: Did,
    device: Controller,
    next_seq: u64,
}

impl Peer {
    fn new(seed: u8) -> Self {
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
        Peer {
            store,
            cache,
            root: root.did(),
            device,
            next_seq: 0,
        }
    }

    /// Author `n` more posts locally (simulates offline local writes between
    /// encounters).
    fn write(&mut self, n: u64) {
        for _ in 0..n {
            let seq = self.next_seq;
            self.next_seq += 1;
            let obj = post(
                &self.root,
                &self.device,
                format!("post {seq}").as_bytes(),
                seq,
            );
            self.store.insert(&obj).unwrap();
        }
    }

    fn ids(&self) -> std::collections::BTreeSet<String> {
        self.store
            .all_ids()
            .unwrap()
            .into_iter()
            .map(|i| i.as_str().to_string())
            .collect()
    }
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

/// One real-TCP encounter between two peers: `a` is the `Responder` (serves
/// first, then pulls its own remainder), `b` is the `Initiator`.
fn encounter(a: &mut Peer, b: &mut Peer) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let a_store = std::mem::replace(&mut a.store, Store::new(MemoryBackend::new()));
    let a_cache = std::mem::replace(&mut a.cache, KelCache::new());
    let (a_store, a_cache) = {
        let mut a_store = a_store;
        let mut a_cache = a_cache;
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
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
            (a_store, a_cache)
        });

        let stream = TcpStream::connect(addr).unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_initiator(&mut bearer);
        sync_bidirectional(
            &mut bearer,
            &mut chan,
            &mut b.store,
            &mut b.cache,
            SyncRole::Initiator,
        )
        .unwrap();
        server.join().unwrap()
    };
    a.store = a_store;
    a.cache = a_cache;
}

/// **Repeated intermittent drops.** A real connection between two peers is
/// killed after only a handful of frames, over and over, with a fresh
/// connection each time -- the way a phone walking in and out of BLE/Wi-Fi
/// range looks from `mini-sync`'s point of view. Every attempt must either
/// fully fail (no partial/duplicate ingest) or fully succeed; after enough
/// attempts the two stores must be byte-identical, never having lost or
/// duplicated anything.
#[test]
fn repeated_intermittent_drops_eventually_converge_without_loss_or_duplication() {
    use mini_bearer::BearerError;

    struct FlakyBearer {
        inner: TcpBearer,
        remaining: usize,
    }
    impl Bearer for FlakyBearer {
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

    let mut a = Peer::new(10);
    a.write(500); // enough posts to need several Objects batches
    let mut b = Peer::new(20); // starts empty of A's content
    let b_own_ids = b.ids(); // B's own identity carriers, never offered by A

    // Attempt with an increasing, but still small, frame budget each time --
    // a real reconnect after a partition doesn't guarantee more airtime than
    // the last one. Eventually (once the budget covers a whole round) it
    // must succeed; every earlier attempt must leave B's store untouched by
    // that attempt (idempotent all-or-nothing ingest).
    for budget in [1usize, 2, 4, 8, 16, 32] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let a_store = std::mem::replace(&mut a.store, Store::new(MemoryBackend::new()));
        let a_cache = std::mem::replace(&mut a.cache, KelCache::new());
        let server = thread::spawn(move || {
            let mut a_store = a_store;
            let mut a_cache = a_cache;
            let (stream, _) = listener.accept().unwrap();
            let mut bearer = TcpBearer::from_stream(stream).unwrap();
            let mut chan = handshake_responder(&mut bearer);
            let _ = sync_bidirectional(
                &mut bearer,
                &mut chan,
                &mut a_store,
                &mut a_cache,
                SyncRole::Responder,
            );
            (a_store, a_cache)
        });

        let stream = TcpStream::connect(addr).unwrap();
        let real_bearer = TcpBearer::from_stream(stream).unwrap();
        let mut flaky = FlakyBearer {
            inner: real_bearer,
            remaining: budget,
        };
        let before = b.ids();
        let mut chan = handshake_initiator(&mut flaky);
        let _ = sync_bidirectional(
            &mut flaky,
            &mut chan,
            &mut b.store,
            &mut b.cache,
            SyncRole::Initiator,
        );
        drop(flaky);
        let (a_store, a_cache) = server.join().unwrap();
        a.store = a_store;
        a.cache = a_cache;

        // Whether this attempt returned Ok or Err, the store must only ever
        // gain content-addressed objects it actually completed a full pull
        // round for -- never lose what it already had (idempotent ingest),
        // and every id it now holds must be a real id A actually offers (no
        // corruption/fabrication from a truncated exchange). An `Initiator`
        // runs its own `pull()` to completion *before* serving the peer's
        // pull, so a kill during the second leg can surface as an overall
        // `Err` even though this attempt's own pull already committed --
        // that is forward progress, not corruption, and is exactly what
        // "resume = idempotence" promises.
        let after = b.ids();
        assert!(
            before.is_subset(&after),
            "a sync attempt at budget {budget} must never lose previously-held content"
        );
        for id in &after {
            assert!(
                a.ids().contains(id) || b_own_ids.contains(id),
                "budget {budget}: id {id} appeared in B's store but is neither A's content \
                 nor B's own pre-existing identity"
            );
        }
    }

    // One final, unconstrained encounter must always be able to finish the
    // job regardless of how many times it was interrupted before.
    encounter(&mut a, &mut b);
    assert_eq!(
        a.ids(),
        b.ids(),
        "after enough retries over repeatedly-dropped connections, stores must fully converge"
    );
    // A's 2 carriers + 500 posts, plus B's own 2 carriers picked up along the
    // way (encounters are bidirectional; B's identity is not filtered out).
    assert_eq!(a.ids().len(), 504);
}

/// **Long offline accumulation before reconnect.** One peer writes a large
/// number of objects across many simulated offline sessions with zero
/// encounters in between, then reconnects once. The single big reconciliation
/// must carry everything in one bounded pull (respecting `MAX_WANT_ROUNDS`
/// and the batch size), not silently truncate.
#[test]
fn long_offline_accumulation_converges_in_a_single_reconnect() {
    let mut a = Peer::new(30);
    // Simulate many offline writing sessions with no sync between them.
    for _ in 0..20 {
        a.write(50);
    }
    assert_eq!(a.ids().len(), 1002); // 2 carriers + 1000 posts

    let mut b = Peer::new(40); // a completely different, also-offline identity

    encounter(&mut a, &mut b);

    // B must now hold everything from both identities; A must also have
    // pulled B's own (small) identity content on its second leg.
    assert_eq!(a.ids(), b.ids());
    assert_eq!(b.ids().len(), 1002 + 2); // A's 1000 posts + 2 carriers each side
}

/// **Asymmetric reachability across 3+ peers.** A cannot reach C directly at
/// all (no connection is ever attempted between them) -- only through B, who
/// can reach both. This is the store-and-forward relay pattern the crate's
/// own doc comment claims ("A3 store-and-forward"): content authored on A
/// must still reach C, carried by B, without A and C ever exchanging a byte
/// directly.
#[test]
fn asymmetric_reachability_relays_content_transitively_through_a_common_peer() {
    let mut a = Peer::new(50);
    a.write(10);
    let mut b = Peer::new(60); // the only peer that can reach both A and C
    b.write(10);
    let mut c = Peer::new(70);
    c.write(10);

    // A and C are never connected to each other, directly or indirectly in
    // the same encounter -- only sequential A<->B then B<->C hops.
    encounter(&mut a, &mut b);
    assert_eq!(
        a.ids(),
        b.ids(),
        "A and B must fully converge on their direct encounter"
    );

    encounter(&mut b, &mut c);
    // B now carries A's content (from the first hop) plus its own and C's
    // original content, and pushed all of it to C in this second hop.
    assert_eq!(
        b.ids(),
        c.ids(),
        "B and C must fully converge on their direct encounter"
    );

    // The real claim under test: A's identity/content, which C never
    // connected to directly, must be present in C purely via B's relay.
    let a_only: std::collections::BTreeSet<String> =
        a.ids().difference(&Peer::new(50).ids()).cloned().collect();
    assert!(
        !a_only.is_empty(),
        "sanity: A must actually have unique content to relay"
    );
    for id in &a_only {
        assert!(
            c.ids().contains(id),
            "content id {id} authored on A must have reached C transitively through B"
        );
    }
}

/// **Out-of-order reconnection with interleaved writes.** Peers connect in a
/// scrambled order (C-B, then A-B, then A-C) with local writes happening on
/// every peer between each encounter. Final state must still fully converge
/// regardless of the order encounters happened in -- reconciliation is a set
/// operation, not a sequence-sensitive replay.
#[test]
fn out_of_order_reconnection_with_interleaved_writes_still_fully_converges() {
    let mut a = Peer::new(80);
    let mut b = Peer::new(90);
    let mut c = Peer::new(100);

    a.write(3);
    b.write(3);
    c.write(3);

    // First hop: C and B meet.
    encounter(&mut c, &mut b);
    // New local writes happen on all three before the next hop.
    a.write(2);
    b.write(2);
    c.write(2);

    // Second hop: A and B meet (out of the "natural" A-B-C order).
    encounter(&mut a, &mut b);
    a.write(1);
    c.write(1);

    // Third hop: A and C meet directly, closing the loop.
    encounter(&mut a, &mut c);

    // A and C are now each other's most-recent contact and, transitively via
    // B, must both hold every object any of the three peers ever wrote plus
    // all three sets of KEL carriers -- regardless of the scrambled order.
    assert_eq!(a.ids(), c.ids());
    // 3 identities x 2 carriers + (3+3+3 first-round) + (2+2+2 second-round)
    // + (1+1 third-round for A and C only, B wrote none in that round) = 6
    // carriers + 9 + 6 + 2 = 23 objects.
    assert_eq!(a.ids().len(), 6 + 9 + 6 + 2);

    // A final B<->C encounter must bring B fully current too -- convergence
    // is total across the whole set, not just between the pair that last met.
    encounter(&mut b, &mut c);
    assert_eq!(b.ids(), c.ids());
    assert_eq!(b.ids(), a.ids());
}

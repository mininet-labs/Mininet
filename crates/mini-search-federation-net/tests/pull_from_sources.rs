//! Black-box coverage using only this crate's public API and *compliant*
//! peers (both sides use `pull_source`/`serve_source`): the honest two-peer
//! F1+F2 pull, and `pull_from_sources`'s source-count refusal. See
//! `src/session.rs`'s inline unit tests for the noncompliant-peer
//! defense-in-depth cases that cannot be reached this way.

use std::thread;

use did_mini::{Capabilities, Controller};
use mini_bearer::{pair, Bearer, Channel, InProcessBearer, Initiator, Responder};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder};
use mini_search_federation::{publish_crawl_observation, publish_index_segment};
use mini_search_federation_net::{pull_from_sources, pull_source, serve_source, PeerSource};
use mini_store::{MemoryBackend, Store};
use mini_sync::{kel_carrier, KelCache};
use mini_web_types::{
    CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus, NormalizedHost,
    ProviderPseudonym, Scheme,
};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn url(host: &str, path: &str) -> CanonicalUrl {
    CanonicalUrl::new(
        Scheme::Https,
        NormalizedHost::new(host).unwrap(),
        None,
        path,
        None,
    )
    .unwrap()
}

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn observation(tag: &str) -> CrawlObservation {
    CrawlObservation {
        id: CrawlObservationId(digest(tag.as_bytes())),
        requested_url: url("example.org", "/"),
        final_url: url("example.org", "/"),
        observed_at_ms: 1,
        status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
        content_digest: None,
        media_type: None,
        byte_length: None,
        redirect_chain: Vec::new(),
        crawler: ProviderPseudonym(digest(b"crawler")),
    }
}

/// Seed a store with one provider's F1 observation and F2 segment, plus its
/// own carriers so a client that already trusts the provider's KEL can
/// verify what it pulls.
fn seeded_provider(seed: u8, tag: &str) -> (Store<MemoryBackend>, Controller, Controller) {
    let (root, device) = human(seed);
    let mut store = Store::new(MemoryBackend::new());
    store
        .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
        .unwrap();
    store
        .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
        .unwrap();
    publish_crawl_observation(&mut store, &root.did(), &device, &observation(tag)).unwrap();
    let mut b = IndexBuilder::new();
    b.add_document(
        mini_lexical_index::UrlId(digest(tag.as_bytes())),
        &[(Field::Title, tag)],
    );
    publish_index_segment(&mut store, &root.did(), &device, &b.build()).unwrap();
    (store, root, device)
}

fn channels(a: &mut InProcessBearer, b: &mut InProcessBearer) -> (Channel, Channel) {
    let (init, hello1) = Initiator::start().unwrap();
    a.send(&hello1).unwrap();
    let got1 = b.recv().unwrap();
    let (chan_b, hello2) = Responder::respond(&got1).unwrap();
    b.send(&hello2).unwrap();
    let got2 = a.recv().unwrap();
    (init.finish(&got2).unwrap(), chan_b)
}

#[test]
fn a_compliant_peer_pulls_exactly_its_advertised_f1_and_f2_objects() {
    let (server_store, root, device) = seeded_provider(10, "obs-1");
    let candidate_ids = server_store.all_ids().unwrap();

    let mut client_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut client_cache = KelCache::new();
    client_cache.insert_verified(root.kel());
    client_cache.insert_verified(device.kel());

    let (mut client_bearer, mut server_bearer) = pair();
    let (mut client_chan, mut server_chan) = channels(&mut client_bearer, &mut server_bearer);

    let server_thread = thread::spawn(move || {
        serve_source(
            &mut server_bearer,
            &mut server_chan,
            &server_store,
            &candidate_ids,
        )
        .unwrap()
    });

    let expected = root.did();
    let report = pull_source(
        &mut client_bearer,
        &mut client_chan,
        &mut client_store,
        &mut client_cache,
        Some(&expected),
        64,
    )
    .unwrap();
    let offered = server_thread.join().unwrap();

    // 2 KEL carriers were on offer too, but `serve_source` filters to F1/F2
    // only -- carriers never reach the client through this path.
    assert_eq!(offered.len(), 2);
    assert_eq!(report.advertised, 2);
    assert_eq!(report.wrong_type, 0);
    assert_eq!(report.wrong_provider, 0);
    assert_eq!(report.trusted.len(), 2);
}

#[test]
fn pull_from_sources_refuses_more_peers_than_max_sources() {
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut cache = KelCache::new();
    let (mut a1, mut a2) = pair();
    let (mut chan1, chan2) = channels(&mut a1, &mut a2);
    let (mut b1, mut b2) = pair();
    let (mut chan3, chan4) = channels(&mut b1, &mut b2);

    let sources = vec![
        PeerSource {
            bearer: &mut a1,
            chan: &mut chan1,
            expected_provider: None,
            max_objects: 8,
        },
        PeerSource {
            bearer: &mut b1,
            chan: &mut chan3,
            expected_provider: None,
            max_objects: 8,
        },
    ];

    let err = pull_from_sources(sources, &mut store, &mut cache, 1).unwrap_err();
    assert_eq!(err, mini_search_federation_net::NetError::TooManySources);

    // Nobody actually talked over the wire -- the refusal happens before any
    // send/recv, so the peer ends are left untouched (drop cleanly).
    drop((a2, chan2, b2, chan4));
}

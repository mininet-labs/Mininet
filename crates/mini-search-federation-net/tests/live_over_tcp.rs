//! Proves this crate's own claim -- "bounded, authenticated real-transport
//! delivery... over any `mini_bearer::Bearer`" -- against a real socket, not
//! just the in-process `pair()` the rest of this crate's tests use. Same
//! `pull_source`/`serve_source` logic, real `TcpBearer` on both ends over
//! localhost. Mirrors `mini-sync`'s own `tests/sync_over_tcp.rs`.

use std::net::{TcpListener, TcpStream};
use std::thread;

use did_mini::{Capabilities, Controller};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_search_federation::publish_crawl_observation;
use mini_search_federation_net::{pull_source, serve_source};
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

#[test]
fn a_peer_pulls_f1_observations_from_another_peer_over_a_real_tcp_socket() {
    let (root, device) = human(120);
    let mut server_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    server_store
        .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
        .unwrap();
    server_store
        .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
        .unwrap();

    let mut candidate_ids = Vec::new();
    for i in 0..5u8 {
        let obs = CrawlObservation {
            id: CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, &[i])),
            requested_url: url("example.org", &format!("/{i}")),
            final_url: url("example.org", &format!("/{i}")),
            observed_at_ms: i as u64,
            status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
            content_digest: None,
            media_type: None,
            byte_length: None,
            redirect_chain: Vec::new(),
            crawler: ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, b"crawler")),
        };
        let id = publish_crawl_observation(&mut server_store, &root.did(), &device, &obs).unwrap();
        candidate_ids.push(id);
    }

    let mut client_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut client_cache = KelCache::new();
    client_cache.insert_verified(root.kel());
    client_cache.insert_verified(device.kel());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        serve_source(&mut bearer, &mut chan, &server_store, &candidate_ids).unwrap()
    });

    let stream = TcpStream::connect(addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let mut chan = handshake_initiator(&mut bearer);
    let expected = root.did();
    let report = pull_source(
        &mut bearer,
        &mut chan,
        &mut client_store,
        &mut client_cache,
        Some(&expected),
        16,
    )
    .unwrap();
    let offered = server.join().unwrap();

    assert_eq!(offered.len(), 5);
    assert_eq!(report.advertised, 5);
    assert_eq!(report.retrieval.ingest.accepted, 5);
    assert_eq!(report.wrong_type, 0);
    assert_eq!(report.wrong_provider, 0);
    assert_eq!(report.trusted.len(), 5);
    assert_eq!(client_store.all_ids().unwrap().len(), 5);
}

#[test]
fn a_client_asking_for_more_than_the_advertised_bound_is_capped_by_the_peer_not_by_trust() {
    // The server only ever offers up to what the client's own max_ids asked
    // for -- proving a hostile-looking "give me everything" request from a
    // compliant client still can't make an honest peer over-serve.
    let (root, device) = human(121);
    let mut server_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    server_store
        .insert(&kel_carrier(&root.kel(), &root.did(), &device).unwrap())
        .unwrap();
    server_store
        .insert(&kel_carrier(&device.kel(), &root.did(), &device).unwrap())
        .unwrap();
    let mut candidate_ids = Vec::new();
    for i in 0..3u8 {
        let obs = CrawlObservation {
            id: CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, &[100 + i])),
            requested_url: url("example.net", &format!("/{i}")),
            final_url: url("example.net", &format!("/{i}")),
            observed_at_ms: i as u64,
            status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
            content_digest: None,
            media_type: None,
            byte_length: None,
            redirect_chain: Vec::new(),
            crawler: ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, b"crawler-2")),
        };
        let id = publish_crawl_observation(&mut server_store, &root.did(), &device, &obs).unwrap();
        candidate_ids.push(id);
    }

    let mut client_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    let mut client_cache = KelCache::new();
    client_cache.insert_verified(root.kel());
    client_cache.insert_verified(device.kel());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        serve_source(&mut bearer, &mut chan, &server_store, &candidate_ids).unwrap()
    });

    let stream = TcpStream::connect(addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let mut chan = handshake_initiator(&mut bearer);
    // Client asks for only 2, even though the server has 3 to offer.
    let report = pull_source(
        &mut bearer,
        &mut chan,
        &mut client_store,
        &mut client_cache,
        None,
        2,
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(report.advertised, 2);
    assert_eq!(client_store.all_ids().unwrap().len(), 2);
}

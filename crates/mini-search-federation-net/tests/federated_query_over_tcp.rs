//! Closes the loop this crate exists for: a real `federate_query` call
//! (Track F3, unmodified) merging one local source with one source pulled
//! over an actual TCP socket -- proving a network-pulled F2 segment plus
//! F2b corpus bundle really can feed a live federated query, not just sit
//! decoded in memory.

use std::net::{TcpListener, TcpStream};
use std::thread;

use did_mini::{Capabilities, Controller};
use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, UrlId};
use mini_query::{parse_query, DocumentContext, DocumentContextTable};
use mini_ranker::{Corpus, DocumentMeta};
use mini_search_federation::{
    federate_query, publish_corpus_bundle, publish_index_segment, FederationSource,
};
use mini_search_federation_net::{assemble_federation_source, pull_source, serve_source};
use mini_store::{MemoryBackend, Store};
use mini_sync::KelCache;
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, NormalizedHost, ProviderPseudonym,
    RankingProfile, RankingProfileId, Scheme,
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

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn canonical(host: &str, path: &str) -> CanonicalUrl {
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

/// One local, in-process provider: a single document with strong inbound
/// links, so it reliably outranks the pulled provider's weaker document
/// under the public-default profile -- letting the test assert on merge
/// order, not just presence.
struct LocalProvider {
    provider: ProviderPseudonym,
    index: mini_lexical_index::IndexSegment,
    corpus: Corpus,
    contexts: DocumentContextTable,
    segment_id: mini_web_types::IndexSegmentId,
}

fn build_local_provider() -> LocalProvider {
    let mut b = IndexBuilder::new();
    let mut corpus = Corpus::new();
    let mut contexts = DocumentContextTable::new();
    let id = UrlId(digest(b"local-doc"));
    b.add_document(
        id.clone(),
        &[
            (Field::Title, "Federated Search"),
            (Field::Body, "the strongest local match"),
        ],
    );
    corpus.insert(
        &id,
        DocumentMeta {
            url: canonical("local.example", "/strong"),
            title: "Federated Search".to_string(),
            snippet: "the strongest local match".to_string(),
            observed_at_ms: 0,
            inbound_links: 100,
            content_digest: digest(b"local-doc-content"),
            availability: AvailabilityState::Available,
        },
    );
    contexts.insert(
        &id,
        DocumentContext {
            language: Some("en".to_string()),
            media_type: None,
            source_observation: CrawlObservationId(digest(b"local-obs")),
        },
    );
    let index = b.build();
    let segment_id = index.segment_id();
    LocalProvider {
        provider: ProviderPseudonym(digest(b"local-provider")),
        index,
        corpus,
        contexts,
        segment_id,
    }
}

#[test]
fn a_federated_query_merges_a_local_source_with_one_pulled_over_a_real_tcp_socket() {
    // --- The remote peer's own provider: published, then served over TCP. ---
    let (root, device) = human(140);
    let mut server_store: Store<MemoryBackend> = Store::new(MemoryBackend::new());

    let mut remote_index = IndexBuilder::new();
    let remote_doc = UrlId(digest(b"remote-doc"));
    remote_index.add_document(
        remote_doc.clone(),
        &[
            (Field::Title, "Federated Search"),
            (Field::Body, "a weaker remote match"),
        ],
    );
    let remote_segment = remote_index.build();
    let remote_segment_id = remote_segment.segment_id();

    let (segment_obj_id, published_segment_id) =
        publish_index_segment(&mut server_store, &root.did(), &device, &remote_segment).unwrap();
    assert_eq!(published_segment_id, remote_segment_id);

    let remote_meta = DocumentMeta {
        url: canonical("remote.example", "/weak"),
        title: "Federated Search".to_string(),
        snippet: "a weaker remote match".to_string(),
        observed_at_ms: 0,
        inbound_links: 1,
        content_digest: digest(b"remote-doc-content"),
        availability: AvailabilityState::Available,
    };
    let remote_context = DocumentContext {
        language: Some("en".to_string()),
        media_type: None,
        source_observation: CrawlObservationId(digest(b"remote-obs")),
    };
    let bundle_obj_id = publish_corpus_bundle(
        &mut server_store,
        &root.did(),
        &device,
        &remote_segment_id,
        &[(remote_doc.clone(), remote_meta)],
        &[(remote_doc, remote_context)],
    )
    .unwrap();

    let candidate_ids = vec![segment_obj_id, bundle_obj_id];

    // --- Pull both objects over a real TCP socket. ---
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
    let expected_provider = root.did();
    let report = pull_source(
        &mut bearer,
        &mut chan,
        &mut client_store,
        &mut client_cache,
        Some(&expected_provider),
        16,
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(report.trusted.len(), 2);

    // --- Assemble the pulled data into a real, owned federation source. ---
    let remote_provider = ProviderPseudonym(digest(b"remote-provider"));
    let owned = assemble_federation_source(&client_store, &report.trusted, remote_provider.clone())
        .unwrap();
    assert_eq!(owned.index_segment, remote_segment_id);
    assert_eq!(owned.corpus.len(), 1);
    assert_eq!(owned.contexts.len(), 1);

    // --- Run a real Track F3 federated query across local + pulled sources. ---
    let local = build_local_provider();
    let parsed = parse_query("federated search");
    let profile = RankingProfile::public_default(RankingProfileId(digest(b"public-default")));
    let sources = vec![
        FederationSource {
            provider: local.provider.clone(),
            index: &local.index,
            corpus: &local.corpus,
            contexts: &local.contexts,
            index_segment: local.segment_id.clone(),
        },
        owned.as_source(),
    ];

    let results = federate_query(&sources, &profile, &parsed, 1_000, 10).unwrap();

    assert_eq!(results.len(), 2);
    // The strongly-linked local document outranks the weakly-linked pulled
    // one under the identical query and profile.
    assert_eq!(results[0].provider, local.provider);
    assert_eq!(
        results[0].result.result.url.canonical_string(),
        "https://local.example/strong"
    );
    assert_eq!(results[1].provider, remote_provider);
    assert_eq!(
        results[1].result.result.url.canonical_string(),
        "https://remote.example/weak"
    );
}

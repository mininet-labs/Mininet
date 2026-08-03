//! Proves Track F6 Phase 1's `remote_query`/`serve_query` over a real
//! socket, not just the in-process `pair()` the unit tests in
//! `src/query.rs` use. Mirrors `live_over_tcp.rs`'s own TCP handshake
//! pattern for `pull_source`/`serve_source`.

use std::net::{TcpListener, TcpStream};
use std::thread;

use mini_bearer::{Bearer, Channel, Initiator, Responder, TcpBearer};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, UrlId};
use mini_query::DocumentContext;
use mini_ranker::{Corpus, DocumentMeta};
use mini_search_federation_net::remote_query;
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, IndexSegmentId, NormalizedHost,
    RankingProfile, RankingProfileId, Scheme,
};

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
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
fn a_client_queries_a_real_peer_over_a_real_tcp_socket_and_gets_ranked_results() {
    let doc_id = UrlId(digest(b"doc-1"));
    let mut b = IndexBuilder::new();
    b.add_document(doc_id.clone(), &[(Field::Title, "mininet search demo")]);
    let index = b.build();

    let mut corpus = Corpus::new();
    corpus.insert(
        &doc_id,
        DocumentMeta {
            url: url("example.org", "/"),
            title: "mininet search demo".to_string(),
            snippet: "a demo document about mininet search".to_string(),
            observed_at_ms: 0,
            inbound_links: 3,
            content_digest: digest(b"content"),
            availability: AvailabilityState::Available,
        },
    );

    let mut contexts = mini_query::DocumentContextTable::new();
    contexts.insert(
        &doc_id,
        DocumentContext {
            language: None,
            media_type: None,
            source_observation: CrawlObservationId(digest(b"obs-1")),
        },
    );

    let segment_id = IndexSegmentId(digest(b"segment-1"));
    let profile = RankingProfile::public_default(RankingProfileId(digest(b"profile-1")));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut bearer = TcpBearer::from_stream(stream).unwrap();
        let mut chan = handshake_responder(&mut bearer);
        mini_search_federation_net::serve_query(
            &mut bearer,
            &mut chan,
            &index,
            &corpus,
            &contexts,
            segment_id,
            1_000,
        )
        .unwrap();
    });

    let stream = TcpStream::connect(addr).unwrap();
    let mut bearer = TcpBearer::from_stream(stream).unwrap();
    let mut chan = handshake_initiator(&mut bearer);

    let results = remote_query(&mut bearer, &mut chan, "mininet", &profile, 8).unwrap();
    server.join().unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, url("example.org", "/"));
    assert_eq!(results[0].title, "mininet search demo");
}

use std::net::{SocketAddr, TcpListener};
use std::thread;

use did_mini::{Capabilities, Controller, FreshnessPins};
use mini_bearer::{Bearer, Responder, TcpBearer};
use mini_crypto::{AgreementPublicKey, AgreementSecretKey, HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_query::{DocumentContext, DocumentContextTable};
use mini_ranker::{Corpus, DocumentMeta};
use mini_search_federation_net::{
    authenticated_provider_pseudonym, merge_authenticated_remote_results,
    remote_query_authenticated, serve_query_authenticated, NetError,
};
use mini_transport_security::{
    authenticate_established_responder, connect_authenticated_tcp, AuthenticatedDialTarget,
    LocalSessionIdentity, PeerAdvertisement, PeerExpectation, ReplayCache, TransportPurpose,
    TransportSecurityError, VerifiedPeerAdvertisement,
};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, IndexSegmentId, NormalizedHost, RankingProfile,
    RankingProfileId, Scheme,
};

const NETWORK_ID: [u8; 32] = [7; 32];

struct Identity {
    root: Controller,
    device: Controller,
    routing: AgreementPublicKey,
}

impl Identity {
    fn new(seed: u8) -> Self {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        let routing = AgreementSecretKey::from_seed(&[seed + 4; 32]).public_key();
        Self {
            root,
            device,
            routing,
        }
    }

    fn local(&self) -> LocalSessionIdentity<'_> {
        LocalSessionIdentity::new(self.root.did(), &self.device, self.routing)
    }
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn verified_advertisement(identity: &Identity, address: SocketAddr) -> VerifiedPeerAdvertisement {
    let advertisement = PeerAdvertisement::issue(
        NETWORK_ID,
        &identity.root.did(),
        &identity.device,
        identity.routing,
        address,
        1_000,
        2_000,
    )
    .unwrap();
    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(16).unwrap();
    advertisement
        .verify(
            NETWORK_ID,
            1_500,
            &identity.root.kel(),
            &identity.device.kel(),
            &mut freshness,
            &mut replay,
        )
        .unwrap()
}

fn responder_channel(mut bearer: TcpBearer) -> (TcpBearer, mini_bearer::Channel) {
    let hello = bearer.recv().unwrap();
    let (channel, response) = Responder::respond(&hello).unwrap();
    bearer.send(&response).unwrap();
    (bearer, channel)
}

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

fn fixture() -> (
    IndexSegment,
    Corpus,
    DocumentContextTable,
    IndexSegmentId,
    RankingProfile,
) {
    let doc_id = UrlId(digest(b"doc-1"));
    let mut builder = IndexBuilder::new();
    builder.add_document(doc_id.clone(), &[(Field::Title, "hello free internet")]);
    let segment = builder.build();

    let mut corpus = Corpus::new();
    corpus.insert(
        &doc_id,
        DocumentMeta {
            url: url("example.org", "/"),
            title: "hello free internet".to_string(),
            snippet: "hello free internet".to_string(),
            observed_at_ms: 0,
            inbound_links: 0,
            content_digest: digest(b"content"),
            availability: AvailabilityState::Available,
        },
    );
    let mut contexts = DocumentContextTable::new();
    contexts.insert(
        &doc_id,
        DocumentContext {
            language: None,
            media_type: None,
            source_observation: mini_web_types::CrawlObservationId(digest(b"obs")),
        },
    );
    let segment_id = IndexSegmentId(digest(b"segment"));
    let profile = RankingProfile::public_default(RankingProfileId(digest(b"profile")));
    (segment, corpus, contexts, segment_id, profile)
}

#[test]
fn authenticated_search_response_carries_the_peer_bound_provider_label() {
    let client = Identity::new(10);
    let provider = Identity::new(40);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&provider, address);
    let provider_root_kel = provider.root.kel();
    let provider_device_kel = provider.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let (index, corpus, contexts, segment_id, profile) = fixture();

    let server_thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        let mut connection = authenticate_established_responder(
            bearer,
            channel,
            provider.local(),
            TransportPurpose::SearchQuery,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap();
        serve_query_authenticated(
            &mut connection,
            &index,
            &corpus,
            &contexts,
            segment_id,
            1_500,
        )
        .unwrap();
    });

    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let mut connection = connect_authenticated_tcp(
        client.local(),
        TransportPurpose::SearchQuery,
        1_000,
        2_000,
        1_500,
        AuthenticatedDialTarget::new(&advertisement, &provider_root_kel, &provider_device_kel),
        5_000,
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    let expected_provider = authenticated_provider_pseudonym(connection.peer());
    let remote = remote_query_authenticated(&mut connection, "hello", &profile, 8).unwrap();
    assert_eq!(remote.provider(), &expected_provider);
    assert_eq!(remote.results().len(), 1);

    let merged = merge_authenticated_remote_results(Vec::new(), remote, 8).unwrap();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].provider, expected_provider);
    server_thread.join().unwrap();
}

#[test]
fn a_peer_exchange_proof_cannot_be_reused_as_search_provider_provenance() {
    let client = Identity::new(10);
    let provider = Identity::new(40);
    let (listener, address) = listener();
    let advertisement = verified_advertisement(&provider, address);
    let provider_root_kel = provider.root.kel();
    let provider_device_kel = provider.device.kel();
    let client_root_kel = client.root.kel();
    let client_device_kel = client.device.kel();
    let (_, _, _, _, profile) = fixture();

    let server_thread = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        authenticate_established_responder(
            bearer,
            channel,
            provider.local(),
            TransportPurpose::PeerExchange,
            1_000,
            2_000,
            1_500,
            PeerExpectation::identity(&client_root_kel, &client_device_kel),
            &mut freshness,
            &mut replay,
        )
        .unwrap();
    });

    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let mut connection = connect_authenticated_tcp(
        client.local(),
        TransportPurpose::PeerExchange,
        1_000,
        2_000,
        1_500,
        AuthenticatedDialTarget::new(&advertisement, &provider_root_kel, &provider_device_kel),
        5_000,
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    assert_eq!(
        remote_query_authenticated(&mut connection, "hello", &profile, 8),
        Err(NetError::TransportSecurity(
            TransportSecurityError::WrongPurpose
        ))
    );
    server_thread.join().unwrap();
}

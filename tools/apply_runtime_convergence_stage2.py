#!/usr/bin/env python3
"""Apply PR #296 stage 2: bind F6 provider provenance to transport identity."""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(text, encoding="utf-8")


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    text = read(path)
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"{path}: expected {expected} matches, found {count}: {old[:100]!r}")
    write(path, text.replace(old, new))


def insert_before(path: str, marker: str, block: str) -> None:
    text = read(path)
    if text.count(marker) != 1:
        raise SystemExit(f"{path}: marker count is not one: {marker[:100]!r}")
    write(path, text.replace(marker, block + marker, 1))


cargo = "crates/mini-search-federation-net/Cargo.toml"
error = "crates/mini-search-federation-net/src/error.rs"
query = "crates/mini-search-federation-net/src/query.rs"
remote_merge = "crates/mini-search-federation-net/src/remote_merge.rs"
lib = "crates/mini-search-federation-net/src/lib.rs"

replace_exact(
    cargo,
    'mini-crypto = { path = "../mini-crypto" }\n',
    'mini-crypto = { path = "../mini-crypto" }\n'
    'mini-transport-security = { path = "../mini-transport-security" }\n',
)

replace_exact(
    error,
    """use mini_sync::SyncError;
""",
    """use mini_sync::SyncError;
use mini_transport_security::TransportSecurityError;
""",
)
replace_exact(
    error,
    """    Query(QueryError),
}
""",
    """    Query(QueryError),
    /// Optional named-peer authentication or authenticated-channel runtime
    /// failed. Anonymous CH1 querying remains a separate API.
    TransportSecurity(TransportSecurityError),
}
""",
)
replace_exact(
    error,
    """            NetError::Query(e) => write!(f, "query: {e}"),
""",
    """            NetError::Query(e) => write!(f, "query: {e}"),
            NetError::TransportSecurity(e) => write!(f, "transport security: {e}"),
""",
)
replace_exact(
    error,
    """impl From<QueryError> for NetError {
""",
    """impl From<TransportSecurityError> for NetError {
    fn from(e: TransportSecurityError) -> Self {
        NetError::TransportSecurity(e)
    }
}
impl From<QueryError> for NetError {
""",
)

replace_exact(
    query,
    """use mini_bearer::{Bearer, Channel};
use mini_crypto::Multihash;
""",
    """use mini_bearer::{Bearer, Channel};
use mini_crypto::{HashAlgorithm, Multihash};
""",
)
replace_exact(
    query,
    """use mini_ranker::Corpus;
use mini_web_types::{
    AvailabilityState, CanonicalUrl, IndexSegmentId, NormalizedHost, PersonalizationPolicy,
    RankingProfile, RankingProfileId, RestrictionReason, Scheme, UnavailabilityReason, WeightBps,
};
""",
    """use mini_ranker::Corpus;
use mini_transport_security::{
    AuthenticatedConnection, AuthenticatedPeer, TransportPurpose, TransportSecurityError,
};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, IndexSegmentId, NormalizedHost, PersonalizationPolicy,
    ProviderPseudonym, RankingProfile, RankingProfileId, RestrictionReason, Scheme,
    UnavailabilityReason, WeightBps,
};
""",
)
replace_exact(
    query,
    """const QUERY_AAD: &[u8] = b"MINI/SEARCHFED-QUERY1";
""",
    """const QUERY_AAD: &[u8] = b"MINI/SEARCHFED-QUERY1";
const AUTHENTICATED_PROVIDER_DOMAIN: &[u8] =
    b"mini-search-federation-net/authenticated-provider/v1";
""",
)

new_query_api = r'''
/// Remote results whose provider label came from the peer identity proved on
/// the exact channel carrying the response. Unlike `merge_remote_results`'s
/// legacy caller-supplied label, this value has no public constructor that takes
/// an arbitrary provider pseudonym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedQueryResults {
    pub provider: ProviderPseudonym,
    pub results: Vec<WireResult>,
}

/// Derive a rotating search-provider pseudonym from an authenticated transport
/// endpoint. The endpoint id already commits to the delegated device and current
/// X25519 routing key, so key rotation also rotates this provider label.
pub fn authenticated_provider_pseudonym(peer: &AuthenticatedPeer) -> ProviderPseudonym {
    let mut transcript = Vec::with_capacity(AUTHENTICATED_PROVIDER_DOMAIN.len() + 32);
    transcript.extend_from_slice(AUTHENTICATED_PROVIDER_DOMAIN);
    transcript.extend_from_slice(&peer.endpoint_id.to_bytes());
    ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, &transcript))
}

/// Named-provider form of [`remote_query`]. The same bounded request and response
/// codec is used, but the connection must have been authenticated specifically
/// for [`TransportPurpose::SearchQuery`], and the returned provider label is
/// derived from that verified peer rather than accepted from the caller.
pub fn remote_query_authenticated<B: Bearer>(
    connection: &mut AuthenticatedConnection<B>,
    query_text: &str,
    profile: &RankingProfile,
    max_results: u32,
) -> Result<AuthenticatedQueryResults> {
    if connection.peer().purpose != TransportPurpose::SearchQuery {
        return Err(NetError::TransportSecurity(
            TransportSecurityError::WrongPurpose,
        ));
    }
    if query_text.len() > MAX_QUERY_TEXT_BYTES {
        return Err(NetError::LimitExceeded);
    }
    if max_results == 0 || max_results > MAX_QUERY_RESULTS {
        return Err(NetError::LimitExceeded);
    }
    let request = Msg::QueryRequest {
        query: query_text.to_string(),
        profile: profile.clone(),
        max_results,
    };
    connection.send(&request.encode(), QUERY_AAD)?;
    let response = Msg::decode(&connection.recv(QUERY_AAD)?)?;
    let results = match response {
        Msg::QueryResponse { results } => {
            if results.len() > max_results as usize {
                return Err(NetError::LimitExceeded);
            }
            results
        }
        _ => return Err(NetError::Protocol),
    };
    Ok(AuthenticatedQueryResults {
        provider: authenticated_provider_pseudonym(connection.peer()),
        results,
    })
}

/// Named-peer form of [`serve_query`]. The requester must have proved a
/// channel-bound identity for the typed search purpose before any query bytes
/// are accepted. This is optional; providers may continue to serve anonymous
/// CH1 callers through [`serve_query`].
#[allow(clippy::too_many_arguments)]
pub fn serve_query_authenticated<B: Bearer>(
    connection: &mut AuthenticatedConnection<B>,
    index: &IndexSegment,
    corpus: &Corpus,
    contexts: &DocumentContextTable,
    index_segment: IndexSegmentId,
    now_ms: u64,
) -> Result<()> {
    if connection.peer().purpose != TransportPurpose::SearchQuery {
        return Err(NetError::TransportSecurity(
            TransportSecurityError::WrongPurpose,
        ));
    }
    let request = Msg::decode(&connection.recv(QUERY_AAD)?)?;
    let (query, profile, max_results) = match request {
        Msg::QueryRequest {
            query,
            profile,
            max_results,
        } => (query, profile, max_results),
        _ => return Err(NetError::Protocol),
    };
    if max_results == 0 || max_results > MAX_QUERY_RESULTS {
        return Err(NetError::LimitExceeded);
    }

    let parsed = parse_query(&query);
    let ranked = search(
        index,
        corpus,
        contexts,
        &profile,
        &parsed,
        index_segment,
        now_ms,
        max_results as usize,
    )?;
    let results: Vec<WireResult> = ranked
        .into_iter()
        .map(|rp| WireResult {
            url: rp.result.url,
            title: rp.result.title,
            snippet: rp.result.snippet,
            relevance_score_bps: rp.result.relevance_score_bps.value(),
            availability: rp.result.availability,
            ranking_profile: rp.result.ranking_profile,
            explanation: [
                rp.result.explanation.lexical_bps.value(),
                rp.result.explanation.phrase_bps.value(),
                rp.result.explanation.link_bps.value(),
                rp.result.explanation.freshness_bps.value(),
                rp.result.explanation.originality_bps.value(),
                rp.result.explanation.diversity_bps.value(),
            ],
            source_observation: rp.source_observation.0,
            index_segment: rp.index_segment,
        })
        .collect();
    connection.send(&Msg::QueryResponse { results }.encode(), QUERY_AAD)?;
    Ok(())
}

'''
insert_before(query, "/// Server side: answer one peer's query against this provider's own\n", new_query_api)

replace_exact(
    remote_merge,
    """use crate::query::WireResult;
""",
    """use crate::query::{AuthenticatedQueryResults, WireResult};
""",
)
new_merge_api = r'''
/// Merge authenticated remote results without accepting a caller-selected
/// provider label. The label is carried by [`AuthenticatedQueryResults`], which
/// can only be produced by the named-peer query path on an authenticated
/// transport connection.
pub fn merge_authenticated_remote_results(
    local: Vec<FederatedResult>,
    remote: AuthenticatedQueryResults,
    max_results: usize,
) -> Result<Vec<FederatedResult>> {
    merge_remote_results(local, remote.results, remote.provider, max_results)
}

'''
insert_before(remote_merge, "#[cfg(test)]\n", new_merge_api)

replace_exact(
    lib,
    """pub use query::{remote_query, serve_query, WireResult, MAX_QUERY_RESULTS, MAX_QUERY_TEXT_BYTES};
pub use remote_merge::{federated_result_from_wire, merge_remote_results};
""",
    """pub use query::{
    authenticated_provider_pseudonym, remote_query, remote_query_authenticated, serve_query,
    serve_query_authenticated, AuthenticatedQueryResults, WireResult, MAX_QUERY_RESULTS,
    MAX_QUERY_TEXT_BYTES,
};
pub use remote_merge::{
    federated_result_from_wire, merge_authenticated_remote_results, merge_remote_results,
};
""",
)

integration = r'''use std::net::{SocketAddr, TcpListener};
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
        LocalSessionIdentity::new(&self.root.did(), &self.device, self.routing)
    }
}

fn listener() -> (TcpListener, SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    (listener, address)
}

fn verified_advertisement(
    identity: &Identity,
    address: SocketAddr,
) -> VerifiedPeerAdvertisement {
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
        AuthenticatedDialTarget::new(
            &advertisement,
            &provider_root_kel,
            &provider_device_kel,
        ),
        5_000,
        &mut freshness,
        &mut replay,
    )
    .unwrap();
    let expected_provider = authenticated_provider_pseudonym(connection.peer());
    let remote = remote_query_authenticated(&mut connection, "hello", &profile, 8).unwrap();
    assert_eq!(remote.provider, expected_provider);
    assert_eq!(remote.results.len(), 1);

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
        AuthenticatedDialTarget::new(
            &advertisement,
            &provider_root_kel,
            &provider_device_kel,
        ),
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
'''
write(
    "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs",
    integration,
)

print("stage 2 applied")

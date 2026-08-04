//! Track F6 Phase 1: a bounded, confidential-in-transit query/response round
//! trip against one already-dialed peer's already-held index. See
//! `docs/design/f6-private-query-transport.md` for the full doctrine --
//! most importantly, this is **not** a private-information-retrieval
//! scheme: the queried peer sees the caller's exact query text. What it
//! does provide is that the query never crosses the wire in cleartext (the
//! channel's own AEAD covers it) and that the requester discloses no
//! identity of its own to run a query (CH1 needs none).
//!
//! Unlike F1/F2/F2b, a query response is not wrapped in a signed `Object`:
//! it is answered fresh for exactly this request and is not meant to be
//! durably stored, replayed, or independently re-verified later. Its only
//! integrity property -- "this came from whoever is on the other end of
//! this channel" -- is exactly what the channel's own authenticated
//! encryption already gives.

use mini_bearer::{Bearer, Channel};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::IndexSegment;
use mini_query::{parse_query, search, DocumentContextTable};
use mini_ranker::Corpus;
use mini_transport_security::{AuthenticatedConnection, TransportPurpose, TransportSecurityError};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, IndexSegmentId, NormalizedHost, PersonalizationPolicy,
    ProviderPseudonym, RankingProfile, RankingProfileId, RestrictionReason, Scheme,
    UnavailabilityReason, WeightBps,
};

use crate::error::{NetError, Result};

const QUERY_AAD: &[u8] = b"MINI/SEARCHFED-QUERY1";
const AUTHENTICATED_PROVIDER_DOMAIN: &[u8] =
    b"mini-search-federation-net/authenticated-provider/v1";

/// Hard ceiling on a raw query string's byte length.
pub const MAX_QUERY_TEXT_BYTES: usize = 512;
/// Hard ceiling on how many results a caller may request, or a compliant
/// server may ever return, in one query.
pub const MAX_QUERY_RESULTS: u32 = 64;

const MAX_HOST_BYTES: usize = 253;
const MAX_PATH_BYTES: usize = 4096;
const MAX_URL_QUERY_BYTES: usize = 4096;
const MAX_TITLE_BYTES: usize = 512;
const MAX_SNIPPET_BYTES: usize = 2048;
const MAX_JURISDICTION_BYTES: usize = 128;
const MAX_MULTIHASH_BYTES: usize = 128;

/// One remote-ranked result: [`mini_query::ResultProvenance`]'s fields,
/// flattened for the wire. Deliberately not `mini_web_types::SearchResult`
/// itself -- this is a response payload for this one live request, not a
/// durable object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireResult {
    pub url: CanonicalUrl,
    pub title: String,
    pub snippet: String,
    pub relevance_score_bps: u16,
    pub availability: AvailabilityState,
    pub ranking_profile: RankingProfileId,
    pub explanation: [u16; 6],
    pub source_observation: Multihash,
    pub index_segment: IndexSegmentId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    QueryRequest {
        query: String,
        profile: RankingProfile,
        max_results: u32,
    },
    QueryResponse {
        results: Vec<WireResult>,
    },
}

const T_REQUEST: u8 = 1;
const T_RESPONSE: u8 = 2;

struct Writer(Vec<u8>);
impl Writer {
    fn new() -> Self {
        Writer(Vec::new())
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
    }
    fn str(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn opt_str(&mut self, v: &Option<String>) {
        match v {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }
    fn finish(self) -> Vec<u8> {
        self.0
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.b.len() - self.p {
            return Err(NetError::Protocol);
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes_limited(&mut self, max: usize) -> Result<Vec<u8>> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(NetError::LimitExceeded);
        }
        Ok(self.take(len)?.to_vec())
    }
    fn str_limited(&mut self, max: usize) -> Result<String> {
        String::from_utf8(self.bytes_limited(max)?).map_err(|_| NetError::Protocol)
    }
    fn opt_str_limited(&mut self, max: usize) -> Result<Option<String>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.str_limited(max)?)),
            _ => Err(NetError::Protocol),
        }
    }
    fn finished(&self) -> bool {
        self.p == self.b.len()
    }
}

fn encode_scheme(w: &mut Writer, s: Scheme) {
    match s {
        Scheme::Http => w.u8(0),
        Scheme::Https => w.u8(1),
        // `Scheme` is `#[non_exhaustive]` upstream; any future variant this
        // crate does not know about yet still round-trips as Https rather
        // than failing to encode at all.
        _ => w.u8(1),
    }
}
fn decode_scheme(r: &mut Reader) -> Result<Scheme> {
    match r.u8()? {
        0 => Ok(Scheme::Http),
        1 => Ok(Scheme::Https),
        _ => Err(NetError::Protocol),
    }
}

fn encode_url(w: &mut Writer, u: &CanonicalUrl) {
    encode_scheme(w, u.scheme);
    w.str(u.host.as_str());
    match u.port {
        Some(p) => {
            w.u8(1);
            w.u16(p);
        }
        None => w.u8(0),
    }
    w.str(&u.path);
    w.opt_str(&u.query);
}
fn decode_url(r: &mut Reader) -> Result<CanonicalUrl> {
    let scheme = decode_scheme(r)?;
    let host =
        NormalizedHost::new(r.str_limited(MAX_HOST_BYTES)?).map_err(|_| NetError::Protocol)?;
    let port = match r.u8()? {
        0 => None,
        1 => Some(r.u16()?),
        _ => return Err(NetError::Protocol),
    };
    let path = r.str_limited(MAX_PATH_BYTES)?;
    let query = r.opt_str_limited(MAX_URL_QUERY_BYTES)?;
    CanonicalUrl::new(scheme, host, port, path, query).map_err(|_| NetError::Protocol)
}

fn encode_availability(w: &mut Writer, a: &AvailabilityState) {
    match a {
        AvailabilityState::Available => w.u8(0),
        AvailabilityState::Unavailable(reason) => {
            w.u8(1);
            match reason {
                UnavailabilityReason::NotFetched => w.u8(0),
                UnavailabilityReason::FetchFailed => w.u8(1),
                UnavailabilityReason::Gone => w.u8(2),
                UnavailabilityReason::UnsupportedContent => w.u8(3),
                // `UnavailabilityReason` is `#[non_exhaustive]` upstream; a
                // future variant this crate does not know about yet still
                // round-trips as a generic "unavailable, unspecified" state
                // rather than failing to encode at all.
                _ => w.u8(255),
            }
        }
        AvailabilityState::Restricted(reason) => {
            w.u8(2);
            match reason {
                RestrictionReason::RobotsExcluded => w.u8(0),
                RestrictionReason::LegalRestriction { jurisdiction } => {
                    w.u8(1);
                    w.str(jurisdiction);
                }
                RestrictionReason::Malware => w.u8(2),
                RestrictionReason::Spam => w.u8(3),
                RestrictionReason::UserFilter => w.u8(4),
                RestrictionReason::SafetyWarning => w.u8(5),
                _ => w.u8(255),
            }
        }
        // `AvailabilityState` is `#[non_exhaustive]` upstream; a future
        // top-level variant still round-trips as a generic restriction
        // rather than failing to encode.
        _ => {
            w.u8(2);
            w.u8(255);
        }
    }
}
fn decode_availability(r: &mut Reader) -> Result<AvailabilityState> {
    Ok(match r.u8()? {
        0 => AvailabilityState::Available,
        1 => AvailabilityState::Unavailable(match r.u8()? {
            0 => UnavailabilityReason::NotFetched,
            1 => UnavailabilityReason::FetchFailed,
            2 => UnavailabilityReason::Gone,
            3 => UnavailabilityReason::UnsupportedContent,
            255 => UnavailabilityReason::FetchFailed,
            _ => return Err(NetError::Protocol),
        }),
        2 => AvailabilityState::Restricted(match r.u8()? {
            0 => RestrictionReason::RobotsExcluded,
            1 => RestrictionReason::LegalRestriction {
                jurisdiction: r.str_limited(MAX_JURISDICTION_BYTES)?,
            },
            2 => RestrictionReason::Malware,
            3 => RestrictionReason::Spam,
            4 => RestrictionReason::UserFilter,
            5 => RestrictionReason::SafetyWarning,
            255 => RestrictionReason::UserFilter,
            _ => return Err(NetError::Protocol),
        }),
        _ => return Err(NetError::Protocol),
    })
}

fn encode_personalization(w: &mut Writer, p: &PersonalizationPolicy) {
    match p {
        PersonalizationPolicy::None => w.u8(0),
        PersonalizationPolicy::LocalUserControlled => w.u8(1),
        // `PersonalizationPolicy` is `#[non_exhaustive]` upstream; a future
        // variant still round-trips as `None` rather than failing to encode.
        _ => w.u8(0),
    }
}
fn decode_personalization(r: &mut Reader) -> Result<PersonalizationPolicy> {
    match r.u8()? {
        0 => Ok(PersonalizationPolicy::None),
        1 => Ok(PersonalizationPolicy::LocalUserControlled),
        _ => Err(NetError::Protocol),
    }
}

fn encode_profile(w: &mut Writer, p: &RankingProfile) {
    w.bytes(&p.id.0.to_bytes());
    w.u16(p.version);
    w.u16(p.lexical_weight.value());
    w.u16(p.phrase_weight.value());
    w.u16(p.link_weight.value());
    w.u16(p.freshness_weight.value());
    w.u16(p.originality_weight.value());
    w.u16(p.diversity_weight.value());
    encode_personalization(w, &p.personalization);
}
fn decode_profile(r: &mut Reader) -> Result<RankingProfile> {
    let id = RankingProfileId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| NetError::Protocol)?,
    );
    let version = r.u16()?;
    let weight = |r: &mut Reader| -> Result<WeightBps> {
        WeightBps::new(r.u16()?).map_err(|_| NetError::Protocol)
    };
    let lexical_weight = weight(r)?;
    let phrase_weight = weight(r)?;
    let link_weight = weight(r)?;
    let freshness_weight = weight(r)?;
    let originality_weight = weight(r)?;
    let diversity_weight = weight(r)?;
    let personalization = decode_personalization(r)?;
    Ok(RankingProfile {
        id,
        version,
        lexical_weight,
        phrase_weight,
        link_weight,
        freshness_weight,
        originality_weight,
        diversity_weight,
        personalization,
    })
}

fn validate_multihash(value: &Multihash) -> Result<()> {
    if value.to_bytes().len() > MAX_MULTIHASH_BYTES {
        return Err(NetError::LimitExceeded);
    }
    Ok(())
}

fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)?;
    profile.validate().map_err(|_| NetError::Protocol)
}

fn validate_url(url: &CanonicalUrl) -> Result<()> {
    if url.host.as_str().len() > MAX_HOST_BYTES
        || url.path.len() > MAX_PATH_BYTES
        || url
            .query
            .as_ref()
            .is_some_and(|query| query.len() > MAX_URL_QUERY_BYTES)
    {
        return Err(NetError::LimitExceeded);
    }
    let reconstructed = CanonicalUrl::new(
        url.scheme,
        url.host.clone(),
        url.port,
        url.path.clone(),
        url.query.clone(),
    )
    .map_err(|_| NetError::Protocol)?;
    if reconstructed != *url {
        return Err(NetError::Protocol);
    }
    Ok(())
}

fn validate_availability(availability: &AvailabilityState) -> Result<()> {
    if let AvailabilityState::Restricted(RestrictionReason::LegalRestriction { jurisdiction }) =
        availability
    {
        if jurisdiction.len() > MAX_JURISDICTION_BYTES {
            return Err(NetError::LimitExceeded);
        }
    }
    Ok(())
}

fn validate_wire_result(result: &WireResult) -> Result<()> {
    validate_url(&result.url)?;
    validate_availability(&result.availability)?;
    if !result.availability.is_displayable() {
        return Err(NetError::Protocol);
    }
    validate_multihash(&result.ranking_profile.0)?;
    validate_multihash(&result.source_observation)?;
    validate_multihash(&result.index_segment.0)?;
    if result.title.len() > MAX_TITLE_BYTES || result.snippet.len() > MAX_SNIPPET_BYTES {
        return Err(NetError::LimitExceeded);
    }
    if result.relevance_score_bps > WeightBps::MAX.value()
        || result
            .explanation
            .iter()
            .any(|weight| *weight > WeightBps::MAX.value())
    {
        return Err(NetError::Protocol);
    }
    Ok(())
}

fn validate_query_response(
    results: &[WireResult],
    requested_profile: &RankingProfile,
    max_results: u32,
) -> Result<()> {
    if max_results == 0 || max_results > MAX_QUERY_RESULTS || results.len() > max_results as usize {
        return Err(NetError::LimitExceeded);
    }
    for result in results {
        validate_wire_result(result)?;
        if result.ranking_profile != requested_profile.id {
            return Err(NetError::Protocol);
        }
    }
    Ok(())
}

fn encode_result(w: &mut Writer, r: &WireResult) {
    encode_url(w, &r.url);
    w.str(&r.title);
    w.str(&r.snippet);
    w.u16(r.relevance_score_bps);
    encode_availability(w, &r.availability);
    w.bytes(&r.ranking_profile.0.to_bytes());
    for weight in r.explanation {
        w.u16(weight);
    }
    w.bytes(&r.source_observation.to_bytes());
    w.bytes(&r.index_segment.0.to_bytes());
}
fn decode_result(r: &mut Reader) -> Result<WireResult> {
    let url = decode_url(r)?;
    let title = r.str_limited(MAX_TITLE_BYTES)?;
    let snippet = r.str_limited(MAX_SNIPPET_BYTES)?;
    let relevance_score_bps = r.u16()?;
    let availability = decode_availability(r)?;
    let ranking_profile = RankingProfileId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| NetError::Protocol)?,
    );
    let mut explanation = [0u16; 6];
    for w in &mut explanation {
        *w = r.u16()?;
    }
    let source_observation = Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
        .map_err(|_| NetError::Protocol)?;
    let index_segment = IndexSegmentId(
        Multihash::from_bytes(&r.bytes_limited(MAX_MULTIHASH_BYTES)?)
            .map_err(|_| NetError::Protocol)?,
    );
    Ok(WireResult {
        url,
        title,
        snippet,
        relevance_score_bps,
        availability,
        ranking_profile,
        explanation,
        source_observation,
        index_segment,
    })
}

impl Msg {
    fn validate(&self) -> Result<()> {
        match self {
            Msg::QueryRequest {
                query,
                profile,
                max_results,
            } => {
                if query.len() > MAX_QUERY_TEXT_BYTES
                    || *max_results == 0
                    || *max_results > MAX_QUERY_RESULTS
                {
                    return Err(NetError::LimitExceeded);
                }
                validate_profile(profile)
            }
            Msg::QueryResponse { results } => {
                if results.len() > MAX_QUERY_RESULTS as usize {
                    return Err(NetError::LimitExceeded);
                }
                for result in results {
                    validate_wire_result(result)?;
                }
                Ok(())
            }
        }
    }

    fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut w = Writer::new();
        match self {
            Msg::QueryRequest {
                query,
                profile,
                max_results,
            } => {
                w.u8(T_REQUEST);
                w.str(query);
                encode_profile(&mut w, profile);
                w.u32(*max_results);
            }
            Msg::QueryResponse { results } => {
                w.u8(T_RESPONSE);
                w.u32(results.len() as u32);
                for result in results {
                    encode_result(&mut w, result);
                }
            }
        }
        Ok(w.finish())
    }

    fn decode(bytes: &[u8]) -> Result<Msg> {
        let mut r = Reader::new(bytes);
        let tag = r.u8()?;
        let msg = match tag {
            T_REQUEST => {
                let query = r.str_limited(MAX_QUERY_TEXT_BYTES)?;
                let profile = decode_profile(&mut r)?;
                let max_results = r.u32()?;
                Msg::QueryRequest {
                    query,
                    profile,
                    max_results,
                }
            }
            T_RESPONSE => {
                let n = r.u32()? as usize;
                if n > MAX_QUERY_RESULTS as usize {
                    return Err(NetError::LimitExceeded);
                }
                let mut results = Vec::with_capacity(n);
                for _ in 0..n {
                    results.push(decode_result(&mut r)?);
                }
                Msg::QueryResponse { results }
            }
            _ => return Err(NetError::Protocol),
        };
        if !r.finished() {
            return Err(NetError::Protocol);
        }
        msg.validate()?;
        Ok(msg)
    }
}

fn send(bearer: &mut dyn Bearer, chan: &mut Channel, msg: &Msg) -> Result<()> {
    let ct = chan.seal(&msg.encode()?, QUERY_AAD)?;
    bearer.send(&ct)?;
    Ok(())
}
fn recv(bearer: &mut dyn Bearer, chan: &mut Channel) -> Result<Msg> {
    let ct = bearer.recv()?;
    let pt = chan.open(&ct, QUERY_AAD)?;
    Msg::decode(&pt)
}

/// Client side: send `query_text` (parsed and ranked by the peer, not
/// locally) plus `profile` and a `max_results` cap to an already-dialed
/// peer, and return its bounded, ranked answer. The peer sees `query_text`
/// in full -- see the module doc and
/// `docs/design/f6-private-query-transport.md` for why this is not
/// query-content-private.
pub fn remote_query(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    query_text: &str,
    profile: &RankingProfile,
    max_results: u32,
) -> Result<Vec<WireResult>> {
    if query_text.len() > MAX_QUERY_TEXT_BYTES {
        return Err(NetError::LimitExceeded);
    }
    if max_results == 0 || max_results > MAX_QUERY_RESULTS {
        return Err(NetError::LimitExceeded);
    }
    send(
        bearer,
        chan,
        &Msg::QueryRequest {
            query: query_text.to_string(),
            profile: profile.clone(),
            max_results,
        },
    )?;
    match recv(bearer, chan)? {
        Msg::QueryResponse { results } => {
            validate_query_response(&results, profile, max_results)?;
            Ok(results)
        }
        _ => Err(NetError::Protocol),
    }
}

/// Remote results whose provider label came from the peer identity proved on
/// the exact channel carrying the response. Unlike `merge_remote_results`'s
/// legacy caller-supplied label, this value has no public constructor that takes
/// an arbitrary provider pseudonym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedQueryResults {
    provider: ProviderPseudonym,
    results: Vec<WireResult>,
}

impl AuthenticatedQueryResults {
    /// Provider label derived from the endpoint authenticated on the response
    /// channel. No public constructor accepts an arbitrary replacement label.
    pub fn provider(&self) -> &ProviderPseudonym {
        &self.provider
    }

    pub fn results(&self) -> &[WireResult] {
        &self.results
    }

    pub(crate) fn into_parts(self) -> (ProviderPseudonym, Vec<WireResult>) {
        (self.provider, self.results)
    }
}

/// Derive a channel-scoped provider pseudonym from a sealed authenticated
/// connection. Binding both the verified endpoint and exact CH1 transcript
/// prevents a caller from manufacturing provenance from a freely constructed
/// `AuthenticatedPeer`, avoids cross-session tracking, and stays stable for
/// repeated queries on this one connection.
fn authenticated_provider_pseudonym<B: Bearer>(
    connection: &AuthenticatedConnection<B>,
) -> ProviderPseudonym {
    let mut transcript = Vec::with_capacity(AUTHENTICATED_PROVIDER_DOMAIN.len() + 64);
    transcript.extend_from_slice(AUTHENTICATED_PROVIDER_DOMAIN);
    transcript.extend_from_slice(&connection.peer().endpoint_id.to_bytes());
    transcript.extend_from_slice(&connection.channel_binding());
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
    connection.send(&request.encode()?, QUERY_AAD)?;
    let response = Msg::decode(&connection.recv(QUERY_AAD)?)?;
    let results = match response {
        Msg::QueryResponse { results } => {
            validate_query_response(&results, profile, max_results)?;
            results
        }
        _ => return Err(NetError::Protocol),
    };
    Ok(AuthenticatedQueryResults {
        provider: authenticated_provider_pseudonym(connection),
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
    let response = Msg::QueryResponse { results };
    connection.send(&response.encode()?, QUERY_AAD)?;
    Ok(())
}

/// Server side: answer one peer's query against this provider's own
/// already-held `index`/`corpus`/`contexts` for `index_segment`, using the
/// unmodified [`mini_query::parse_query`] and [`mini_query::search`]. Never
/// returns more than the requester's own `max_results`, and never more than
/// [`MAX_QUERY_RESULTS`] regardless of what the request claims.
pub fn serve_query(
    bearer: &mut dyn Bearer,
    chan: &mut Channel,
    index: &IndexSegment,
    corpus: &Corpus,
    contexts: &DocumentContextTable,
    index_segment: IndexSegmentId,
    now_ms: u64,
) -> Result<()> {
    let (query, profile, max_results) = match recv(bearer, chan)? {
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

    send(bearer, chan, &Msg::QueryResponse { results })
}

#[cfg(test)]
mod tests {
    use mini_query::DocumentContext;
    use mini_ranker::DocumentMeta;

    use super::*;

    fn channels(
        a: &mut mini_bearer::InProcessBearer,
        b: &mut mini_bearer::InProcessBearer,
    ) -> (Channel, Channel) {
        use mini_bearer::{Initiator, Responder};
        let (init, hello1) = Initiator::start().unwrap();
        a.send(&hello1).unwrap();
        let got1 = b.recv().unwrap();
        let (chan_b, hello2) = Responder::respond(&got1).unwrap();
        b.send(&hello2).unwrap();
        let got2 = a.recv().unwrap();
        (init.finish(&got2).unwrap(), chan_b)
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
        Multihash::of(mini_crypto::HashAlgorithm::Blake3, seed)
    }

    fn fixture() -> (
        IndexSegment,
        Corpus,
        DocumentContextTable,
        IndexSegmentId,
        RankingProfile,
    ) {
        use mini_lexical_index::{Field, IndexBuilder, UrlId};

        let doc_id = UrlId(digest(b"doc-1"));
        let mut b = IndexBuilder::new();
        b.add_document(doc_id.clone(), &[(Field::Title, "hello world")]);
        let segment = b.build();

        let mut corpus = Corpus::new();
        corpus.insert(
            &doc_id,
            DocumentMeta {
                url: url("example.org", "/"),
                title: "hello world".to_string(),
                snippet: "hello world".to_string(),
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
                source_observation: mini_web_types::CrawlObservationId(digest(b"obs-1")),
            },
        );

        let segment_id = IndexSegmentId(digest(b"segment-1"));
        let profile = RankingProfile::public_default(RankingProfileId(digest(b"profile-1")));
        (segment, corpus, contexts, segment_id, profile)
    }

    #[test]
    fn round_trip_matches_a_local_search() {
        let (index, corpus, contexts, segment_id, profile) = fixture();
        let (mut a, mut b) = mini_bearer::pair();
        let (mut chan_a, mut chan_b) = channels(&mut a, &mut b);

        let expected = search(
            &index,
            &corpus,
            &contexts,
            &profile,
            &parse_query("hello"),
            segment_id.clone(),
            1_000,
            8,
        )
        .unwrap();

        let server_index = index.clone();
        let server_corpus = corpus.clone();
        let server_contexts = contexts.clone();
        let server_segment = segment_id.clone();
        let server_thread = std::thread::spawn(move || {
            serve_query(
                &mut b,
                &mut chan_b,
                &server_index,
                &server_corpus,
                &server_contexts,
                server_segment,
                1_000,
            )
            .unwrap();
        });

        let results = remote_query(&mut a, &mut chan_a, "hello", &profile, 8).unwrap();
        server_thread.join().unwrap();

        assert_eq!(results.len(), expected.len());
        assert_eq!(results[0].url, expected[0].result.url);
        assert_eq!(results[0].title, expected[0].result.title);
        assert_eq!(
            results[0].relevance_score_bps,
            expected[0].result.relevance_score_bps.value()
        );
    }

    #[test]
    fn oversized_query_text_is_rejected_before_sending() {
        let (_, _, _, _, profile) = fixture();
        let (mut a, mut b) = mini_bearer::pair();
        let (mut chan_a, _chan_b) = channels(&mut a, &mut b);
        let huge = "x".repeat(MAX_QUERY_TEXT_BYTES + 1);
        assert_eq!(
            remote_query(&mut a, &mut chan_a, &huge, &profile, 8),
            Err(NetError::LimitExceeded)
        );
    }

    #[test]
    fn zero_or_oversized_max_results_is_rejected() {
        let (_, _, _, _, profile) = fixture();
        let (mut a, mut b) = mini_bearer::pair();
        let (mut chan_a, _chan_b) = channels(&mut a, &mut b);
        assert_eq!(
            remote_query(&mut a, &mut chan_a, "hello", &profile, 0),
            Err(NetError::LimitExceeded)
        );
        assert_eq!(
            remote_query(
                &mut a,
                &mut chan_a,
                "hello",
                &profile,
                MAX_QUERY_RESULTS + 1
            ),
            Err(NetError::LimitExceeded)
        );
    }

    #[test]
    fn outbound_and_inbound_codecs_enforce_the_same_field_bounds() {
        let (_, _, _, segment_id, profile) = fixture();
        let base = WireResult {
            url: url("example.org", "/"),
            title: "title".to_string(),
            snippet: "snippet".to_string(),
            relevance_score_bps: 100,
            availability: AvailabilityState::Available,
            ranking_profile: profile.id.clone(),
            explanation: [100, 0, 0, 0, 0, 0],
            source_observation: digest(b"obs"),
            index_segment: segment_id,
        };

        let mut oversized_title = base.clone();
        oversized_title.title = "x".repeat(MAX_TITLE_BYTES + 1);
        assert_eq!(
            Msg::QueryResponse {
                results: vec![oversized_title]
            }
            .encode(),
            Err(NetError::LimitExceeded)
        );

        let mut invalid_url = base.clone();
        invalid_url.url.path = "relative".to_string();
        assert_eq!(
            Msg::QueryResponse {
                results: vec![invalid_url]
            }
            .encode(),
            Err(NetError::Protocol)
        );

        let mut invalid_profile = profile.clone();
        invalid_profile.version = 0;
        assert_eq!(
            Msg::QueryRequest {
                query: "hello".to_string(),
                profile: invalid_profile,
                max_results: 8,
            }
            .encode(),
            Err(NetError::Protocol)
        );

        let mut invalid_score = base;
        invalid_score.relevance_score_bps = WeightBps::MAX.value() + 1;
        assert_eq!(
            Msg::QueryResponse {
                results: vec![invalid_score.clone()]
            }
            .encode(),
            Err(NetError::Protocol)
        );

        // Bypass the outbound validator to emulate a malicious peer and prove
        // the decoder applies the same semantic score bound.
        let mut writer = Writer::new();
        writer.u8(T_RESPONSE);
        writer.u32(1);
        encode_result(&mut writer, &invalid_score);
        assert_eq!(Msg::decode(&writer.finish()), Err(NetError::Protocol));
    }

    #[test]
    fn a_ranked_response_cannot_reintroduce_a_filtered_document() {
        let (_, _, _, segment_id, profile) = fixture();
        let restricted = WireResult {
            url: url("example.org", "/"),
            title: "title".to_string(),
            snippet: "snippet".to_string(),
            relevance_score_bps: 100,
            availability: AvailabilityState::Restricted(RestrictionReason::UserFilter),
            ranking_profile: profile.id.clone(),
            explanation: [100, 0, 0, 0, 0, 0],
            source_observation: digest(b"obs"),
            index_segment: segment_id,
        };
        assert_eq!(
            Msg::QueryResponse {
                results: vec![restricted.clone()]
            }
            .encode(),
            Err(NetError::Protocol)
        );

        // Emulate a malicious peer that bypassed the encoder and prove the
        // decoder independently preserves the displayability invariant.
        let mut writer = Writer::new();
        writer.u8(T_RESPONSE);
        writer.u32(1);
        encode_result(&mut writer, &restricted);
        assert_eq!(Msg::decode(&writer.finish()), Err(NetError::Protocol));
    }

    #[test]
    fn a_response_for_another_ranking_profile_is_rejected() {
        let (_, _, _, segment_id, profile) = fixture();
        let result = WireResult {
            url: url("example.org", "/"),
            title: "title".to_string(),
            snippet: "snippet".to_string(),
            relevance_score_bps: 100,
            availability: AvailabilityState::Available,
            ranking_profile: RankingProfileId(digest(b"wrong-profile")),
            explanation: [100, 0, 0, 0, 0, 0],
            source_observation: digest(b"obs"),
            index_segment: segment_id,
        };
        assert_eq!(
            validate_query_response(&[result], &profile, 8),
            Err(NetError::Protocol)
        );
    }

    #[test]
    fn a_compliant_server_never_exceeds_the_requested_max_results() {
        let (index, corpus, contexts, segment_id, profile) = fixture();
        let (mut a, mut b) = mini_bearer::pair();
        let (mut chan_a, mut chan_b) = channels(&mut a, &mut b);

        let server_thread = std::thread::spawn(move || {
            serve_query(
                &mut b,
                &mut chan_b,
                &index,
                &corpus,
                &contexts,
                segment_id,
                1_000,
            )
            .unwrap();
        });
        let results = remote_query(&mut a, &mut chan_a, "hello", &profile, 1).unwrap();
        server_thread.join().unwrap();
        assert!(results.len() <= 1);
    }

    #[test]
    fn tampered_ciphertext_fails_closed() {
        let (mut a, mut b) = mini_bearer::pair();
        let (mut chan_a, mut chan_b) = channels(&mut a, &mut b);
        let (_, _, _, _, profile) = fixture();

        send(
            &mut a,
            &mut chan_a,
            &Msg::QueryRequest {
                query: "hello".to_string(),
                profile,
                max_results: 8,
            },
        )
        .unwrap();
        // Tamper with the ciphertext before the peer opens it.
        let mut tampered = b.recv().unwrap();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        assert!(chan_b.open(&tampered, QUERY_AAD).is_err());
    }

    #[test]
    fn every_availability_variant_round_trips_through_the_wire_codec() {
        let cases = [
            AvailabilityState::Available,
            AvailabilityState::Unavailable(UnavailabilityReason::NotFetched),
            AvailabilityState::Unavailable(UnavailabilityReason::FetchFailed),
            AvailabilityState::Unavailable(UnavailabilityReason::Gone),
            AvailabilityState::Unavailable(UnavailabilityReason::UnsupportedContent),
            AvailabilityState::Restricted(RestrictionReason::RobotsExcluded),
            AvailabilityState::Restricted(RestrictionReason::LegalRestriction {
                jurisdiction: "eu".to_string(),
            }),
            AvailabilityState::Restricted(RestrictionReason::Malware),
            AvailabilityState::Restricted(RestrictionReason::Spam),
            AvailabilityState::Restricted(RestrictionReason::UserFilter),
            AvailabilityState::Restricted(RestrictionReason::SafetyWarning),
        ];
        for case in cases {
            let mut w = Writer::new();
            encode_availability(&mut w, &case);
            let bytes = w.finish();
            let mut r = Reader::new(&bytes);
            assert_eq!(decode_availability(&mut r).unwrap(), case);
            assert!(r.finished());
        }
    }
}

#!/usr/bin/env python3
"""Preserve F3 deterministic tie-breaks and fail closed on future wire enums."""
from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:160]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


query = "crates/mini-search-federation-net/src/query.rs"
replace_once(
    query,
    """//! does provide is that the query never crosses the wire in cleartext (the
//! channel's own AEAD covers it) and that the requester discloses no
//! identity of its own to run a query (CH1 needs none).
""",
    """//! does provide for the anonymous `remote_query` path is that the query
//! never crosses the wire in cleartext (the channel's own AEAD covers it)
//! and that the requester discloses no identity of its own (CH1 needs none).
//! The optional `remote_query_authenticated` path is mutual authentication:
//! it binds provider provenance but also discloses the requester's chosen
//! root or pairwise identity, as documented in the Phase 3 limits.
""",
)
replace_once(
    query,
    """        // `Scheme` is `#[non_exhaustive]` upstream; any future variant this
        // crate does not know about yet still round-trips as Https rather
        // than failing to encode at all.
        _ => w.u8(1),
""",
    """        // Protocol-level validation rejects unknown future variants before
        // this private encoder is reached. Keep a deterministic defensive
        // fallback for direct internal test construction only.
        _ => w.u8(1),
""",
)
replace_once(
    query,
    """                // `UnavailabilityReason` is `#[non_exhaustive]` upstream; a
                // future variant this crate does not know about yet still
                // round-trips as a generic \"unavailable, unspecified\" state
                // rather than failing to encode at all.
                _ => w.u8(255),
""",
    """                // Protocol-level validation rejects unknown future variants;
                // this private fallback exists only for defensive internal use.
                _ => w.u8(255),
""",
)
replace_once(
    query,
    """        // `AvailabilityState` is `#[non_exhaustive]` upstream; a future
        // top-level variant still round-trips as a generic restriction
        // rather than failing to encode.
        _ => {
""",
    """        // Protocol-level validation rejects unknown future variants;
        // this private fallback exists only for defensive internal use.
        _ => {
""",
)
replace_once(
    query,
    """        // `PersonalizationPolicy` is `#[non_exhaustive]` upstream; a future
        // variant still round-trips as `None` rather than failing to encode.
        _ => w.u8(0),
""",
    """        // Protocol-level validation rejects unknown future variants;
        // this private fallback exists only for defensive internal use.
        _ => w.u8(0),
""",
)
replace_once(
    query,
    """fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)?;
    profile.validate().map_err(|_| NetError::Protocol)
}

fn validate_url(url: &CanonicalUrl) -> Result<()> {
    if url.host.as_str().len() > MAX_HOST_BYTES
""",
    """fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)?;
    profile.validate().map_err(|_| NetError::Protocol)?;
    match profile.personalization {
        PersonalizationPolicy::None | PersonalizationPolicy::LocalUserControlled => Ok(()),
        _ => Err(NetError::Protocol),
    }
}

fn validate_url(url: &CanonicalUrl) -> Result<()> {
    match url.scheme {
        Scheme::Http | Scheme::Https => {}
        _ => return Err(NetError::Protocol),
    }
    if url.host.as_str().len() > MAX_HOST_BYTES
""",
)
replace_once(
    query,
    """fn validate_availability(availability: &AvailabilityState) -> Result<()> {
    if let AvailabilityState::Restricted(RestrictionReason::LegalRestriction { jurisdiction }) =
        availability
    {
        if jurisdiction.len() > MAX_JURISDICTION_BYTES {
            return Err(NetError::LimitExceeded);
        }
    }
    Ok(())
}
""",
    """fn validate_availability(availability: &AvailabilityState) -> Result<()> {
    match availability {
        AvailabilityState::Available => Ok(()),
        AvailabilityState::Unavailable(reason) => match reason {
            UnavailabilityReason::NotFetched
            | UnavailabilityReason::FetchFailed
            | UnavailabilityReason::Gone
            | UnavailabilityReason::UnsupportedContent => Ok(()),
            _ => Err(NetError::Protocol),
        },
        AvailabilityState::Restricted(reason) => match reason {
            RestrictionReason::LegalRestriction { jurisdiction } => {
                if jurisdiction.len() > MAX_JURISDICTION_BYTES {
                    Err(NetError::LimitExceeded)
                } else {
                    Ok(())
                }
            }
            RestrictionReason::RobotsExcluded
            | RestrictionReason::Malware
            | RestrictionReason::Spam
            | RestrictionReason::UserFilter
            | RestrictionReason::SafetyWarning => Ok(()),
            _ => Err(NetError::Protocol),
        },
        _ => Err(NetError::Protocol),
    }
}
""",
)
replace_once(
    query,
    """/// Derive a channel-scoped provider pseudonym from a sealed authenticated
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
""",
    """/// Derive a provider pseudonym from the endpoint proved on a sealed
/// authenticated connection. The helper is private, so callers cannot attach an
/// arbitrary peer label. The label deliberately excludes the CH1 binding:
/// `ProviderPseudonym` participates in F3's equal-score deduplication tie-break,
/// and channel randomness there would make identical source sets merge
/// differently on every connection and let a responder grind handshakes for a
/// favorable tie-break. `TransportEndpointId` already commits to the delegated
/// device/pairwise identity and current routing key and is disclosed by the
/// selected advertisement, so this adds no linkability beyond that endpoint.
/// Rotating the routing key, device, or pairwise identity rotates the label.
fn authenticated_provider_pseudonym<B: Bearer>(
    connection: &AuthenticatedConnection<B>,
) -> ProviderPseudonym {
    let mut transcript = Vec::with_capacity(AUTHENTICATED_PROVIDER_DOMAIN.len() + 32);
    transcript.extend_from_slice(AUTHENTICATED_PROVIDER_DOMAIN);
    transcript.extend_from_slice(&connection.peer().endpoint_id.to_bytes());
    ProviderPseudonym(Multihash::of(HashAlgorithm::Blake3, &transcript))
}
""",
)

integration = "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs"
replace_once(
    integration,
    """#[test]
fn a_peer_exchange_proof_cannot_be_reused_as_search_provider_provenance() {
""",
    """#[test]
fn one_authenticated_endpoint_has_one_provider_label_across_channels() {
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
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(32).unwrap();
        for _ in 0..2 {
            let (stream, _) = listener.accept().unwrap();
            let (bearer, channel) = responder_channel(TcpBearer::from_stream(stream).unwrap());
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
                segment_id.clone(),
                1_500,
            )
            .unwrap();
        }
    });

    let mut freshness = FreshnessPins::new();
    let mut replay = ReplayCache::new(32).unwrap();
    let mut labels = Vec::new();
    for _ in 0..2 {
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
        let remote = remote_query_authenticated(&mut connection, "hello", &profile, 8).unwrap();
        labels.push(remote.provider().clone());
    }
    assert_eq!(labels[0], labels[1]);
    server_thread.join().unwrap();
}

#[test]
fn a_peer_exchange_proof_cannot_be_reused_as_search_provider_provenance() {
""",
)

f6 = "docs/design/f6-private-query-transport.md"
replace_once(
    f6,
    """- The named query constructor internally domain-separates and hashes both the
  sealed connection's verified `TransportEndpointId` and exact
  CH1 binding. The label is stable for repeated queries on that connection but
  rotates across channels, preventing the named API from becoming a permanent
  cross-session tracking identifier.
""",
    """- The named query constructor privately domain-separates and hashes the sealed
  connection's verified `TransportEndpointId`. The label is stable across
  channels to the same advertised endpoint, preserving F3's deterministic
  equal-score provider tie-break and preventing responder handshake grinding.
  This creates no additional cross-session identifier beyond the endpoint id the
  caller already selected; routing-key, device, or pairwise-identity rotation
  rotates the label.
""",
)
replace_once(
    f6,
    """**Exact remaining failure:** endpoint-and-channel-bound provenance proves who
controlled one transport endpoint for one session, not that the provider's
index is honest or independently operated. Every new channel intentionally
changes the provider label, so durable reputation requires a separate,
privacy-conscious continuity design. The anonymous legacy path can still be
""",
    """**Exact remaining failure:** endpoint-bound provenance proves who controlled
one advertised transport endpoint, not that the provider's index is honest or
independently operated. The label is stable only while that endpoint id remains
stable; routing-key, device, or pairwise-identity rotation breaks continuity, so
durable reputation still requires a separate privacy-conscious design. The
anonymous legacy path can still be
""",
)
replace_once(
    f6,
    """- Decide whether a future privacy-preserving continuity proof should link rotating authenticated provider labels without turning one global provider identity into a tracking or ranking authority.
""",
    """- Decide whether a future privacy-preserving continuity proof should link authenticated provider labels across endpoint rotation without turning one global provider identity into a tracking or ranking authority.
""",
)
replace_once(
    f6,
    """- every current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason` variant round-trips through the `WireResult` codec, with a future-variant-safe fallback for both `#[non_exhaustive]` enums, mirroring F2b's own coverage discipline;
""",
    """- every current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason` variant round-trips through the internal codec, while any future unknown `#[non_exhaustive]` scheme, personalization, availability, or reason variant fails protocol validation instead of being silently reinterpreted;
""",
)

planning = "docs/planning/privacy-transport-runtime-convergence.md"
replace_once(
    planning,
    """`AuthenticatedConnection`. The provider pseudonym is domain-separated from the
verified `TransportEndpointId` and exact CH1 binding, so routing-key rotation or
opening a new channel rotates the label.
""",
    """`AuthenticatedConnection`. The provider pseudonym is privately derived from
its verified `TransportEndpointId`, stays stable across channels to that exact
endpoint so F3 tie-breaks remain deterministic, and rotates with the routing key,
device, or pairwise identity.
""",
)
replace_once(
    planning,
    """| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, channel-scoped endpoint+CH1 provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across sessions. |
""",
    """| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, endpoint-derived label stable across channels, and sealed merge path. Stability preserves F3's deterministic provider tie-break and removes responder handshake grinding. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across endpoint rotation. |
""",
)
replace_once(
    planning,
    """  Provider labels intentionally rotate across channels; privacy-preserving
  durable continuity remains undesigned. The named exchange is mutual, so there
""",
    """  Provider labels remain stable for one advertised endpoint but rotate with
  its routing key, device, or pairwise identity; continuity across that rotation
  remains undesigned. The named exchange is mutual, so there
""",
)

threat = "docs/THREAT_MODEL.md"
replace_once(
    threat,
    """| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, channel-scoped endpoint+CH1 provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth or cross-session continuity. |
""",
    """| **Forged or handshake-ground F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, private `AuthenticatedQueryResults`, sealed merge, and a label derived only from the verified advertised endpoint—not responder-controlled CH1 randomness. | **Closed for the named API and same-endpoint deterministic tie-break.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth or continuity across endpoint rotation. |
""",
)

status = "docs/STATUS.md"
replace_once(
    status,
    """- **implemented in draft PR #296** — optional named F6 search uses the distinct
  `SearchQuery` purpose, derives a channel-scoped provider pseudonym from the
  endpoint and exact CH1 binding proved on the response channel, and seals the authenticated merge input behind
  private fields. Anonymous search and caller-labeled legacy merge remain
""",
    """- **implemented in draft PR #296** — optional named F6 search uses the distinct
  `SearchQuery` purpose, derives a provider pseudonym from the verified advertised
  endpoint, keeps it stable across channels so F3 tie-breaks remain deterministic,
  and seals the authenticated merge input behind private fields. Anonymous search
  and caller-labeled legacy merge remain
""",
)

log = "docs/DECISION_LOG.md"
replace_once(
    log,
    """`AuthenticatedConnection`, derives a rotating `ProviderPseudonym` from the
verified `TransportEndpointId` plus exact CH1 binding, returns an
`AuthenticatedQueryResults` with
""",
    """`AuthenticatedConnection`, privately derives a `ProviderPseudonym` from the
verified `TransportEndpointId`, returns an `AuthenticatedQueryResults` with
""",
)
replace_once(
    log,
    """onion chain; sealed channel-scoped search-provider provenance; symmetric F6 wire bounds, profile attribution,
""",
    """onion chain; sealed endpoint-stable search-provider provenance that preserves
F3 deterministic tie-breaks and removes channel-handshake grinding; symmetric F6 wire bounds, profile attribution,
""",
)
replace_once(
    log,
    """or result truth. Every authenticated F6 connection receives a channel-scoped provider label;
privacy-preserving durable continuity across sessions is intentionally undesigned.
""",
    """or result truth. An authenticated F6 label is stable for one advertised
endpoint but rotates with its routing key, device, or pairwise identity;
privacy-preserving continuity across endpoint rotation is intentionally undesigned.
""",
)

print("PR 296 stable provider and fail-closed enum hardening applied")

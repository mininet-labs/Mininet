#!/usr/bin/env python3
"""Close the final PR #296 review gaps: visible-identity selection diversity and symmetric F6 wire bounds."""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    write(path, text.replace(old, new))


def replace_between(path: str, start_marker: str, end_marker: str, replacement: str) -> None:
    text = read(path)
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"{path}: missing start marker {start_marker!r}")
    end = text.find(end_marker, start)
    if end < 0:
        raise SystemExit(f"{path}: missing end marker {end_marker!r}")
    if text.find(start_marker, start + 1) >= 0:
        raise SystemExit(f"{path}: start marker is not unique {start_marker!r}")
    write(path, text[:start] + replacement + text[end:])


# ---------------------------------------------------------------------------
# 1. Peer selection: one visible root/device may not occupy several slots.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-transport-security/src/selection.rs",
    "//! Locally seeded, bounded, prefix-diverse peer selection.\n",
    "//! Locally seeded, bounded, visible-identity- and prefix-diverse peer selection.\n",
)
replace_once(
    "crates/mini-transport-security/src/selection.rs",
    """/// Build a bounded dial order. Records must already have passed signature and
/// KEL verification. Duplicate endpoint ids and concentrated network prefixes
/// are skipped; no majority or peer vote is consulted.
""",
    """/// Build a bounded dial order. Records must already have passed signature and
/// KEL verification. Duplicate endpoint ids, routing keys, visible roots, visible
/// devices, and concentrated network prefixes are skipped; no majority or peer
/// vote is consulted. Visible identity diversity raises eclipse cost but does not
/// prove independent operators: one adversary can still control many pairwise
/// roots or apparently unrelated network prefixes.
""",
)
replace_once(
    "crates/mini-transport-security/src/selection.rs",
    """    let mut selected = Vec::with_capacity(policy.max_peers);
    let mut endpoints = HashSet::new();
    let mut routing_keys = HashSet::new();
    let mut prefix_counts: HashMap<NetworkPrefix, usize> = HashMap::new();
    for (_, record) in candidates {
        if selected.len() >= policy.max_peers {
            break;
        }
        if !endpoints.insert(record.endpoint_id()) || !routing_keys.insert(record.routing_key()) {
            continue;
        }
        let prefix = NetworkPrefix::from_ip(record.address().ip());
        let count = prefix_counts.entry(prefix).or_default();
        if *count >= policy.max_per_network_prefix {
            continue;
        }
        *count += 1;
        selected.push(DialAttempt {
            endpoint_id: record.endpoint_id(),
            address: record.address(),
            routing_key: record.routing_key(),
            timeout_ms: policy.dial_timeout_ms,
        });
    }
""",
    """    let mut selected = Vec::with_capacity(policy.max_peers);
    let mut endpoints = HashSet::new();
    let mut routing_keys = HashSet::new();
    let mut roots = HashSet::new();
    let mut devices = HashSet::new();
    let mut prefix_counts: HashMap<NetworkPrefix, usize> = HashMap::new();
    for (_, record) in candidates {
        if selected.len() >= policy.max_peers {
            break;
        }
        if endpoints.contains(&record.endpoint_id())
            || routing_keys.contains(&record.routing_key())
            || roots.contains(record.root())
            || devices.contains(record.device())
        {
            continue;
        }
        let prefix = NetworkPrefix::from_ip(record.address().ip());
        let count = prefix_counts.entry(prefix).or_default();
        if *count >= policy.max_per_network_prefix {
            continue;
        }
        *count += 1;
        endpoints.insert(record.endpoint_id());
        routing_keys.insert(record.routing_key());
        roots.insert(record.root().clone());
        devices.insert(record.device().clone());
        selected.push(DialAttempt {
            endpoint_id: record.endpoint_id(),
            address: record.address(),
            routing_key: record.routing_key(),
            timeout_ms: policy.dial_timeout_ms,
        });
    }
""",
)
replace_once(
    "crates/mini-transport-security/src/selection.rs",
    """    #[test]
    fn candidate_input_is_bounded_before_sorting() {
""",
    """    #[test]
    fn one_visible_identity_cannot_fill_multiple_selection_slots() {
        let mut root = Controller::incept_single_from_seeds(&[60; 32], &[61; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[62; 32],
            &[63; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();

        let make_record = |routing_seed: u8, address: &str| {
            let routing = AgreementSecretKey::from_seed(&[routing_seed; 32]).public_key();
            let advertisement = PeerAdvertisement::issue(
                [7; 32],
                &root.did(),
                &device,
                routing,
                address.parse().unwrap(),
                1_000,
                2_000,
            )
            .unwrap();
            let mut freshness = FreshnessPins::new();
            let mut replay = ReplayCache::new(8).unwrap();
            advertisement
                .verify(
                    [7; 32],
                    1_500,
                    &root.kel(),
                    &device.kel(),
                    &mut freshness,
                    &mut replay,
                )
                .unwrap()
        };

        let same_identity_a = make_record(64, "10.0.0.1:9000");
        let same_identity_b = make_record(65, "10.0.1.1:9000");
        let independent = verified(80, "10.0.2.1:9000");
        let policy = PeerSelectionPolicy {
            max_peers: 3,
            max_per_network_prefix: 3,
            dial_timeout_ms: 1_000,
        };
        let plan = diverse_dial_plan(
            &[
                same_identity_a.clone(),
                same_identity_b.clone(),
                independent,
            ],
            [9; 32],
            policy,
        )
        .unwrap();

        assert_eq!(plan.len(), 2);
        let same_identity_slots = plan
            .iter()
            .filter(|attempt| {
                attempt.endpoint_id == same_identity_a.endpoint_id()
                    || attempt.endpoint_id == same_identity_b.endpoint_id()
            })
            .count();
        assert_eq!(same_identity_slots, 1);
    }

    #[test]
    fn candidate_input_is_bounded_before_sorting() {
""",
)

# ---------------------------------------------------------------------------
# 2. F6 wire codec: validate outbound values before encoding and re-check all
#    decoded values, including score ranges and requested ranking profile.
# ---------------------------------------------------------------------------
query_path = "crates/mini-search-federation-net/src/query.rs"
replace_once(
    query_path,
    """fn encode_result(w: &mut Writer, r: &WireResult) {
""",
    """fn validate_multihash(value: &Multihash) -> Result<()> {
    if value.to_bytes().len() > MAX_MULTIHASH_BYTES {
        return Err(NetError::LimitExceeded);
    }
    Ok(())
}

fn validate_profile(profile: &RankingProfile) -> Result<()> {
    validate_multihash(&profile.id.0)
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
    Ok(())
}

fn validate_availability(availability: &AvailabilityState) -> Result<()> {
    if let AvailabilityState::Restricted(RestrictionReason::LegalRestriction {
        jurisdiction,
    }) = availability
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
    if max_results == 0
        || max_results > MAX_QUERY_RESULTS
        || results.len() > max_results as usize
    {
        return Err(NetError::LimitExceeded);
    }
    for result in results {
        validate_wire_result(result)?;
        if &result.ranking_profile != &requested_profile.id {
            return Err(NetError::Protocol);
        }
    }
    Ok(())
}

fn encode_result(w: &mut Writer, r: &WireResult) {
""",
)

new_impl = r'''impl Msg {
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
'''
replace_between(query_path, "impl Msg {\n", "\nfn send(", new_impl)
replace_once(
    query_path,
    """fn send(bearer: &mut dyn Bearer, chan: &mut Channel, msg: &Msg) -> Result<()> {
    let ct = chan.seal(&msg.encode(), QUERY_AAD)?;
""",
    """fn send(bearer: &mut dyn Bearer, chan: &mut Channel, msg: &Msg) -> Result<()> {
    let ct = chan.seal(&msg.encode()?, QUERY_AAD)?;
""",
)
replace_once(
    query_path,
    """        Msg::QueryResponse { results } => {
            if results.len() > max_results as usize {
                return Err(NetError::LimitExceeded);
            }
            Ok(results)
        }
""",
    """        Msg::QueryResponse { results } => {
            validate_query_response(&results, profile, max_results)?;
            Ok(results)
        }
""",
)
replace_once(
    query_path,
    """    connection.send(&request.encode(), QUERY_AAD)?;
""",
    """    connection.send(&request.encode()?, QUERY_AAD)?;
""",
)
replace_once(
    query_path,
    """    let results = match response {
        Msg::QueryResponse { results } => {
            if results.len() > max_results as usize {
                return Err(NetError::LimitExceeded);
            }
            results
        }
        _ => return Err(NetError::Protocol),
    };
""",
    """    let results = match response {
        Msg::QueryResponse { results } => {
            validate_query_response(&results, profile, max_results)?;
            results
        }
        _ => return Err(NetError::Protocol),
    };
""",
)
replace_once(
    query_path,
    """    connection.send(&Msg::QueryResponse { results }.encode(), QUERY_AAD)?;
""",
    """    let response = Msg::QueryResponse { results };
    connection.send(&response.encode()?, QUERY_AAD)?;
""",
)
replace_once(
    query_path,
    """    #[test]
    fn a_compliant_server_never_exceeds_the_requested_max_results() {
""",
    """    #[test]
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
""",
)

# The merge conversion remains a defense-in-depth boundary for public, locally
# constructed WireResult values even though the network decoder now rejects the
# same invalid scores immediately.
replace_once(
    "crates/mini-search-federation-net/src/remote_merge.rs",
    """/// only ever emits validated [`WeightBps`]. `WireResult`'s wire codec does
/// not itself bound these fields (unlike e.g. `RankingProfile`'s decoded
/// weights), so this is the real fail-closed check for a wire peer that
/// sends an out-of-range score.
""",
    """/// only ever emits validated [`WeightBps`]. The F6 wire decoder also
/// rejects these values; this conversion deliberately repeats the check because
/// [`WireResult`] is public and can be constructed locally without passing
/// through the decoder. Invalid local or legacy inputs therefore still fail
/// closed before entering the typed federated merge.
""",
)

# ---------------------------------------------------------------------------
# 3. Truth sync for the two hardened boundaries.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-transport-security/README.md",
    """- `diverse_dial_plan` is locally seeded, input-order-independent, duplicate-
  resistant, capped per IPv4 `/24` or IPv6 `/48` prefix, and rejects more than
  1,024 candidates before allocation/sort.
""",
    """- `diverse_dial_plan` is locally seeded, input-order-independent, rejects
  repeated endpoint ids, routing keys, visible roots, and visible devices, caps
  IPv4 `/24` and IPv6 `/48` concentration, and rejects more than 1,024 candidates
  before allocation/sort.
""",
)
replace_once(
    "docs/THREAT_MODEL.md",
    """| **Bootstrap eclipse** | Caller-local seeded ordering, endpoint/routing-key deduplication, bounded retry/timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can acquire diverse prefixes/ASNs or control all discovery sources; address diversity is not operator independence. |
""",
    """| **Bootstrap eclipse** | Caller-local seeded ordering, endpoint/routing-key/root/device deduplication, bounded retry/timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can create many pairwise roots, acquire diverse prefixes/ASNs, or control all discovery sources; visible identity/address diversity is not operator independence. |
""",
)
replace_once(
    "docs/THREAT_MODEL.md",
    """| **Purpose confusion in provider provenance** | `SearchQuery` is a distinct signed purpose, and authenticated query APIs reject any other purpose. | **Closed for the typed API.** A provider still sees the full query and may log or manipulate results. |
""",
    """| **Purpose confusion in provider provenance** | `SearchQuery` is a distinct signed purpose, and authenticated query APIs reject any other purpose. | **Closed for the typed API.** A provider still sees the full query and may log or manipulate results. |
| **F6 outbound/decode bound asymmetry or profile substitution** | Every request/response is validated before encoding and after decoding; response fields, score components, result counts, and multihashes share one bound set, and clients require every result to name the profile they requested. | **Closed for F6 framing and profile attribution.** Provider honesty and query-content privacy remain unsolved. |
""",
)
replace_once(
    "docs/design/f6-private-query-transport.md",
    """- a compliant server never returns more results than the request's own `max_results`;
- every current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason` variant round-trips through the `WireResult` codec, with a future-variant-safe fallback for both `#[non_exhaustive]` enums, mirroring F2b's own coverage discipline;
""",
    """- a compliant server never returns more results than the request's own `max_results`;
- the same field, score, count, URL, jurisdiction, and multihash bounds run before encoding and after decoding, so a provider cannot emit a message its own peer would reject solely because local metadata was oversized;
- a response is rejected if any result names a ranking profile other than the one requested, preserving F3's same-profile score-comparability premise;
- every current `AvailabilityState`/`RestrictionReason`/`UnavailabilityReason` variant round-trips through the `WireResult` codec, with a future-variant-safe fallback for both `#[non_exhaustive]` enums, mirroring F2b's own coverage discipline;
""",
)
replace_once(
    "docs/design/f6-private-query-transport.md",
    """It rejects (`NetError::Protocol`) any `relevance_score_bps` or `explanation` component above `WeightBps::MAX` — a value a compliant `serve_query` can never produce (`mini_query::search` only ever emits validated `WeightBps`), but which `WireResult`'s own wire codec does not itself bound on decode, so this is the real fail-closed check against a peer that sends an out-of-range score.
""",
    """It rejects (`NetError::Protocol`) any `relevance_score_bps` or `explanation` component above `WeightBps::MAX` — a value a compliant `serve_query` can never produce (`mini_query::search` only ever emits validated `WeightBps`). The F6 decoder now rejects the same values; the conversion repeats the check because public `WireResult` values may also be constructed locally or arrive through legacy code without traversing that decoder.
""",
)
replace_once(
    "docs/DECISION_LOG.md",
    """`explanation` component above `WeightBps::MAX` — a value a compliant
`serve_query` can never produce, since `mini_query::search` only ever
emits validated `WeightBps`, but `WireResult`'s own wire codec does not
itself bound these fields on decode) and `merge_remote_results(local:
""",
    """`explanation` component above `WeightBps::MAX` — a value a compliant
`serve_query` can never produce, since `mini_query::search` only ever
emits validated `WeightBps`; the F6 wire decoder now rejects the same
values, while this conversion repeats the check for public `WireResult`
values constructed locally or received through legacy code) and
`merge_remote_results(local:
""",
)
replace_once(
    "docs/DECISION_LOG.md",
    """`AvailabilityState`/`RestrictionReason`/`UnavailabilityReason`/`Scheme`/
`PersonalizationPolicy` variant round-trips with a future-variant-safe
fallback for each `#[non_exhaustive]` enum; a tampered ciphertext fails
closed) plus one new real-socket integration test
""",
    """`AvailabilityState`/`RestrictionReason`/`UnavailabilityReason`/`Scheme`/
`PersonalizationPolicy` variant round-trips with a future-variant-safe
fallback for each `#[non_exhaustive]` enum; outbound and inbound field/score
bounds are symmetric; profile substitution is rejected; a tampered ciphertext
fails closed) plus one new real-socket integration test
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """| Central naming/bridge authority avoidance | **PASS** | Caller-held KELs, self-certifying endpoints, local selection, and reuse of the existing pluggable-transport boundary; no CA, canonical list, or bridge directory. | First-contact unseen KEL revocation still needs witness/gossip evidence. |
""",
    """| Central naming/bridge authority avoidance | **PASS** | Caller-held KELs, self-certifying endpoints, locally seeded selection that deduplicates visible roots/devices as well as endpoint/routing keys, and reuse of the existing pluggable-transport boundary; no CA, canonical list, or bridge directory. | First-contact unseen KEL revocation still needs witness/gossip evidence; pairwise roots do not prove independent operators. |
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """- `mini-search-federation-net` strict Clippy and all focused tests pass,
  including a real authenticated F6 query/merge and wrong-purpose rejection.
""",
    """- `mini-search-federation-net` strict Clippy and all focused tests pass,
  including a real authenticated F6 query/merge, wrong-purpose rejection,
  symmetric outbound/decode bounds, and requested-profile enforcement.
""",
)

print("PR 296 final review hardening applied")

#!/usr/bin/env python3
"""Seal the last fail-closed protocol boundaries in PR #296."""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:140]!r}")
    write(path, text.replace(old, new))


# ---------------------------------------------------------------------------
# 1. Onion construction/encoding: destination key must not be any relay key,
#    and public mutable packet fields may not encode a noncanonical packet.
# ---------------------------------------------------------------------------
onion = "crates/mini-relay/src/onion.rs"
replace_once(
    onion,
    """    validate_route(hops)?;
    validate_onion_window(now_ms, expires_at_ms)?;
""",
    """    validate_route(hops)?;
    if hops
        .iter()
        .any(|hop| hop.routing_key == destination_key)
    {
        return Err(RelayError::InvalidOnionRoute);
    }
    validate_onion_window(now_ms, expires_at_ms)?;
""",
)
replace_once(
    onion,
    """    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.u8(ONION_VERSION);
        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u8(self.hop_index);
""",
    """    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.hop_index as usize >= ONION_HOP_COUNT
            || self.ciphertext.len() != onion_ciphertext_bytes(self.size_class, self.hop_index)?
        {
            return Err(RelayError::InvalidOnionRoute);
        }
        let mut writer = Writer::new();
        writer.u8(ONION_VERSION);
        writer.raw(&self.connection_id.to_bytes());
        writer.u8(size_class_tag(self.size_class));
        writer.u8(self.hop_index);
""",
)
replace_once(
    onion,
    """        let (hops, _) = route();
        let oversized = vec![0u8; SMALL_ONION_PAYLOAD_BYTES];
""",
    """        let (hops, _) = route();
        assert_eq!(
            build_onion(
                ConnectionId::from_bytes([7; 16]),
                PayloadSizeClass::Small,
                &hops,
                hops[2].routing_key,
                b"payload",
                BUILD_NOW_MS,
                EXPIRES_AT_MS,
            ),
            Err(RelayError::InvalidOnionRoute)
        );
        let oversized = vec![0u8; SMALL_ONION_PAYLOAD_BYTES];
""",
)
replace_once(
    onion,
    """        let mut shorter = packet.clone();
        shorter.ciphertext.pop().unwrap();
        assert_eq!(
            OnionPacket::from_bytes(&shorter.to_bytes().unwrap()),
            Err(RelayError::InvalidOnionRoute)
        );

        let mut longer = packet;
        longer.ciphertext.push(0);
        assert_eq!(
            OnionPacket::from_bytes(&longer.to_bytes().unwrap()),
            Err(RelayError::LimitExceeded)
        );
""",
    """        let mut shorter = packet.clone();
        shorter.ciphertext.pop().unwrap();
        assert_eq!(shorter.to_bytes(), Err(RelayError::InvalidOnionRoute));

        let mut longer = packet.clone();
        longer.ciphertext.push(0);
        assert_eq!(longer.to_bytes(), Err(RelayError::InvalidOnionRoute));

        let mut wrong_hop = packet;
        wrong_hop.hop_index = ONION_HOP_COUNT as u8;
        assert_eq!(wrong_hop.to_bytes(), Err(RelayError::InvalidOnionRoute));
""",
)

# ---------------------------------------------------------------------------
# 2. Public peer selection must not silently combine independent networks.
# ---------------------------------------------------------------------------
selection = "crates/mini-transport-security/src/selection.rs"
replace_once(
    selection,
    """    if records.len() > MAX_SELECTION_CANDIDATES {
        return Err(TransportSecurityError::LimitExceeded);
    }
    let mut candidates: Vec<_> = records
""",
    """    if records.len() > MAX_SELECTION_CANDIDATES {
        return Err(TransportSecurityError::LimitExceeded);
    }
    if let Some(expected_network) = records.first().map(VerifiedPeerAdvertisement::network_id) {
        if records
            .iter()
            .any(|record| record.network_id() != expected_network)
        {
            return Err(TransportSecurityError::WrongNetwork);
        }
    }
    let mut candidates: Vec<_> = records
""",
)
replace_once(
    selection,
    """    #[test]
    fn candidate_input_is_bounded_before_sorting() {
""",
    """    #[test]
    fn selection_rejects_mixed_network_records() {
        let local = verified(10, "10.0.0.1:9000");
        let mut foreign_root =
            Controller::incept_single_from_seeds(&[90; 32], &[91; 32]).unwrap();
        let foreign_device = Controller::incept_device_single_from_seeds(
            &foreign_root.did(),
            &[92; 32],
            &[93; 32],
        )
        .unwrap();
        foreign_root
            .delegate_device(&foreign_device.did(), Capabilities::primary())
            .unwrap();
        let foreign_routing = AgreementSecretKey::from_seed(&[94; 32]).public_key();
        let foreign = PeerAdvertisement::issue(
            [8; 32],
            &foreign_root.did(),
            &foreign_device,
            foreign_routing,
            "10.0.1.1:9000".parse().unwrap(),
            1_000,
            2_000,
        )
        .unwrap();
        let mut freshness = FreshnessPins::new();
        let mut replay = ReplayCache::new(8).unwrap();
        let foreign = foreign
            .verify(
                [8; 32],
                1_500,
                &foreign_root.kel(),
                &foreign_device.kel(),
                &mut freshness,
                &mut replay,
            )
            .unwrap();

        assert_eq!(
            diverse_dial_plan(
                &[local, foreign],
                [1; 32],
                PeerSelectionPolicy::default(),
            ),
            Err(TransportSecurityError::WrongNetwork)
        );
    }

    #[test]
    fn candidate_input_is_bounded_before_sorting() {
""",
)

# ---------------------------------------------------------------------------
# 3. Secure PEX outer and inner network identifiers must agree structurally.
# ---------------------------------------------------------------------------
advertisement = "crates/mini-transport-security/src/advertisement.rs"
replace_once(
    advertisement,
    """    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.advertisements.len() > MAX_SECURE_PEX_RECORDS {
            return Err(TransportSecurityError::LimitExceeded);
        }
        let mut writer = Writer::new();
""",
    """    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.advertisements.len() > MAX_SECURE_PEX_RECORDS {
            return Err(TransportSecurityError::LimitExceeded);
        }
        if self
            .advertisements
            .iter()
            .any(|advertisement| advertisement.network_id != self.network_id)
        {
            return Err(TransportSecurityError::WrongNetwork);
        }
        let mut writer = Writer::new();
""",
)
replace_once(
    advertisement,
    """        let bytes = response.to_bytes().unwrap();
        assert_eq!(SecurePexResponse::from_bytes(&bytes).unwrap(), response);
        let mut trailing = bytes;
""",
    """        let bytes = response.to_bytes().unwrap();
        assert_eq!(SecurePexResponse::from_bytes(&bytes).unwrap(), response);
        let mismatched = SecurePexResponse {
            network_id: [8; 32],
            advertisements: response.advertisements.clone(),
        };
        assert_eq!(
            mismatched.to_bytes(),
            Err(TransportSecurityError::WrongNetwork)
        );
        let mut trailing = bytes;
""",
)

# ---------------------------------------------------------------------------
# 4. F6 ranked responses must preserve the ranker's displayability invariant.
# ---------------------------------------------------------------------------
query = "crates/mini-search-federation-net/src/query.rs"
replace_once(
    query,
    """    validate_url(&result.url)?;
    validate_availability(&result.availability)?;
    validate_multihash(&result.ranking_profile.0)?;
""",
    """    validate_url(&result.url)?;
    validate_availability(&result.availability)?;
    if !result.availability.is_displayable() {
        return Err(NetError::Protocol);
    }
    validate_multihash(&result.ranking_profile.0)?;
""",
)
replace_once(
    query,
    """    #[test]
    fn a_response_for_another_ranking_profile_is_rejected() {
""",
    """    #[test]
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
""",
)

# ---------------------------------------------------------------------------
# 5. Truth sync. No new authority or anonymity claims are added.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-transport-security/README.md",
    """- `SecurePexResponse` carries a bounded canonical list of signed advertisements.
""",
    """- `SecurePexResponse` carries a bounded canonical list of signed advertisements
  and rejects any record whose network id differs from its outer response.
""",
)
replace_once(
    "crates/mini-transport-security/README.md",
    """- `build_verified_onion_route` accepts three live same-network verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building
""",
    """- `build_verified_onion_route` accepts three live same-network verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building;
  the lower onion constructor also rejects using any relay routing key as the
  destination key, so no relay can become the destination by caller mistake,
""",
)
replace_once(
    "crates/mini-transport-security/README.md",
    """  the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`. A permanent
  integration tests start with signed advertisements and local selection, then
""",
    """  then builds the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`.
  Permanent integration tests start with signed advertisements and local selection, then
""",
)

threat = "docs/THREAT_MODEL.md"
replace_once(
    threat,
    """| **Bootstrap eclipse** | Caller-local seeded ordering, endpoint/routing-key/root/device deduplication, bounded retry/timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can create many pairwise roots, acquire diverse prefixes/ASNs, or control all discovery sources; visible identity/address diversity is not operator independence. |
""",
    """| **Bootstrap eclipse** | Caller-local seeded ordering, same-network enforcement, endpoint/routing-key/root/device deduplication, bounded retry/timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can create many pairwise roots, acquire diverse prefixes/ASNs, or control all discovery sources; visible identity/address diversity is not operator independence. |
""",
)
replace_once(
    threat,
    """| **One visible endpoint assigned multiple onion roles** | `build_verified_onion_route` rechecks live same-network advertisements and rejects endpoint-id, routing-key, visible-root, or device reuse before Entry/Rendezvous/Delivery construction. | **Partial.** One hidden operator can control several pairwise roots, devices, addresses, or ASNs. |
""",
    """| **One visible endpoint assigned multiple onion roles or used as destination** | `build_verified_onion_route` rechecks live same-network advertisements and rejects endpoint-id, routing-key, visible-root, or device reuse; `build_onion` also rejects a destination key equal to any relay key. | **Partial.** One hidden operator can control several pairwise roots, devices, addresses, or ASNs. |
""",
)
replace_once(
    threat,
    """| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption. | **Closed for explicit circuit ids.** Timing/volume correlation remains open. |
""",
    """| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination connection id appears only after the delivery layer is peeled and differs from every public hop id. | **Closed for one shared clear circuit id.** Timing/volume correlation remains open. |
""",
)
replace_once(
    threat,
    """| **Declared onion size-class bypass** | Onion v2 derives the exact ciphertext length for each hop and payload class and rejects both shorter and longer canonical frames before decryption. | **Closed for packet framing.** Timing and coarse class choice remain visible by design. |
""",
    """| **Declared onion size-class bypass or invalid public packet encoding** | Onion v2 derives the exact ciphertext length for each hop and payload class; both decoding and public `to_bytes` reject wrong hop indices and shorter/longer bodies before transport. | **Closed for packet framing.** Timing and coarse class choice remain visible by design. |
""",
)
replace_once(
    threat,
    """| **F6 outbound/decode bound asymmetry or profile substitution** | Every request/response is validated before encoding and after decoding; response fields, score components, result counts, and multihashes share one bound set, and clients require every result to name the profile they requested. | **Closed for F6 framing and profile attribution.** Provider honesty and query-content privacy remain unsolved. |
""",
    """| **F6 outbound/decode bound asymmetry, profile substitution, or filtered-result reinsertion** | Every request/response is validated before encoding and after decoding; response fields, score components, result counts, and multihashes share one bound set, clients require the requested profile, and ranked responses reject every non-displayable availability state. | **Closed for F6 framing, profile attribution, and ranker-filter preservation.** Provider honesty and query-content privacy remain unsolved. |
""",
)
replace_once(
    threat,
    """| **Unsigned discovery redirect** | Signed, expiring, network-bound `PeerAdvertisement`; `connect_authenticated_tcp` establishes CH1 and invokes `verify_advertised` before returning a connection. | **Closed for the runtime path.** Legacy `mini-net::pex` and specialist callers composing lower-level APIs directly retain caller-owned redirect risk. |
""",
    """| **Unsigned discovery redirect** | Signed, expiring, network-bound `PeerAdvertisement`; `SecurePexResponse` rejects mixed-network contents; `connect_authenticated_tcp` establishes CH1 and invokes `verify_advertised` before returning a connection. | **Closed for the runtime path.** Legacy `mini-net::pex` and specialist callers composing lower-level APIs directly retain caller-owned redirect risk. |
""",
)

replace_once(
    "docs/design/f6-private-query-transport.md",
    """- a response is rejected if any result names a ranking profile other than the one requested, preserving F3's same-profile score-comparability premise;
""",
    """- a response is rejected if any result names a ranking profile other than the one requested, preserving F3's same-profile score-comparability premise;
- a ranked response rejects `Restricted` and `Unavailable` results, so a malicious provider cannot reinsert a document that the ranker structurally excludes before scoring;
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
""",
    """| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device; the destination key must differ from every relay routing key. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """  including a real authenticated F6 query/merge, wrong-purpose rejection,
  symmetric outbound/decode bounds, and requested-profile enforcement.
""",
    """  including a real authenticated F6 query/merge, wrong-purpose rejection,
  symmetric outbound/decode bounds, requested-profile enforcement, and rejection
  of non-displayable ranked results.
""",
)
replace_once(
    "docs/planning/privacy-transport-runtime-convergence.md",
    """  rejection, malformed-state atomicity, and exact ciphertext length for every
  declared payload size class and hop.
""",
    """  rejection, malformed-state atomicity, exact ciphertext length for every
  declared payload size class and hop, outbound packet-state validation, and
  destination/relay key separation.
""",
)

# Keep D-0438's composition claim accurate: the wrapper adds no second crypto
# construction, but this PR did harden mini-relay itself.
replace_once(
    "docs/DECISION_LOG.md",
    """uses the unchanged `mini-relay::build_onion` implementation.
""",
    """delegates cryptography to `mini-relay::build_onion` rather than adding a
second onion construction; this PR also hardens that implementation's replay,
size, and relay/destination-key boundaries.
""",
)
replace_once(
    "docs/DECISION_LOG.md",
    """expiry/network rechecks; bounded selection input; permanent connection poisoning
on ambiguous bearer/channel failure; authenticated CH1 on every socket in a full
""",
    """expiry/network rechecks; same-network and visible-identity-diverse selection;
secure-PEX outer/inner network consistency; destination/relay key separation;
exact inbound/outbound onion sizing; bounded selection input; permanent connection
poisoning on ambiguous bearer/channel failure; authenticated CH1 on every socket in a full
""",
)
replace_once(
    "docs/DECISION_LOG.md",
    """search-provider provenance; and wrong-purpose rejection. Focused
""",
    """search-provider provenance; symmetric F6 wire bounds, profile attribution,
ranker-filter preservation; and wrong-purpose rejection. Focused
""",
)

print("PR 296 final protocol-boundary hardening applied")

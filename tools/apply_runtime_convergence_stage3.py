#!/usr/bin/env python3
"""Seal authenticated search provenance and truth-sync PR #296."""

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
        raise SystemExit(
            f"{path}: expected {expected} matches, found {count}: {old[:120]!r}"
        )
    write(path, text.replace(old, new))


def replace_from_marker(path: str, marker: str, replacement: str) -> None:
    text = read(path)
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{path}: expected one marker, found {count}: {marker!r}")
    prefix = text[: text.index(marker)]
    write(path, prefix + replacement)


query = "crates/mini-search-federation-net/src/query.rs"
remote_merge = "crates/mini-search-federation-net/src/remote_merge.rs"
query_test = "crates/mini-search-federation-net/tests/authenticated_query_over_tcp.rs"
search_lib = "crates/mini-search-federation-net/src/lib.rs"
search_cargo = "crates/mini-search-federation-net/Cargo.toml"
transport_readme = "crates/mini-transport-security/README.md"
f6_design = "docs/design/f6-private-query-transport.md"
decision_log = "docs/DECISION_LOG.md"
status = "docs/STATUS.md"
threat_model = "docs/THREAT_MODEL.md"
planning = "docs/planning/privacy-transport-runtime-convergence.md"

# The authenticated merge API must not accept a struct literal carrying a
# caller-chosen provider pseudonym. Keep construction inside query.rs and expose
# read-only access plus a crate-private consuming seam for remote_merge.rs.
replace_exact(
    query,
    """#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedQueryResults {
    pub provider: ProviderPseudonym,
    pub results: Vec<WireResult>,
}
""",
    """#[derive(Debug, Clone, PartialEq, Eq)]
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
""",
)
replace_exact(
    remote_merge,
    """    merge_remote_results(local, remote.results, remote.provider, max_results)
""",
    """    let (provider, results) = remote.into_parts();
    merge_remote_results(local, results, provider, max_results)
""",
)
replace_exact(
    query_test,
    """    assert_eq!(remote.provider, expected_provider);
    assert_eq!(remote.results.len(), 1);
""",
    """    assert_eq!(remote.provider(), &expected_provider);
    assert_eq!(remote.results().len(), 1);
""",
)

replace_exact(
    search_lib,
    """//! 2. [`remote_query`]/[`serve_query`] (Track F6 Phase 1, [`query`]) are
//!    the deliberate, separately-scoped exception: a bounded query string
//!    *does* cross the wire here, confidential-in-transit only — the
//!    queried peer sees it in full. This is not a private-information-
//!    retrieval scheme; see `docs/design/f6-private-query-transport.md`
//!    for the full doctrine on what this is and is not.
""",
    """//! 2. [`remote_query`]/[`serve_query`] (Track F6 Phase 1, [`query`]) are
//!    the deliberate, separately-scoped anonymous exception: a bounded query
//!    string *does* cross the wire, confidential-in-transit only, and the
//!    queried peer sees it in full. The optional named path,
//!    [`remote_query_authenticated`]/[`serve_query_authenticated`], requires an
//!    exact `SearchQuery`-purpose [`mini_transport_security::AuthenticatedConnection`]
//!    and derives merged provider provenance from that live peer rather than a
//!    caller-selected label. Neither path is private information retrieval.
""",
)
replace_exact(
    search_lib,
    """//! - No peer discovery, connection setup, or bootstrap. Callers dial and
//!   handshake exactly as any other `mini_bearer`/`mini_sync` caller does;
//!   this crate only ever takes an already-established `Bearer`/`Channel`.
""",
    """//! - No peer discovery, bootstrap authority, or scheduler. Anonymous APIs take
//!   an already-established `Bearer`/`Channel`; named APIs take an already-
//!   authenticated connection produced by `mini-transport-security`. This crate
//!   never chooses which peer is legitimate or turns discovery into truth.
""",
)

replace_exact(
    search_cargo,
    """description = "Bounded, authenticated real-transport delivery of mini-search-federation's F1/F2/F2b signed objects between two Mininet peers (Track F required follow-up: 'wire F1/F2 objects through a bounded real transport with authenticated peer behavior and source-count limits'), plus assembly of a pulled F2 segment + F2b corpus bundle into a real federate_query-ready source. The advertise/pull exchange never sends query terms -- a peer advertises which object ids it holds, nothing more. Track F6 Phase 1 (remote_query/serve_query) is the deliberate, separately-scoped exception: a bounded query string does cross the wire, confidential-in-transit only -- not a private-information-retrieval scheme, see docs/design/f6-private-query-transport.md."
""",
    """description = "Bounded real-transport delivery of signed federated-search objects plus anonymous or optionally channel-authenticated remote query/merge. The named path derives provider provenance from the exact authenticated transport endpoint; neither path is private information retrieval."
""",
)
replace_exact(
    search_cargo,
    """# IndexSegmentId/CrawlObservationId are all Multihash-backed) rather than
# introducing a second id representation.
""",
    """# IndexSegmentId/CrawlObservationId are all Multihash-backed) rather than
# introducing a second id representation. mini-transport-security supplies the
# optional named-session runtime; it remains an identity-binding layer, never a
# ranking, discovery, personhood, or truth authority.
""",
)

write(
    transport_readme,
    """# mini-transport-security

Optional self-certifying endpoint authentication, secure peer discovery, and an
executable connection seam above Mininet's anonymous `mini-bearer::Channel`.

## What is implemented

- `SessionAuthClaim` signs one exact CH1 channel binding with a delegated
  `did:mini` device for one endpoint role and typed transport purpose.
- Verification uses caller-supplied root/device KELs, delegation capability
  checks, highest-sequence freshness pins, bounded validity windows, and a
  validity-window replay cache that fails closed at capacity.
- `TransportEndpointId` binds the presented device or pairwise DID to its
  current X25519 routing key. Rotating the routing key rotates the endpoint id.
- `PeerAdvertisement` signs a network id, dial address, routing key, endpoint
  id, validity window, and internally generated replay nonce. Advertisements are
  dial hints; the live CH1 session must still prove the same endpoint and key.
- `SecurePexResponse` carries a bounded canonical list of signed advertisements.
- `diverse_dial_plan` is locally seeded, input-order-independent, duplicate-
  resistant, and capped per IPv4 `/24` or IPv6 `/48` prefix.
- `AuthenticatedConnection<B>` owns one bearer, the exact CH1 channel, and the
  peer verified on that channel as one object. It exposes authenticated `send`
  and `recv`, not detachable raw identity state.
- `connect_authenticated_tcp` performs signed-advertisement dial, CH1, encrypted
  responder-first authentication, and exact advertisement/session binding.
  `connect_first_authenticated_tcp` retries a bounded local diverse plan and
  returns no partially accepted state from failed attempts.
- `authenticate_established_initiator` and
  `authenticate_established_responder` accept a channel established by any
  bearer, including `mini-bridge` adapters, without making the bridge an
  identity authority.
- `build_verified_onion_route` accepts three already-verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building
  the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`.
- `executable_transport` permits implemented Direct and Relayed execution and
  refuses Mixed/Burst until the exact mix executor receives independent review.

## Transactional verification boundary

Runtime authentication clones `FreshnessPins` and `ReplayCache`, verifies the
complete remote proof, and commits those states only when the exchange reaches a
fully authenticated connection. A redirected genuine endpoint is rejected
before the initiator sends its own identity proof. Network, decode, identity,
role, purpose, freshness, or replay failure returns no accepted connection and
cannot partially advance caller-held freshness/replay state.

## Authority boundary

This crate creates no certificate authority, hosted directory, canonical relay
registry, hardcoded trusted peer, trust-on-first-use rule, admin key, recovery
key, identity-unmasking path, or download-majority rule. Payment, balance,
storage, bandwidth, and provider revenue are absent from every authority and
selection input.

Anonymous CH1 remains valid. A caller that needs unlinkability should present a
pairwise identity or use onion/mix routing rather than authenticating a global
root to every counterparty. An authenticated endpoint proves key control on one
channel; it is not personhood, operator independence, reputation, or truth.

## Exact limits

- A first-contact verifier cannot know about a later unseen revocation without
  witness/gossip freshness evidence. `FreshnessPins` prevents rollback below a
  KEL sequence already observed locally; it does not invent global freshness.
- Prefix diversity raises eclipse cost but does not prove independent ownership,
  ASN, jurisdiction, or operator identity.
- Endpoint authentication protects identity binding, not traffic shape or IP
  metadata. It can increase linkability when a global root is presented.
- The three-hop onion implementation lives in `mini-relay`; it protects payload
  confidentiality and separates endpoint knowledge, but is not Sphinx and does
  not defeat a global timing/volume observer.
- The bridge seam reuses `mini-bridge::PluggableTransport` and
  `PtProcessManager`; no real obfs4/WebTunnel/Snowflake adapter is added here.
- NAT traversal, reconnect, private bridge distribution, multipath migration,
  and background service supervision remain deployment work.

See `docs/planning/privacy-transport-runtime-convergence.md`,
`docs/planning/privacy-transport-security.md`, and
`docs/audits/issue-27-censorship-resistance-review.md`.
""",
)

# Update the F6 design from its former caller-asserted provenance floor to the
# new optional named path while preserving anonymous querying as a first-class
# mode.
replace_exact(
    f6_design,
    """**Decisions:** D-0435 (Phase 1), D-0436 (Phase 2) (see `docs/DECISION_LOG.md`)
**Status:** Phase 1 and Phase 2 implemented and tested. Not a private-information-retrieval scheme — see "What Phase 1 is not" below.
""",
    """**Decisions:** D-0435 (Phase 1), D-0436 (Phase 2), D-0437 (optional authenticated provider provenance) (see `docs/DECISION_LOG.md`)
**Status:** Phases 1 and 2 are merged. Phase 3 is implemented in draft PR #296. None is a private-information-retrieval scheme — see "What Phase 1 is not" below.
""",
)
replace_exact(
    f6_design,
    """- **Not a truth or trust upgrade.** The response is the queried provider's own computed ranking over its own held data — exactly as authoritative (and exactly as unverified against independent corroboration) as any other Track F source. `mini-transport-security`'s optional endpoint authentication (a separate, concurrently-developed crate) can bind *which* peer answered if a caller wants that; this document does not duplicate it.
""",
    """- **Not a truth or trust upgrade.** The response is the queried provider's own computed ranking over its own held data — exactly as authoritative (and exactly as unverified against independent corroboration) as any other Track F source. Phase 3 can prove *which transport endpoint* answered on one exact channel; it does not prove that endpoint is honest, human, independent of other endpoints, or correct.
""",
)
replace_exact(
    f6_design,
    """**The `remote_provider` tag is caller-asserted, not cryptographically verified** — unchanged from Phase 1's own stated floor. A query response carries no `Object`/signature wrapping, and F6 provides no caller/provider authentication beyond the channel itself. A caller names `remote_provider` from whatever it already knows out-of-band about who it dialed (an advertisement it resolved, its own session setup) — exactly as honest, and exactly as unverified, as every other Track F provider label already is once results leave a single signed object's custody. Binding this to `mini-transport-security`'s authenticated peer identity, once that crate lands review, remains real follow-up (see below), not attempted here.
""",
    """**The legacy `merge_remote_results` API keeps a caller-asserted `remote_provider` tag.** That remains intentional for anonymous CH1 and callers managing their own out-of-band provenance. Phase 3 adds a separate named path rather than silently changing anonymous semantics: `remote_query_authenticated` returns an `AuthenticatedQueryResults` whose provider is derived from the endpoint proved on the response channel, and `merge_authenticated_remote_results` accepts that sealed result object instead of a caller-selected provider label.
""",
)
phase3 = """
## Phase 3: optional authenticated provider provenance (D-0437, PR #296)

Phase 3 composes F6 with `mini-transport-security` instead of creating a second
identity or connection system:

- `TransportPurpose::SearchQuery` is a distinct signed session purpose. A proof
  disclosed for `PeerExchange`, messaging, relay, state sync, or consensus cannot
  be reused as search-provider provenance.
- `remote_query_authenticated` and `serve_query_authenticated` require an
  `AuthenticatedConnection<B>` that owns the bearer, exact CH1 channel, and peer
  verified on that channel. The response remains ordinary bounded F6 wire data;
  no durable signature or false re-verifiability claim is added.
- `authenticated_provider_pseudonym` domain-separates and hashes the verified
  `TransportEndpointId`. Because that endpoint id commits to the delegated device
  and current X25519 routing key, routing-key rotation also rotates the search
  provider label rather than creating a permanent global identifier.
- `AuthenticatedQueryResults` has private fields. External callers can inspect
  its provider and results but cannot construct one with an arbitrary provider
  label. `merge_authenticated_remote_results` consumes this sealed value through
  a crate-private split, closing Phase 2's silent caller-mislabel path for the
  named API.
- The anonymous `remote_query`/`serve_query` and legacy
  `merge_remote_results` APIs remain available. Identity disclosure is optional,
  not made mandatory for search.

A real TCP integration test proves signed advertisement verification, CH1,
`SearchQuery`-purpose mutual authentication, bounded remote search, peer-derived
provider labeling, and typed merge in one path. A second test proves a valid
`PeerExchange`-purpose connection is rejected by the authenticated search API.

**Exact remaining failure:** endpoint-bound provenance proves who controlled one
transport endpoint for one session, not that the provider's index is honest or
independently operated. Pairwise/routing-key rotation intentionally changes the
provider label, so durable cross-rotation reputation requires a separate,
privacy-conscious continuity design. The anonymous legacy path can still be
caller-mislabeled because that is its explicit contract.

"""
replace_exact(f6_design, "## Required follow-up\n", phase3 + "## Required follow-up\n")
replace_exact(
    f6_design,
    """- Bind `remote_provider` to `mini-transport-security`'s authenticated peer identity once that crate lands review, closing the caller-assertion gap Phase 2 explicitly leaves open.
""",
    """- Decide whether a future privacy-preserving continuity proof should link rotating authenticated provider labels without turning one global provider identity into a tracking or ranking authority.
""",
)
replace_exact(
    f6_design,
    """New ground — no prior decision addressed sending live query terms to a remote peer. Phase 2 (D-0436) builds directly on Phase 1 (D-0435), completing its named follow-up; it does not modify F1-F5/F7's own object formats or `federate_query`'s external behavior/signature (only its internal implementation, now delegating to the newly extracted `merge_federated_results`). Builds on and does not modify `mini_query::parse_query`/`search` (unmodified, reused exactly as-is), or `mini-search-federation-net`'s existing advertise/pull/assemble exchange (`message.rs`/`session.rs`/`multi.rs`/`assemble.rs`, all untouched).
""",
    """New ground — no prior decision addressed sending live query terms to a remote peer. Phase 2 (D-0436) builds directly on Phase 1 (D-0435), completing its merge follow-up. Phase 3 (D-0437) closes Phase 2's named caller-asserted-provider gap for an optional named path while preserving anonymous querying unchanged. None modifies F1-F5/F7's object formats or `federate_query`'s external behavior/signature. All build on and do not modify `mini_query::parse_query`/`search`, or the existing advertise/pull/assemble exchange.
""",
)

write(
    planning,
    """# Privacy transport runtime convergence

**Status:** implementation complete in draft PR #296; merge and production
claims remain gated on exact-head CI and human review.  
**Base state inspected:** current `main` at
`e60191c4a0fdc8be42995cc2fb21b9a56e910f44`, after PRs #289, #293, #294,
and #292 merged.  
**Decision track:** D-0377 plus proposed D-0437.

## Why this PR exists

PR #292 merged sound transport-security primitives: signed discovery, optional
channel-bound endpoint proofs, bounded replay/freshness state, locally seeded
prefix diversity, a three-hop destination-encrypted onion, and a fail-closed
Mixed/Burst gate. PR #294 merged bounded remote search and typed result merge,
but explicitly left its provider label caller-asserted. The remaining defect was
composition: normal callers could still forget to bind a selected advertisement
to the live channel, detach identity from the connection that proved it, mutate
freshness state during a failed exchange, or mislabel an F6 provider after the
response arrived.

PR #296 turns those seams into executable types and tests without replacing the
lower-level anonymous APIs.

## Implemented mechanisms

### One authenticated connection object

`AuthenticatedConnection<B>` owns one bearer, the exact CH1 `Channel`, and the
`AuthenticatedPeer` verified on that channel. It exposes authenticated `send`
and `recv`; it does not expose constructors or detachable bearer/channel fields.

`connect_authenticated_tcp` performs:

1. bounded TCP dial to one verified `PeerAdvertisement`;
2. anonymous CH1 establishment;
3. encrypted responder-first `SessionAuthClaim` exchange;
4. exact `verify_advertised` identity, endpoint-id, and routing-key binding; and
5. return of the inseparable authenticated connection.

Responder-first ordering means a redirected genuine endpoint must prove it is
the selected endpoint before the initiator discloses its own DID. Both sides
stage cloned `FreshnessPins` and `ReplayCache` values and commit them only after
the whole peer proof succeeds. Failure returns no connection and cannot
partially advance caller-held security state.

### Bounded retry without a trusted first peer

`connect_first_authenticated_tcp` consumes locally seeded,
input-order-independent, prefix-diverse dial attempts. Each network/codec/
identity failure is discarded whole; success still requires exact
advertisement-to-session binding. Exhaustion returns only a bounded attempted
count, not a partially verified peer.

### Existing bridge boundary, no second process authority

`authenticate_established_initiator` and
`authenticate_established_responder` accept any already-established
`Bearer`/`Channel`, including one returned by
`mini-bridge::PluggableTransport`. Camouflage and external adapter work remains
inside `mini-bridge::PtProcessManager`; transport security neither launches a
second adapter manager nor treats a bridge descriptor as identity truth.

### Verified onion route construction

`build_verified_onion_route` accepts three already-verified endpoint records and
rejects visible endpoint-id, routing-key, root, or device reuse before mapping
them to Entry/Rendezvous/Delivery. It then delegates packet cryptography to the
existing `mini-relay::build_onion`; no new onion construction is invented here.

### Peer-bound F6 search provenance

A distinct signed `TransportPurpose::SearchQuery` prevents a generic peer proof
from being replayed as search-provider provenance.
`remote_query_authenticated`/`serve_query_authenticated` operate only on an
`AuthenticatedConnection`. The provider pseudonym is domain-separated from the
verified `TransportEndpointId`, so routing-key rotation also rotates the label.
`AuthenticatedQueryResults` has private fields, and
`merge_authenticated_remote_results` consumes it without accepting a caller-
selected provider label. Anonymous CH1 and the legacy caller-labeled merge API
remain available as explicitly weaker alternatives.

## Verdict matrix

| Value | Verdict | Proving mechanism | Exact remaining failure |
|---|---|---|---|
| Discovery-to-live-session binding | **PASS** | `connect_authenticated_tcp` can return only after `verify_advertised` checks root, device, endpoint id, and routing key on the exact CH1 binding. | Lower-level primitives remain public for specialist callers; bypassing the runtime API preserves their caller-owned composition risk. |
| No identity leak to a redirected endpoint | **PASS** | Responder sends and verifies first; the initiator sends its claim only after the selected advertisement matches. | Network metadata still reveals the initiator IP to the contacted address. |
| Failed-attempt state atomicity | **PASS** | Freshness/replay values are cloned and committed only after full verification and successful exchange. | Crash-persistent replay state remains the host application's responsibility. |
| Central naming/bridge authority avoidance | **PASS** | Caller-held KELs, self-certifying endpoints, local selection, and reuse of the existing pluggable-transport boundary; no CA, canonical list, or bridge directory. | First-contact unseen KEL revocation still needs witness/gossip evidence. |
| Relay role separation at route build | **PARTIAL** | Three verified records must differ by endpoint id, routing key, visible root, and device. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, endpoint-derived provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth. |
| Global traffic-analysis resistance | **FAIL** | Mixed/Burst remain runtime-fail-closed rather than inheriting a false claim. | CH1/TCP timing, volume, transcript shape, and three-hop correlation remain visible until an independently reviewed mix/camouflage system exists. |

## Permanent evidence

- `mini-transport-security` strict Clippy and all focused tests pass, including
  four new real-TCP runtime tests and verified-route unit tests.
- `mini-search-federation-net` strict Clippy and all focused tests pass,
  including a real authenticated F6 query/merge and wrong-purpose rejection.
- `mini-relay` unit and real-socket onion tests pass unchanged.
- Tests prove redirect rejection before initiator disclosure, no partial
  freshness/replay mutation, bounded retry over an unreachable first hint,
  reuse of a `mini-bridge`-established channel, distinct verified onion roles,
  provider labels derived from the authenticated peer, and inability to reuse a
  `PeerExchange` proof as `SearchQuery` authority.

## Frozen authority boundaries

- Anonymous CH1 and pairwise identities remain valid; named identity is opt-in.
- No CA, DNS authority, TOFU database, canonical bootstrap/relay registry,
  hosted identity directory, mandatory bridge distributor, administrator,
  recovery, legal-unmasking, or traffic-master key.
- Payment, balance, stake, storage, bandwidth, provider revenue, and service
  metrics never enter identity, routing, ranking, personhood, validator, review,
  or governance authority.
- Availability remains a hint. KELs, delegation checks, signed advertisements,
  channel transcripts, and content/state proofs remain authoritative only in
  their own domains.

## Honest non-claims and required follow-up

- No production NAT traversal, mobile reconnect supervisor, private bridge
  distribution, multipath migration, or background daemon is added.
- No obfs4/WebTunnel/Snowflake executable is bundled. TCP/CH1 remains
  fingerprintable and blockable.
- The three-hop onion is not Sphinx and does not resist a global timing/volume
  observer. Mixed/Burst remain externally gated under #72.
- Prefix diversity and visible identity separation do not prove independent
  operator, ASN, jurisdiction, or human ownership.
- First-contact KEL freshness cannot reveal an unseen later revocation without
  witness/gossip receipts.
- Named F6 proves endpoint control, not index honesty. Privacy-preserving
  continuity across rotating provider endpoints remains undesigned.
- True query-content privacy against the provider requires independently
  reviewed PIR/oblivious-search work; the provider still sees the query.

## Merge floor

The final SHA must contain no staging helper or write-capable workflow and must
pass formatting, strict Clippy, complete workspace tests, dependency policy,
governance, reproducibility, Android, Android reproducibility, CodeQL, and
navigation checks. Human approval is mandatory; AI-authored code and evidence
carry zero approval weight.

Refs #291, #292, #296, #24, #27, #72, #175, #289, #293, #294.
""",
)

new_status = """## Privacy transport runtime convergence — D-0377 and proposed D-0437

- **shipped in PR #292** — optional channel-bound endpoint authentication,
  signed network-bound discovery, validity-window replay retention, local
  prefix-diverse dial planning, real three-hop destination-encrypted onion
  forwarding, and a runtime-fail-closed Mixed/Burst gate.
- **implemented in draft PR #296** — `AuthenticatedConnection<B>` fuses one
  bearer, exact CH1 channel, and peer verified on that channel. The TCP path
  performs signed-advertisement dial, responder-first proof, exact
  advertisement/session binding, and transactional freshness/replay commit.
- **implemented in draft PR #296** — bounded local retry discards every failed
  attempt whole; bridge-created channels enter the same authentication seam
  through the existing `mini-bridge` boundary; verified onion route assembly
  rejects visible endpoint/routing-key/root/device reuse across roles.
- **implemented in draft PR #296** — optional named F6 search uses the distinct
  `SearchQuery` purpose, derives a rotating provider pseudonym from the endpoint
  proved on the response channel, and seals the authenticated merge input behind
  private fields. Anonymous search and caller-labeled legacy merge remain
  available as explicitly weaker contracts.
- **fail-closed** — `PrivacyTier::Mixed` and `Burst` still have no operational
  executor. `mini_transport_security::executable_transport` refuses them until
  the exact D-0305 Sphinx/Loopix implementation receives #72's independent
  review.
- **open exact limits** — first-contact KEL witness freshness; independent
  operator/ASN/jurisdiction evidence; NAT traversal and reconnect; private
  bridge operations; real camouflage adapters; ISP-throttling resistance;
  global timing/volume/intersection protection; and privacy-preserving
  continuity for rotating authenticated search providers.

**Authority boundary:** anonymous CH1 and pairwise identities remain valid;
there is no CA, canonical relay/bootstrap registry, hosted identity directory,
trusted first peer, majority-by-download rule, admin/unmasking key, or
value-to-routing/ranking/voice path. An authenticated endpoint proves control of
one key-bound endpoint on one channel, not personhood, operator independence,
result truth, or governance standing.
"""
replace_from_marker(status, "## Privacy and transport security — D-0377 proposal", new_status)

new_threat = """## Transport/privacy addendum — D-0377 and proposed D-0437

| Threat | Current mechanism | Status / exact failure |
|---|---|---|
| **Unsigned discovery redirect** | Signed, expiring, network-bound `PeerAdvertisement`; `connect_authenticated_tcp` establishes CH1 and invokes `verify_advertised` before returning a connection. | **Closed for the runtime path.** Legacy `mini-net::pex` and specialist callers composing lower-level APIs directly retain caller-owned redirect risk. |
| **Redirected genuine endpoint collecting initiator identity** | Runtime authentication is responder-first: the dialed peer proves the exact advertised endpoint before the initiator sends its claim. | **Closed for named runtime sessions.** The contacted address still observes source-IP/network metadata. |
| **Partial freshness/replay mutation on failed authentication** | Runtime verification clones `FreshnessPins` and `ReplayCache` and commits only after the full exchange succeeds. | **Closed in-process.** Crash-persistent replay state remains a host responsibility. |
| **Endpoint impersonation on encrypted CH1** | Delegated-device signature over channel binding, role, typed purpose, pairwise/root identity, rotating X25519 key, expiry, and replay nonce. | **Closed within caller-supplied KEL freshness.** First-contact unseen revocation remains open until witness/gossip freshness exists. |
| **Bootstrap eclipse** | Caller-local seeded ordering, endpoint/routing-key deduplication, bounded retry/timeouts, IPv4 `/24` and IPv6 `/48` caps. | **Partial.** One adversary can acquire diverse prefixes/ASNs or control all discovery sources; address diversity is not operator independence. |
| **Bridge adapter becoming identity authority** | Any `mini-bridge::PluggableTransport` may establish the bearer/channel, but the same independent session proof verifies identity afterward. | **Closed structurally.** A malicious/blockable adapter can deny or observe availability; it cannot make its descriptor an accepted peer identity. |
| **One visible endpoint assigned multiple onion roles** | `build_verified_onion_route` rejects endpoint-id, routing-key, visible-root, or device reuse before Entry/Rendezvous/Delivery construction. | **Partial.** One hidden operator can control several pairwise roots, devices, addresses, or ASNs. |
| **Entry-relay plaintext exposure** | Three independent Entry/Rendezvous/Delivery X25519+AEAD layers around a destination-encrypted fixed-size payload. | **Closed for payload content.** Each relay still observes its local predecessor, timing, volume class, and opaque next-hop token. |
| **Cross-hop clear identifier correlation** | Every relay layer has an independent random public connection id; the destination id exists only inside destination encryption. | **Closed for explicit circuit ids.** Timing/volume correlation remains open. |
| **Forged F6 provider label after authenticated query** | `SearchQuery`-purpose `AuthenticatedConnection`, endpoint-derived provider pseudonym, private `AuthenticatedQueryResults` fields, and `merge_authenticated_remote_results`. | **Closed for the named API.** Anonymous/legacy merge is intentionally caller-labeled; endpoint control does not prove result truth. |
| **Purpose confusion in provider provenance** | `SearchQuery` is a distinct signed purpose, and authenticated query APIs reject any other purpose. | **Closed for the typed API.** A provider still sees the full query and may log or manipulate results. |
| **Public relay/bridge blocking** | No canonical relay registry; signed endpoints and local route rotation permit many independent relays. | **Partial.** Known TCP addresses remain enumerable and blockable; private bridge distribution and real camouflage adapters are unbuilt. |
| **DPI/protocol fingerprinting** | Payload encryption and fixed destination size classes. | **Open/critical.** CH1 handshake, TCP framing, size classes, timing, and connection behavior remain recognizable; no reviewed camouflage profile exists. |
| **ISP throttling/blackholing** | Bounded timeouts prevent indefinite stalls. | **Open/critical.** Internet TCP can be delayed or dropped; multipath migration and progress-aware resumable routing are unbuilt. |
| **Global timing/volume/intersection observer** | Relayed payload separation only; Mixed/Burst runtime gate. | **Open/critical.** Three-hop onion is not a mixnet. The D-0305 Sphinx/Loopix executor and external review remain mandatory. |
| **Administrative unmasking/capture** | No traffic master key, escrow key, CA, hosted directory, canonical relay list, or identity-correlation service; pairwise identity is valid. | **Closed structurally in the current types.** Future adapters and continuity schemes must preserve this dependency wall. |

The complete state-censorship assessment and concrete mitigation sequence are in
`docs/audits/issue-27-censorship-resistance-review.md`. This addendum does not
claim NAT traversal, camouflage, private query content, independent operators,
or global anonymity exists.
"""
replace_from_marker(threat_model, "## Transport/privacy addendum — D-0377", new_threat)

if "### D-0437 —" in read(decision_log):
    raise SystemExit("D-0437 already exists")
decision = """

### D-0437 — Authenticated transport runtime convergence and peer-bound F6 provenance  ·  *Proposed*

**Date:** 2026-08-04 · **Refs:** D-0377, D-0435, D-0436; PR #296;
issues #291/#24/#27/#72/#175; merged PRs #292/#294; Directives
2/5/6/9/14/16/18.

**Decision:** add one executable runtime seam in `mini-transport-security` rather
than leaving signed discovery, CH1, endpoint proof, retry, bridge entry, and
onion route construction as caller conventions. `AuthenticatedConnection<B>`
owns the bearer, exact CH1 channel, and `AuthenticatedPeer` verified on that
channel. The TCP initiator verifies a responder-first claim against the selected
`PeerAdvertisement` before disclosing its own identity. Freshness/replay state is
transactional: cloned before verification and committed only after the complete
exchange succeeds. Bounded retry uses the existing locally seeded prefix-diverse
plan and returns no partial accepted state. Already-established
`mini-bridge::PluggableTransport` channels enter this same proof seam; no second
adapter process manager or bridge identity authority is created.

Add `build_verified_onion_route`, which accepts three verified endpoint records,
rejects visible endpoint-id/routing-key/root/device reuse across roles, and then
uses the unchanged `mini-relay::build_onion` implementation.

Close D-0436's named F6 provenance gap with a distinct signed
`TransportPurpose::SearchQuery`. The optional named query path operates on an
`AuthenticatedConnection`, derives a rotating `ProviderPseudonym` from the
verified `TransportEndpointId`, returns an `AuthenticatedQueryResults` with
private fields, and merges it without accepting a caller-selected provider
label. Preserve anonymous CH1 querying and the legacy caller-labeled merge API;
identity disclosure remains optional.

**Reason:** every individual primitive in D-0377 could be sound while an
application still forgot one composition check, advanced security state during a
failed attempt, detached identity from the channel that proved it, or mislabeled
which F6 provider answered. Security-critical sequencing and provenance belong
in types and executable code, not documentation telling every future caller to
remember the same multi-step ritual.

**Constitutional impact:** strengthens Directive 2 by adding no mandatory
operator; Directive 5 by retaining caller-held KEL authority; Directive 6 by
making failed attempts atomic and explicit; Directive 9 by keeping named
identity optional and pairwise-compatible; Directive 14 by reusing CH1,
`mini-bridge`, and `mini-relay` rather than inventing duplicate transport or
cryptography; Directive 16 by keeping payment/value absent from route, provider,
ranking, identity, and governance authority; and Directive 18 by keeping external
adapters at the edge behind the existing bridge boundary.

No CA, DNS authority, TOFU database, canonical bootstrap/relay registry, hosted
identity directory, mandatory bridge distributor, administrator, recovery,
legal-unmasking, or traffic-master key exists. No balance, payment, stake,
storage, bandwidth, provider revenue, or service metric enters identity,
routing, ranking, personhood, validator, review, or governance authority.

**Implementation status:** complete in draft PR #296. Permanent code adds the
runtime module, typed search purpose, peer-bound authenticated F6 query/merge,
and verified onion-route builder. Permanent real-socket tests prove signed
advertisement -> CH1 -> exact peer binding -> application data; redirect
rejection before initiator disclosure; atomic freshness/replay state on failure;
bounded retry past an unreachable first hint; reuse of a `mini-bridge` channel;
authenticated search-provider provenance; and wrong-purpose rejection. Focused
strict Clippy and all tests for `mini-transport-security`,
`mini-search-federation-net`, and `mini-relay` pass. Exact-head full workspace,
dependency, governance, reproducibility, Android, CodeQL, and human-review checks
remain the merge floor.

**Failure point:** a verified endpoint proves control of one key-bound endpoint
on one channel, not personhood, honesty, independent operation, ASN/jurisdiction
diversity, or result truth. Pairwise/routing-key rotation intentionally rotates
the F6 provider label; privacy-preserving durable continuity is undesigned.
First-contact KEL freshness still cannot reveal an unseen later revocation.
Known TCP endpoints remain blockable/fingerprintable; NAT traversal, reconnect,
private bridge distribution, multipath migration, and real camouflage adapters
are absent. The three-hop onion remains correlatable by a global timing/volume
observer; Mixed/Burst stay closed.

**Required follow-up:** add witness/gossip KEL freshness; independently evidenced
operator/ASN/jurisdiction diversity; issue #24 NAT/reconnect/relay fallback;
private bridge distribution and reviewed adapters under the existing
`mini-bridge` boundary; privacy-conscious provider continuity if a real product
needs cross-rotation reputation; and the externally reviewed D-0305 mix executor
under #72. None may introduce a mandatory checkpoint, canonical directory,
cloud front, unmasking key, or value-derived authority.

**Supersedes / superseded by:** builds on and does not supersede D-0377's wire
formats or lower-level optional primitives. Closes D-0436's named caller-asserted
provider-label gap for the new named API while preserving D-0435/D-0436's
anonymous APIs unchanged. Does not supersede D-0305 or lift any external-review
gate.
"""
write(decision_log, read(decision_log).rstrip() + decision + "\n")

print("stage 3 applied")

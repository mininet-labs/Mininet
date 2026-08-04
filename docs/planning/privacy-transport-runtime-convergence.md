# Privacy transport runtime convergence

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
| Ordered connection state after transport failure | **PASS** | `AuthenticatedConnection` permanently poisons itself after bearer send/receive or channel-open failure, so a caller cannot continue after an ambiguous CH1 counter/stream position. | Recovery requires a new channel; the generic lower-level `Channel` + `Bearer` APIs remain caller-managed. |
| Central naming/bridge authority avoidance | **PASS** | Caller-held KELs, self-certifying endpoints, local selection, and reuse of the existing pluggable-transport boundary; no CA, canonical list, or bridge directory. | First-contact unseen KEL revocation still needs witness/gossip evidence. |
| Relay role separation at route build | **PARTIAL** | Three live, same-network verified records must differ by endpoint id, routing key, visible root, and device. | One hidden operator can control several valid pairwise roots, devices, prefixes, or ASNs. |
| Relay and destination replay defense | **PASS in-process** | Onion v2 uses v2 key domains, encrypts expiry/replay tokens for every relay and destination, bounds lifetime with explicit clock-skew tolerance, retains a monotonic local time high-water mark so wall-clock rollback cannot resurrect expired tokens, never evicts live entries, records only after inner validation, and fails closed at capacity. | Hosts must persist equivalent replay state and its time high-water mark across restart; authenticated packet floods can still exhaust bounded capacity and require rate/resource controls. |
| Authenticated F6 provider labeling | **PASS for the named API** | Typed `SearchQuery` proof, private `AuthenticatedQueryResults` fields, channel-scoped endpoint+CH1 provider pseudonym, and sealed merge path. | Anonymous/legacy APIs intentionally retain caller-owned labeling; provider identity does not prove result truth or continuity across sessions. |
| Global traffic-analysis resistance | **FAIL** | Mixed/Burst remain runtime-fail-closed rather than inheriting a false claim. | CH1/TCP timing, volume, transcript shape, and three-hop correlation remain visible until an independently reviewed mix/camouflage system exists. |

## Permanent evidence

- `mini-transport-security` strict Clippy and all focused tests pass, including
  four real-TCP authentication/runtime tests, verified-route unit tests, and one
  signed-discovery -> local selection -> verified onion-route ->
  three-relay-socket -> destination-only plaintext test, plus a full chain where
  client, every relay-to-relay hop, and delivery-to-destination all use typed
  `Relay`-purpose `AuthenticatedConnection`s.
- `mini-search-federation-net` strict Clippy and all focused tests pass,
  including a real authenticated F6 query/merge and wrong-purpose rejection.
- `mini-relay` unit and real-socket onion tests pass after the onion-v2
  replay/lifetime upgrade, including destination replay, fail-closed capacity,
  expiry pruning, excessive-lifetime rejection, and malformed-state atomicity.
- Tests prove redirect rejection before initiator disclosure, no partial
  freshness/replay mutation, bounded retry over an unreachable first hint,
  reuse of a `mini-bridge`-established channel, distinct verified onion roles,
  signed advertisements feeding real onion forwarding with CH1 authentication
  on every socket, connection poisoning after ambiguous transport failure, provider
  labels derived from the authenticated peer, and inability to reuse a
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
- Named F6 proves endpoint control on one exact channel, not index honesty.
  Provider labels intentionally rotate across channels; privacy-preserving
  durable continuity remains undesigned.
- True query-content privacy against the provider requires independently
  reviewed PIR/oblivious-search work; the provider still sees the query.

## Merge floor

The final SHA must contain no staging helper or write-capable workflow and must
pass formatting, strict Clippy, complete workspace tests, dependency policy,
governance, reproducibility, Android, Android reproducibility, CodeQL, and
navigation checks. Human approval is mandatory; AI-authored code and evidence
carry zero approval weight.

Refs #291, #292, #296, #24, #27, #72, #175, #289, #293, #294.

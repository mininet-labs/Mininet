# MN-207 Research Report: Bridges, Pluggable Transports, and Censorship-Resistant Entry Privacy

Research target

Repository: mininet-labs/mininet
Work item: MN-207 — bridge / pluggable transport
Research date: 14 July 2026

---

## Executive conclusion

MN-207 should define and later implement a pluggable entry-transport framework that lets Mininet reach its ordinary relay, rendezvous, and future mix services through multiple censorship-resistant entry mechanisms without coupling the core network to any one disguise.

The recommended architecture is not "invent a Mininet obfuscation protocol."

It is:

Build a small, typed pluggable-transport interface; adopt proven transports through adapters; provide several independent bridge-distribution paths; support local Wi-Fi/Bluetooth forwarding; measure blocking; and rotate transport choices under policy.

Mininet's research direction already requires:

* unpublished or invitation-only bridges;
* rotating bridge identities;
* pluggable-transport obfuscation;
* local Wi-Fi/Bluetooth bridge forwarding;
* multiple bootstrap sources;
* no single public relay-directory dependency.

The first production-capable set should be:

1. Direct TLS/QUIC bridge mode for networks that block public Mininet infrastructure by address but do not perform strong traffic classification.
2. obfs4-compatible or Lyrebird-backed mode for random-looking authenticated obfuscation.
3. WebTunnel-style HTTPS mode for environments where ordinary encrypted web traffic remains usable.
4. Snowflake-style ephemeral volunteer proxy mode for rapid IP churn and broad volunteer participation.
5. Local bridge mode over BLE or local Wi-Fi, where one nearby device has wider connectivity.
6. Tor and I2P compatibility bearers as bootstrap and fallback, consistent with the project's existing direction that they remain optional bearers rather than the whole architecture.

The framework must separate four concerns:

Discovery      How a client learns a bridge candidate
Camouflage     How traffic appears on the censored link
Carriage       How bytes reach the Mininet entry relay
Health policy  How the client detects failure and chooses alternatives

A transport that solves one does not automatically solve the others.

The core recommendation is a portfolio, not a winner:

* obfs4 for strong active-probing resistance and non-mimicking obfuscation;
* WebTunnel for blending into normal HTTPS deployment patterns;
* Snowflake for rapidly changing volunteer IP addresses;
* MASQUE/HTTP tunnelling only as a carriage option, not as a censorship solution by itself;
* local BLE/Wi-Fi bridges for internet outages, account blocking, or selective local access;
* invitation bridges for small high-risk communities;
* Tor as an optional upstream path when Mininet-native bridges are unavailable.

No transport should be described as undetectable. Every transport has a fingerprint, deployment dependency, or blocking strategy available to a sufficiently capable censor.

The honest security claim is:

MN-207 increases the cost and collateral damage of blocking Mininet by making entry traffic transport-agile, bridge addresses non-universally enumerable, and bootstrap paths diverse. It does not make network use impossible to detect under all conditions.

---

## 1. Scope and relationship to existing lanes

MN-207 sits at the boundary between the user and the first Mininet-visible network service.

It is not:

* the Tier 1 relay protocol itself;
* the Tier 2 mix packet;
* object encryption;
* rendezvous semantics;
* private DHT lookup;
* anonymous payment;
* censorship-resistant storage.

The current wave-two issue for Tier 1 explicitly lists MN-207 as a later bridge/pluggable-transport task. It also confirms that Mininet currently lacks a DHT value-storage layer, so MN-208 remains separate and deferred.

### Relationship to Tier 1

Tier 1 separates roles so an entry relay knows the client IP but not the destination, while rendezvous and optional delivery relays learn other limited pieces. No direct user-to-user connection is permitted.

MN-207 protects the client's ability to reach that entry relay when:

* its address is blocked;
* Mininet's handshake is fingerprinted;
* ordinary relay traffic is throttled;
* public bootstrap services are unavailable;
* the network permits only selected protocols;
* a local peer has connectivity the client lacks.

### Relationship to Tier 2

A pluggable transport conceals or changes the client-to-entry link.

A mixnet separates and delays traffic after entry.

A user may need both:

```
Client
  → pluggable transport / bridge
  → Mininet entry
  → mix route
  → destination service
```

A mixnet without bridge support may be unreachable in censored regions.

A bridge without mixing may hide protocol use or bypass blocking but does not provide broad sender-destination unlinkability.

### Relationship to the cost doctrine

Entry obfuscation spends:

* additional handshake bytes;
* proxy bandwidth;
* deployment diversity;
* latency;
* bridge-distribution resources;
* domain or hosting capacity;
* monitoring;
* volunteer infrastructure.

The project's research classifies local observers and censorship systems as an adversary class countered through entry obfuscation, bridges, padding, and cover resources.

---

## 2. Problem definition

A censor can block Mininet at several layers.

### 2.1 Address blocking

The censor blocks: public Mininet relay IP addresses; known domains; known hosting ranges; directory services; bootstrap nodes.

### 2.2 Protocol fingerprinting

The censor identifies: handshake byte patterns; TLS fingerprints; QUIC transport parameters; packet sizes; timing; connection direction; retry behaviour; application-layer framing.

### 2.3 Active probing

After observing a suspected bridge, the censor connects to it and sends protocol-specific inputs to determine whether it is a Mininet bridge.

### 2.4 Traffic analysis

Even when individual bytes look ordinary, the censor classifies: burst sizes; session duration; uplink/downlink ratios; packet timing; idle behaviour; TLS record distributions; destination concentration.

### 2.5 Bridge enumeration

The censor requests bridges through the same distribution mechanisms available to users, then blocks each learned address.

### 2.6 Distribution blocking

The censor blocks: bridge websites; email bridge delivery; app-store updates; DNS bootstrap; social distribution channels; known broker domains.

### 2.7 Collateral-pressure blocking

The censor blocks an entire protocol or cloud provider if the political and economic cost is acceptable.

### 2.8 Complete disconnection

The censor disables external internet access entirely while local networks or personal proximity remain available.

No one transport solves every layer.

---

## 3. Threat model

### 3.1 Adversary capabilities

The MN-207 adversary may: observe client traffic at an ISP, carrier, workplace, campus, or Wi-Fi network; block IPs, ports, domains, SNI values, DNS responses, protocols, or autonomous systems; inspect packet contents and metadata; perform active probes; create legitimate-looking users to request bridge addresses; operate malicious bridge distributors; operate malicious volunteer proxies; throttle rather than fully block; adapt classifiers after observing deployments; correlate bridge use with later Mininet traffic; coerce hosting providers; block app updates; prevent access to one or more bootstrap channels.

### 3.2 Assumptions

At least one of the following remains available: some ordinary HTTPS traffic; some WebRTC or browser-like traffic; an unpublished reachable IP; a local Wi-Fi or Bluetooth peer; a Tor or I2P path; an offline bridge descriptor transferred by a trusted person; a removable-media or QR bootstrap; a permitted cloud or content-hosting path.

If every outbound channel is disabled and no local courier exists, no network protocol can create connectivity from nothing.

### 3.3 Adversary exclusions

MN-207 alone does not protect against: compromised endpoints; malicious destination services; all selected mix nodes colluding; identification through message content; physical surveillance of the user; device seizure; total radio jamming; global long-term traffic correlation after the bridge; bridge operators logging client IPs.

---

## 4. Design principles

### 4.1 Transport agility is the primary defence

A fixed disguise eventually becomes a fixed signature.

The core must support multiple transports that can be added, removed, upgraded, regionally prioritised, disabled after blocking, independently audited, and selected by policy.

Tor's pluggable-transport model was created around this exact principle: modular subprocesses transform traffic so circumvention mechanisms can be developed and deployed without rewriting the anonymity network. (spec.torproject.org)

Mininet should adopt the architectural lesson, not necessarily Tor's exact subprocess control protocol.

### 4.2 No mandatory transport

The core relay protocol must remain usable over raw TCP, QUIC, WebSocket, HTTP, WebRTC data channels, Tor streams, I2P streams, BLE, local Wi-Fi, and future transports.

The privacy-policy router should select an eligible carrier based on the requested properties and current network conditions. The founder research already requires applications to request properties rather than hard-code bearers.

### 4.3 Camouflage must not become authentication

The bridge still needs cryptographic authentication independent of whatever outer protocol it imitates or uses.

A fake HTTPS tunnel that reaches an attacker must not be accepted merely because the TLS certificate or domain looks plausible.

### 4.4 Bridge discovery must be separate from bridge use

A client should be able to obtain the same bridge descriptor through trusted contacts, QR codes, email, app-bundled reserve lists, distributed invitation channels, rendezvous brokers, removable media, governance-approved public lists, Tor or I2P, or local device exchange.

The descriptor format should be transport-neutral.

### 4.5 Failure must not become a fingerprint

A client that tries transports in a fixed global order can be classified.

Selection should consider cached regional success, randomisation, user risk profile, previous blocking, latency budget, data budget, bridge freshness, and transport availability.

### 4.6 Do not overfit to one censor

A transport effective in one country or network may fail elsewhere.

The policy must be adaptive and measurement-driven.

---

## 5. Proposed Mininet pluggable-transport interface

The interface should operate on an authenticated byte stream or datagram abstraction.

Conceptually:

```rust
pub trait PluggableTransport {
    type ClientConfig;
    type ServerConfig;
    type Error;
    fn transport_id(&self) -> TransportId;
    async fn connect(
        &self,
        bridge: &BridgeDescriptor,
        config: &Self::ClientConfig,
        deadline: Deadline,
    ) -> Result<TransportChannel, Self::Error>;
    fn observable_profile(&self) -> ObservableProfile;
    fn supports(&self) -> TransportCapabilities;
}
```

The exact Rust design should follow repository conventions, but the semantic boundary matters more than syntax.

### 5.1 TransportId

Use a typed, versioned identifier:

```rust
pub enum TransportId {
    DirectTlsV1,
    DirectQuicV1,
    Obfs4V1,
    WebTunnelV1,
    SnowflakeV1,
    TorStreamV1,
    I2pStreamV1,
    LocalBleV1,
    LocalWifiV1,
}
```

The enum may be non-exhaustive if external adapters are needed.

Do not use an arbitrary command string as the security policy interface.

### 5.2 TransportCapabilities

Capabilities might include:

```rust
pub struct TransportCapabilities {
    stream: bool,
    datagram: bool,
    active_probe_resistance: ProbeResistance,
    address_agility: AddressAgility,
    requires_domain: bool,
    requires_broker: bool,
    supports_local_only: bool,
    expected_overhead: CostClass,
}
```

These are policy facts, not marketing labels.

### 5.3 BridgeDescriptor

A bridge descriptor should include only what the selected transport requires:

```rust
pub struct BridgeDescriptor {
    version: BridgeDescriptorVersion,
    bridge_key: BridgeIdentityKey,
    transport: TransportId,
    endpoint: EncryptedOrOpaqueEndpoint,
    transport_parameters: BoundedTransportParameters,
    valid_from: u64,
    expires_at: u64,
    distributor_scope: Option<DistributorScope>,
    signature: BridgeDescriptorSignature,
}
```

Requirements: signed; versioned; bounded; short-lived where practical; independent from the user's global DID; no generic executable command; no arbitrary environment variables; no shell interpolation; no plaintext user identity; no long-lived stable distribution identifier.

### 5.4 Authenticated inner handshake

After the outer transport connects, Mininet must perform its own authenticated ephemeral handshake through the tunnel.

The transport provides carriage and camouflage.

The inner handshake provides: bridge authenticity; forward secrecy; protocol negotiation; channel binding; replay resistance; downgrade resistance.

A censor-operated imitation transport must not be able to impersonate a real Mininet bridge.

---

## 6. Bridge roles

### 6.1 Static private bridge

A non-public relay reachable at a stable or slowly changing endpoint. Best for invitation communities, organisations, trusted contacts, low-scale censorship, long-lived operational reliability. Risk: once discovered, the address can be blocked.

### 6.2 Rotating bridge

A bridge whose address, domain, port, transport secret, descriptor, certificate, or hosting location changes under a scheduled or emergency rotation policy. Rotation must not be so frequent that clients cannot update.

### 6.3 Ephemeral volunteer proxy

A short-lived proxy forwards the client to a stable Mininet entry or bridge.

Snowflake demonstrates this operational model: clients are paired with volunteer ephemeral proxies, making wholesale IP enumeration harder because the proxy population changes frequently. Tor currently documents Snowflake as a supported bridge-operation mode alongside obfs4 and WebTunnel. (community.torproject.org)

Risk: broker dependency; WebRTC fingerprinting; malicious volunteer proxies; NAT traversal failure; unstable performance.

### 6.4 Local bridge

A nearby device receives Mininet traffic over BLE, Wi-Fi Direct, local hotspot, LAN TCP, or QR-assisted pairing, and forwards through its own available bearer.

This supports partial internet outages, one connected device serving several local devices, roaming, censored SIMs using another connection, store-and-forward handoff, and offline community bootstrap.

The local bridge learns physical proximity and likely device timing. It should not automatically learn the destination or application object.

### 6.5 Compatibility bridge

A Tor or I2P path carries the Mininet inner handshake to an entry service.

This is valuable because Mininet can benefit from an existing circumvention ecosystem while its native bridge population develops. It must remain optional.

---

## 7. Transport evaluation

### 7.1 Direct TLS or QUIC bridge

Connect to an unpublished bridge using ordinary TLS or QUIC, then run the Mininet inner protocol.

Strengths: simple; fast; low overhead; mature libraries; easy mobile integration; looks like encrypted internet traffic at a broad level.

Weaknesses: endpoint blocking; TLS/QUIC fingerprinting; SNI or certificate correlation; active probing; traffic-shape identification; hosting concentration.

Recommendation: ship as the lowest-cost bridge mode, but never label it obfuscated merely because it uses TLS.

### 7.2 obfs4 / Lyrebird-style transport

obfs4 transforms traffic to appear random and uses an out-of-band secret to resist active probing. Tor's operational documentation continues to recommend obfs4 bridges and notes that the implementation has been renamed Lyrebird. (community.torproject.org)

Strengths: field-tested; active-probing resistance; bridge-specific secret; does not depend on faithfully mimicking a complex public protocol; relatively straightforward deployment; supported on many platforms.

Weaknesses: random-looking traffic can itself be blocked where allowlisting is used; bridge IPs remain blockable after discovery; handshake and flow classifiers can evolve; requires descriptor distribution; less useful when only sanctioned web traffic is permitted.

Recommendation: make obfs4-compatible transport the first serious censorship-resistant adapter. Prefer integration with an audited implementation such as Lyrebird rather than rewriting the protocol from scratch.

### 7.3 WebTunnel-style HTTPS transport

The bridge is deployed behind an ordinary HTTPS-facing web server or endpoint and carries tunnel traffic through a web-compatible route.

Tor currently provides a dedicated WebTunnel bridge deployment path. (community.torproject.org)

Strengths: blends operationally with common HTTPS infrastructure; can share an endpoint with legitimate web content; blocking can impose collateral damage; easier to host behind common reverse proxies; useful where random-looking traffic is blocked.

Weaknesses: TLS fingerprints; unusual HTTP request patterns; domain blocking; hosting-provider pressure; server configuration complexity; camouflage may degrade if deployments all use identical templates.

Recommendation: adopt as the second native adapter after obfs4. Require deployment diversity and ordinary decoy web behaviour rather than a universal Mininet-only path.

### 7.4 Snowflake-style ephemeral proxying

Clients obtain short-lived volunteer proxies through a broker and use a broadly deployed browser communication protocol, typically WebRTC, to reach a stable bridge.

Strengths: large, rapidly changing IP population; easy volunteer participation; useful against address enumeration; clients may not require a stable private bridge; high collateral cost if a censor broadly blocks the underlying protocol.

Weaknesses: broker discoverability; WebRTC and DTLS fingerprints; dependency on third-party infrastructure; volunteer reliability; NAT and connectivity variation; malicious proxy observation; performance inconsistency.

Academic evaluation has shown that specific Snowflake handshake features could distinguish it from ordinary WebRTC applications with very high accuracy in the studied dataset, demonstrating that use of a popular protocol does not automatically confer indistinguishability. (arXiv)

Recommendation: support through an adapter, but describe it as address-agile circumvention, not guaranteed WebRTC indistinguishability. Mininet should reuse a mature Snowflake implementation rather than create a superficially similar protocol.

### 7.5 Tor pluggable-transport compatibility

The Mininet client launches or connects to a Tor PT implementation through the established PT control interface, then carries a Mininet connection through it.

Tor's specification defines modular subprocess startup, shutdown, and inter-process communication specifically to allow censorship circumvention modules to evolve independently. (spec.torproject.org)

Strengths: mature ecosystem; immediate access to obfs4, Snowflake, WebTunnel, and future transports; separates Mininet core from transport implementations; reduces local protocol invention; supports desktop deployment quickly.

Weaknesses: subprocess management complexity; difficult mobile sandbox integration; dependency footprint; PT interface was designed around Tor's deployment assumptions; adapter metadata and lifecycle must be hardened; not all transports fit every Mininet environment.

Recommendation: use Tor PT subprocess compatibility as a desktop and research integration layer. For mobile and embedded use, define a native in-process trait with equivalent semantic boundaries.

### 7.6 MASQUE / CONNECT-UDP

MASQUE's CONNECT-UDP standard allows UDP traffic to be tunnelled through an HTTP proxy, including HTTP/2 and HTTP/3 carriage. RFC 9298 is an IETF Standards Track specification. (datatracker.ietf.org)

Strengths: standards-based; integrates with HTTP infrastructure; supports QUIC and UDP applications; useful for general proxy carriage; can multiplex efficiently.

Weaknesses: not inherently censorship-resistant; proxy address remains blockable; URI and deployment patterns may be identifiable; ordinary MASQUE provides carriage, not hidden bridge distribution; an allowlisting censor may permit only selected proxies.

Recommendation: treat MASQUE as a carrier adapter, not as the MN-207 censorship strategy. It can be used inside a WebTunnel-like or privately distributed bridge deployment.

### 7.7 Oblivious HTTP

Oblivious HTTP separates a relay that sees the client connection from a gateway that decrypts and processes the request. RFC 9458 standardises the architecture. (datatracker.ietf.org)

Strengths: established two-party privacy separation; useful for discrete request/response operations; standardised; deployable through ordinary HTTP infrastructure.

Weaknesses: not a general long-lived bidirectional stream by itself; not designed primarily for censorship circumvention; requires relay/gateway non-collusion; endpoint and protocol blocking remain possible; unsuitable as the only bearer for arbitrary Mininet sessions.

Recommendation: use concepts from OHTTP for bridge descriptor retrieval, bootstrap queries, health measurements, and one-shot mailbox or directory operations. Do not use it as the primary Mininet bridge stream.

### 7.8 Domain fronting

A connection appears directed to an allowed domain at one protocol layer while infrastructure routes it to another service.

Strengths: can impose substantial collateral damage on blocking; historically useful for circumvention; can conceal service destination from a local observer under certain infrastructure assumptions.

Weaknesses: major cloud providers have restricted or disabled it; fragile provider dependency; contractual and operational risk; easy to lose abruptly; centralises circumvention on a few corporations.

Recommendation: do not make domain fronting a required Mininet mechanism. Permit provider-specific adapters where lawful and technically available, but classify them as optional and fragile.

### 7.9 Protocol mimicry

Traffic attempts to imitate another protocol such as video calling, web browsing, or gaming.

Strengths: can raise collateral blocking cost; may work against simple classifiers; can pass protocol allowlists if imitation is convincing.

Weaknesses: accurate mimicry is hard; malformed state transitions expose the tunnel; active probing reveals incomplete implementations; traffic behaviour differs from the claimed application; maintaining parity with a changing protocol is expensive.

Recommendation: avoid building a custom Mininet mimicry protocol in the first implementation. Use established transports with real deployment evidence.

---

## 8. Bridge discovery

Bridge distribution is at least as important as traffic transformation.

Tor bridges are deliberately not included in the public relay directory, making address-based blocking harder than for publicly listed relays. (Support)

However, "not public" does not mean "undiscoverable."

### 8.1 Distribution channels

Mininet should support: direct invitation; QR code; contact-to-contact transfer; signed community bundles; app-bundled emergency descriptors; email; web retrieval; OHTTP retrieval; Tor/I2P retrieval; local BLE exchange; removable media; governance-signed public emergency sets; ephemeral broker allocation; proof-of-work or rate-limited requests; anonymous credential-gated distribution.

### 8.2 Descriptor classes

**Public reserve bridge** — widely distributed and expected to be blocked first. Useful for initial bootstrap, low-capability censors, emergency fallback.

**Rate-limited bridge** — distributed through a service that limits enumeration.

**Invitation bridge** — shared within a trust or community graph.

**Single-use or short-lived bridge** — valid for a limited time or allocation window.

**Ephemeral proxy assignment** — provided by a broker immediately before use.

**Local bridge** — discovered over proximity and mutually authenticated locally.

### 8.3 Multi-distributor architecture

No single distributor should know every bridge, every client, every allocation, every region, or every active descriptor.

Potential model:

```
Bridge registry quorum
    → several distributors receive subsets
    → clients query one or more through privacy-preserving channels
    → descriptors have short validity and signed provenance
```

### 8.4 Enumeration resistance

Possible tools: per-request limits; proof-of-work; anonymous rate credentials; trust-graph invitations; regional allocation; bridge pools; staged release; short descriptor lifetimes; honey bridges; distributor diversity.

Every mechanism has exclusion risks.

Government ID, phone number, payment card, or global Mininet identity must not become mandatory for bridge access.

---

## 9. Bridge identity and rotation

### 9.1 Separate identity from endpoint

A bridge should have a cryptographic identity that survives IP rotation, port changes, transport changes, domain changes, and hosting migration.

Clients authenticate the bridge key, not merely the endpoint.

### 9.2 Scoped bridge keys

A bridge may use separate keys for descriptor signing, transport handshake, Mininet inner handshake, telemetry, and distributor registration.

This reduces cross-system linkage and limits compromise.

### 9.3 Rotation policy

Rotate transport secrets, descriptor nonces, endpoints, domains where needed, outer certificates, and ephemeral handshake keys.

Do not rotate the root bridge identity so frequently that trusted continuity becomes impossible.

### 9.4 Compromise response

Descriptors should support expiry, revocation, supersession, emergency withdrawal, replacement chains, and client grace windows.

A revoked bridge must not become a downgrade path.

---

## 10. Local Wi-Fi and Bluetooth bridge mode

Local bridging is one of Mininet's distinctive opportunities because the project already treats BLE and local Wi-Fi as permanent core bearers rather than temporary bootstrap hacks.

### 10.1 Local use cases

One phone has working mobile data; another SIM or network blocks Mininet; external internet is cut but a local mesh or hotspot exists; community devices exchange bridge descriptors; a traveller receives connectivity from a trusted nearby user; a courier carries queued objects between disconnected regions.

### 10.2 Pairing

Use proximity, QR confirmation, short authentication strings, existing device delegation, channel binding, and fresh nonces. Avoid automatic unauthenticated forwarding.

### 10.3 Privacy

The local bridge should learn only that a nearby device requested forwarding, resource usage, and the next Mininet entry or opaque tunnel endpoint where unavoidable.

It should not receive application plaintext, global DID, destination service, social relationship, or object type.

### 10.4 Abuse limits

Local bridges need bounded queues, user approval, bandwidth caps, battery caps, expiry, no default internet exit, no arbitrary destination proxying, Mininet-only inner handshake, and rate controls.

This keeps the feature from becoming a general open proxy.

### 10.5 Store-and-forward

When no live internet path exists, the bridge may accept opaque bounded bundles for later forwarding.

This is distinct from a live pluggable transport and should be represented as delay-tolerant carriage.

---

## 11. Transport selection policy

### 11.1 Inputs

The selector should consider: required censorship resistance; active-probe risk; local observer risk; latency limit; bandwidth budget; battery budget; available bridge descriptors; recent regional success; transport implementation availability; user-configured exclusions; deadline.

### 11.2 Example order

Under light blocking: private direct bridge → WebTunnel → obfs4 → Snowflake → Tor compatibility → local bridge.

Under active probing and protocol allowlisting: WebTunnel → Snowflake → trusted invitation bridge → Tor transport → local bridge.

There must not be one fixed universal order.

### 11.3 Parallel racing

Racing several transports can improve reliability but leaks that the client is attempting circumvention, multiple destinations, bridge inventory, and timing relationships.

Use bounded staggered racing: select a small candidate set; randomise order; start one; start another after a policy delay; cancel safely after success.

### 11.4 Downgrade rules

A request requiring entry-obfuscation must not silently fall back to direct public Mininet TCP.

The selector returns achieved transport class, residual risk, cost, and failure reason.

The user or calling policy decides whether weaker fallback is permitted.

---

## 12. Traffic-shape policy

Pluggable transports often focus on handshake camouflage, while flow shape remains classifiable.

MN-207 should therefore define optional shaping profiles: None, PaddedHandshake, InteractiveWebLike, FixedBurst, LowRateBackground, HighRiskEntry.

These names should remain internal policy classes until measured.

### 12.1 Padding

Padding can reduce handshake-size signatures, first-flight signatures, exact message-length leakage, and record-boundary fingerprints.

### 12.2 Timing

Timing perturbation can reduce simple classifiers but adds latency and may create a new unique pattern.

### 12.3 Multiplexing

Multiple logical Mininet streams over one outer connection can reduce handshake exposure and make traffic more web-like, but can also create long distinctive sessions.

### 12.4 Cover traffic

Entry cover traffic is expensive and should be pooled or scheduled under the cost doctrine.

It must not create a rare "high privacy user" pattern. The research explicitly warns that privacy mechanisms used only by a small premium population become fingerprints.

---

## 13. Measurement and adaptation

### 13.1 Why measurement is required

A transport may appear blocked because of bridge outage, DNS failure, TLS interception, NAT, packet loss, broker failure, implementation bug, or actual censorship.

Research comparing pluggable transports found significant variation in performance and reliability, meaning client selection cannot assume all PT failures represent blocking or that all transports perform similarly. (arXiv)

### 13.2 Local measurements

Clients may record, with privacy-preserving local storage: success/failure; coarse latency; handshake stage; network type; transport; descriptor age; failure class; time bucket.

Do not record destination application or user identity.

### 13.3 Shared measurements

Optional aggregate reporting should use coarse region, time buckets, differential privacy or minimum cohort thresholds, no client identifier, no complete bridge address, no exact failure trace, delayed reporting, and transport-level statistics only.

### 13.4 OONI compatibility

OONI's model demonstrates the usefulness of distributed network measurements for understanding censorship.

Mininet should publish a test specification that independent tools can run without joining the user network.

The measurement path must not become a bridge-enumeration oracle.

---

## 14. Active-probing resistance

### 14.1 Secret-gated response

A bridge should not reveal Mininet behaviour unless the client proves possession of transport-specific secret material or completes an authenticated outer handshake.

### 14.2 Uniform rejection

Unknown or invalid clients should receive ordinary web behaviour, a generic close, randomised timing within safe bounds, no Mininet error code, and no stable protocol banner.

### 14.3 Decoy service

WebTunnel-style deployments should serve plausible ordinary content at the same endpoint.

The decoy must be operationally real enough that paths exist, TLS setup is ordinary, error pages are normal, HTTP methods behave plausibly, and automated scanners do not immediately identify an empty tunnel host.

### 14.4 Probe correlation

A censor may observe a suspected client, then immediately probe the destination.

Bridges should avoid client-triggered externally visible state changes that reveal successful Mininet use.

---

## 15. Bootstrap resilience

A fresh installation is most vulnerable because it has no trusted descriptors.

### 15.1 Bundled bootstrap set

The application may ship public reserve bridges, transport broker keys, distributor keys, Tor/I2P bootstrap configuration, trusted directory quorum keys, and local exchange protocol.

Bundled addresses will eventually be blocked. They are seeds, not permanent infrastructure.

### 15.2 Update channels

Descriptor updates may arrive through normal application update, signed remote bundle, Tor, I2P, email, QR, Bluetooth, local Wi-Fi, community social channels, or removable media.

### 15.3 Multiple roots of discovery

No one company domain, GitHub repository, DNS zone, app store, or CDN should be the only route to fresh descriptors.

### 15.4 Offline verification

All descriptor bundles must be verifiable offline through pinned governance or distributor keys.

---

## 16. Abuse and resource controls

Bridges can be attacked through bandwidth exhaustion, connection floods, descriptor harvesting, proxy misuse, malformed handshakes, replay, and CPU exhaustion.

Controls should include: bounded pre-authentication work; stateless cookies where appropriate; per-source connection limits; anonymous rate credentials; optional proof-of-work; transport-secret validation; bounded handshake parsing; queue limits; timeouts; circuit limits; no arbitrary open-proxy behaviour.

Do not require globally identifying accounts.

The founder research already recommends anonymous rate credentials, service-chosen proof-of-work or postage, bounded queues, and capability quotas for denial-of-service control.

---

## 17. Technologies considered

**Solution A — one custom Mininet obfuscation protocol.** Reject: fixed fingerprint; high research burden; easy active-probing mistakes; no deployment history.

**Solution B — direct TLS bridges only.** Support only as the cheapest bridge class.

**Solution C — obfs4 only.** Adopt, but not alone.

**Solution D — WebTunnel only.** Adopt as one transport in the portfolio.

**Solution E — Snowflake only.** Adopt as fallback through an existing implementation.

**Solution F — Tor PT compatibility plus native adapter trait.** Recommend.

**Solution G — MASQUE as the universal answer.** Use as an optional carrier, not the doctrine.

**Solution H — local-only mesh bridging.** Adopt as a distinct local bearer, not a substitute for internet bridges.

---

## 18. Recommended implementation phases

**Phase 1 — doctrine and interfaces.** Produce `docs/design/bridge-pluggable-transport.md`: threat model; transport trait; bridge descriptor; authentication boundary; discovery boundary; policy inputs; residual floors; supported initial adapters; no-undetectability claim.

**Phase 2 — direct private bridge.** Implement signed descriptors; unpublished endpoint; native Mininet inner handshake; expiry and rotation; typed transport policy; strict downgrade prevention. This proves the framework without claiming strong camouflage.

**Phase 3 — Tor PT subprocess adapter.** Integrate a mature PT runtime for obfs4/Lyrebird, WebTunnel, Snowflake where supported. Security requirements: no shell command strings from descriptors; fixed executable allowlist; bounded environment; subprocess sandbox; lifecycle timeouts; no inherited unnecessary file descriptors; structured IPC; redacted logs.

**Phase 4 — native mobile adapters.** Implement or bind audited libraries for obfs4, WebTunnel, Snowflake. Do not reimplement cryptography merely for in-process convenience.

**Phase 5 — local bridge.** Add BLE pairing, local Wi-Fi forwarding, bounded Mininet-only carriage, store-and-forward option, user controls, no generic internet proxy.

**Phase 6 — distribution.** Implement several independent descriptor channels: signed web bundle; QR; contact invitation; local exchange; Tor retrieval; app reserve set.

**Phase 7 — measurement.** Build synthetic censor lab, regional test harness, classifier evaluation, active-probing tests, descriptor-enumeration simulation, failure-class telemetry, transport-performance comparison.

**Phase 8 — pilot.** Run a labelled experimental pilot with multiple providers, multiple transport families, independent bridge operators, published measurements, explicit limitations, emergency withdrawal and rotation.

---

## 19. Adversarial tests

**Descriptor tests:** unknown descriptor versions rejected; expired descriptors rejected; modified endpoints fail signature verification; a descriptor for one transport cannot be interpreted as another; arbitrary subprocess commands cannot enter a descriptor; oversized parameters rejected before allocation; duplicate/conflicting fields rejected; revoked bridge keys rejected; superseded descriptors cannot downgrade to older transport secrets; offline descriptor verification succeeds without contacting a central service.

**Transport tests:** invalid clients do not trigger a Mininet-specific response; active probes without the bridge secret receive decoy/generic behaviour; outer transport success without inner bridge authentication fails; inner handshake downgrade is rejected; transport failure does not silently permit forbidden direct fallback; cancelling a raced transport erases temporary secrets; logs contain no bridge secret/user DID/destination; malformed handshake floods remain within CPU and memory bounds; replay of an outer handshake fails; transport adapters cannot request arbitrary destination proxying.

**Distribution tests:** one compromised distributor cannot replace descriptor signatures; one blocked distributor does not remove all bootstrap paths; a malicious requester cannot enumerate an entire bridge pool under expected limits; bridge allocations expire; QR and offline bundles verify without internet access; app-bundled reserve bridges can be superseded; regional bridge allocation does not encode a stable client identity; anonymous rate limits do not require global identity.

**Local bridge tests:** unpaired devices cannot forward; a local bridge cannot read the Mininet inner payload; a local bridge cannot redirect the inner handshake to another bridge undetected; forwarding is bounded by user policy; local bridge mode cannot become a generic SOCKS or HTTP proxy; store-and-forward bundles expire; BLE and Wi-Fi sessions are channel-bound; replayed pairing messages fail.

**Policy tests:** required obfuscation never falls back to public direct transport; a latency-constrained request receives an honest weaker-result report rather than a false strong label; blocked transports are deprioritised without permanent global disablement; selection order is not globally deterministic; bridge freshness affects selection; the reported result states the actual transport used; regional measurement data cannot override explicit user exclusions.

---

## 20. Residual privacy and censorship floors

* A censor can block enough collateral infrastructure that circumvention fails. MN-207 raises the censor's cost; it does not remove its power.
* A bridge sees the client IP unless the client reaches it through another privacy layer.
* Traffic classification improves over time; transport agility must remain permanent.
* Bridge distribution can be infiltrated.
* Small communities sharing a bridge are vulnerable to membership disclosure.
* Local bridges reveal proximity, timing, approximate volume, and device characteristics.
* Endpoint compromise defeats every transport.
* User behaviour patterns can identify use despite successful transport camouflage.

---

## 21. Recommended design-note text

**Status:** bridge and pluggable-transport architecture. No claim of undetectable communication.

**Decision:** Mininet will expose a typed pluggable-transport interface between clients and entry relays. The initial portfolio will support private direct bridges, obfs4/Lyrebird-compatible transport, WebTunnel-style HTTPS carriage, Snowflake-style ephemeral proxies, Tor/I2P compatibility, and local BLE/Wi-Fi bridge forwarding.

Bridge discovery, traffic camouflage, byte carriage, and transport health are separate protocol concerns.

**Security boundary:** every outer transport terminates into a Mininet-authenticated inner handshake. The outer transport does not define bridge identity or Mininet authorization.

**Distribution boundary:** bridge descriptors are signed, versioned, bounded, expiring, and obtainable through multiple independent channels. No single public directory is mandatory.

**Policy boundary:** applications request transport properties. They do not select raw transport implementations directly.

**Hard limitation:** no supported transport is guaranteed to be unidentifiable or unblockable. The system raises censorship cost through diversity, address agility, active-probing resistance, collateral resistance, and local alternatives.

---

## 22. Proposed decision-log entry

**Decision:** Mininet will use a portfolio-based bridge and pluggable-transport architecture for censored entry. The core relay protocol remains transport-independent and authenticates itself inside the chosen outer transport.

Initial supported classes are private direct bridges, obfs4/Lyrebird-compatible obfuscation, WebTunnel-style HTTPS carriage, Snowflake-style ephemeral volunteer proxying, Tor/I2P compatibility, and BLE/local-Wi-Fi bridge forwarding.

**Reason:** no single circumvention transport remains effective against every censor. A modular portfolio allows regional adaptation, rapid replacement, diverse bridge distribution, and reuse of proven systems without making any external network the permanent Mininet substrate.

**Constitutional impact:** strengthens censorship resistance; preserves owner choice of bearer; preserves BLE and local Wi-Fi as permanent core options; creates no mandatory central relay directory; introduces no global transport identity; does not change governance or validator weight; does not claim anonymity beyond the actual selected route.

**Failure point:** the design fails if all adapters share one recognisable outer fingerprint, bridge distribution becomes centrally enumerable, downgrade silently selects direct transport, or one provider becomes the mandatory bootstrap and carriage path.

**Required follow-up:** transport adapter security review; active-probing test lab; descriptor-distribution simulation; mobile integration; local bridge prototype; regional pilot; published residual-risk and blocking measurements.

---

## 23. Final recommendations

**Adopt:** a typed native pluggable-transport trait; Tor PT subprocess compatibility for desktop/research; an authenticated Mininet inner handshake independent of camouflage; signed, expiring bridge descriptors; private direct bridge mode; obfs4/Lyrebird adapter; WebTunnel adapter; Snowflake adapter; Tor and I2P compatibility bearers; BLE and local Wi-Fi bridge forwarding; multiple bridge-distribution channels; offline QR and contact invitation; no single mandatory directory; transport-specific observability and cost metadata; policy-based adaptive selection; strict downgrade prevention; bounded staggered transport racing; active-probing tests; regional performance measurements; bridge rotation and revocation; explicit residual-risk reporting; a clear statement that camouflage is not anonymity.

**Defer:** custom domain-fronting infrastructure; new protocol-mimicry research; universal MASQUE deployment; fully decentralised bridge allocation; PIR-based bridge distribution; anonymous e-cash for bridge admission; post-quantum transport handshakes; satellite/broadcast bootstrap; large-scale offline courier routing; automatic global bridge reputation; machine-learning transport selection until sufficient safe data exists; one protocol claiming to replace all PTs.

**Reject:** one fixed custom Mininet disguise; publicly listing every bridge; treating TLS as censorship resistance by itself; treating MASQUE as inherently unobservable; treating WebRTC as automatically indistinguishable; reimplementing obfs4/Snowflake/WebTunnel cryptography without need; generic executable commands in descriptors; shell-based PT invocation from untrusted config; global DID in bridge requests; phone-number or government-ID bridge access; silent downgrade to direct transport; one mandatory broker/CDN/DNS zone; detailed probe errors; local bridge as an unrestricted open proxy; claims that Mininet becomes unblockable; claims that bridge use hides the client from the bridge; claims that transport camouflage defeats endpoint compromise; claims that a currently successful PT will remain successful permanently.

---

## 24. Essay: Censorship Resistance Is the Ability to Change Shape

A network protocol has two lives.

One is the life its designers describe: message formats, keys, routes, identities, and guarantees.

The other is the life an observer sees: destination addresses, packet timings, certificates, retries, ports, lengths, and patterns.

A system can be cryptographically correct in its first life and trivial to block in its second.

This is the problem MN-207 must solve.

The goal is not to make Mininet traffic look like nothing. Nothing is not a network protocol. Every connection consumes addresses, packets, time, and bandwidth. The goal is to prevent one stable observation from becoming a permanent global blocking rule.

A public relay has an obvious weakness. Its address is known because users must learn it. A censor downloads the same list, blocks every address, and the system disappears.

A private bridge changes that economy. Its address is not universally published. The censor must discover it through infiltration, scanning, observation, or coercion. Tor's bridges use this basic principle: they are ordinary relays removed from the public directory so blocking requires more than downloading a complete list. (Support)

But secrecy of address is temporary. A bridge used by real people eventually leaves evidence. A censor can request bridges, observe connections, probe suspicious hosts, or compromise distribution channels.

This is why obfuscation was added. The censor should not be able to scan every server and ask a simple question whose answer is "yes, I am a bridge." A secret-gated transport such as obfs4 attempts to make an unauthorised probe indistinguishable from an ordinary failed connection while transforming valid traffic into a less recognisable form.

Still, random-looking bytes are not universally safe. A network that permits only approved protocols can block every unknown encrypted stream. In that environment, a transport that behaves like ordinary HTTPS may survive longer because blocking it risks harming normal web use.

Yet mimicry creates another trap. It is easy to wrap traffic in TLS and call it web-like. It is much harder to reproduce the full behaviour of real web software: handshake fingerprints, request sequences, packet sizes, idle periods, errors, and server responses. A fake protocol may fool a simple filter while remaining obvious to a determined classifier.

Snowflake chooses a different axis. Rather than relying only on one bridge remaining hidden, it recruits many short-lived volunteer proxies. The address set changes continually. The censor can still classify the protocol or interfere with its broker, but simple IP enumeration becomes more expensive.

This diversity reveals the central rule:

A censorship-resistant network should not have one shape.

One network may permit HTTPS but block random traffic. Another may block Tor and VPN protocols but permit WebRTC. Another may allow only domestic services. Another may cut external internet while leaving local Wi-Fi available. Another may use active probes. Another may rely mostly on DNS poisoning.

No single transport dominates all these environments.

Therefore the Mininet core should not know that it "runs over obfs4" or "runs over WebTunnel." It should know that it needs an authenticated channel with particular observable and operational properties. The pluggable transport supplies that channel. The privacy-policy router decides which available mechanism best matches the threat, latency, cost, and failure history.

This also keeps camouflage in its proper place.

A WebTunnel server is not the user's identity authority. A Snowflake proxy is not a trusted Mininet relay. A Tor circuit does not grant Mininet capabilities. An obfs4 secret does not establish application authorization.

After the outer transport succeeds, Mininet must authenticate the intended bridge through its own inner handshake. Otherwise, whoever controls a decoy web server, volunteer proxy, or local forwarder could impersonate the network.

Bridge discovery must be equally plural.

A system that supports five transports but publishes all descriptors through one website still has one point of failure. A censor blocks the site and the transport diversity becomes irrelevant.

Descriptors should move through websites, email, Tor, I2P, QR codes, local contacts, BLE, removable media, and signed application bundles. Some paths can be public and disposable. Others can be private and scarce. Some can use anonymous rate limits. Some can rely on existing trust relationships.

None should require a passport, phone number, payment card, or permanent Mininet identity. Such requirements might slow bridge enumeration, but they would turn censorship circumvention into identity surveillance and exclude precisely the users who need it most.

Local bridging deserves special attention.

Most circumvention systems assume the censored device must itself reach an internet bridge. Mininet already treats proximity networking as part of its permanent substrate. A phone with a blocked SIM may reach another phone over Bluetooth. A community hotspot may accept opaque Mininet bundles. A traveller may receive a signed bridge descriptor by QR code. A disconnected region may carry encrypted objects outward through devices that later reconnect.

This does not eliminate trust. The local bridge sees proximity and timing. It may know the user personally. It can drop traffic. But it need not read the object or know the final service. The same principle of role separation used in Tier 1 can begin at arm's length.

Transport agility must also include the ability to admit failure.

A client should not display "connected privately" merely because some tunnel opened. It should report which transport actually succeeded and what that means.

A direct private bridge hides the server from public directories but may remain fingerprintable. obfs4 resists certain probes but may be blocked as unknown encrypted traffic. WebTunnel may blend with HTTPS but remains domain-dependent. Snowflake changes proxy addresses rapidly but relies on brokers and WebRTC behaviour. Tor adds an existing network but introduces its own blocking surface. BLE avoids the ISP but exposes physical proximity.

These are not defects to conceal. They are coordinates on Mininet's cost and risk curve.

The censor is adaptive. Successful circumvention creates an incentive to classify, infiltrate, and block it. Therefore MN-207 cannot be a feature that is implemented once and declared complete. It must be a maintained boundary where new transports can enter and failed transports can leave without rewriting the relay, mailbox, object, or application layers.

The lasting defence is not one perfect disguise.

It is the ability to change shape faster than the censor can make one blocking rule universal, while ensuring that every new shape still carries the same authenticated Mininet protocol inside.

That is what a pluggable transport architecture buys: not invisibility, but mobility in protocol space.

And in censorship resistance, the ability to move is often the difference between a network that exists only on paper and one people can still reach.

MN-207 should therefore become a portfolio architecture and bridge-distribution doctrine, followed by adapters to proven systems. The strongest initial engineering sequence is: private bridge framework → Tor PT compatibility → obfs4/Lyrebird → WebTunnel → Snowflake → BLE/local-Wi-Fi bridge → multi-channel descriptor distribution and measurement.

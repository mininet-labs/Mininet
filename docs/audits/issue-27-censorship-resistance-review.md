# Issue #27 — censorship-resistance review

**Decision track:** D-0377.  
**Implementation reviewed:** PR #292 (`mini-bearer`, `mini-net`,
`mini-transport-security`, `mini-relay`, and `mini-transport-policy`).  
**Scope:** state-level blocking, DPI/protocol fingerprinting, relay/bridge
blocking, throttling/blackholing, bootstrap capture, and traffic analysis.

This is an engineering threat assessment, not an anonymity certification. A
mechanism receives PASS only where executable code and permanent tests prove the
claim. A policy name or research document is not runtime evidence.

## Verdict matrix

| Value / threat | Verdict | Mechanism and exact failure | Long-term engineering solution |
|---|---|---|---|
| Payload confidentiality and integrity on a direct link | **PASS** | `mini-bearer::Channel` uses a fresh X25519 handshake, HKDF, and ChaCha20-Poly1305 with ordered nonces. Real-socket tests prove plaintext is not sent directly and tampering fails. Endpoint identity remains optional and separate. | Preserve anonymous CH1 as the minimum bearer and keep every new bearer behind the same bounded channel interface. |
| Optional endpoint authentication without a central naming authority | **PASS** | `mini-transport-security::SessionAuthClaim` signs the exact CH1 channel binding, role, typed purpose, pairwise/root DID, delegated device, X25519 routing key, validity window, and replay nonce. Verification uses caller-held KELs, delegation capability checks, and local highest-sequence pins. There is no CA, DNS authority, TOFU record, hosted identity directory, or global login service. | Add witness receipts and gossip freshness so first-contact verifiers can discover later revocations without a central directory. |
| No administrative or legal unmasking path | **PASS** | No admin key, recovery key, traffic master key, escrow key, identity-correlation service, or protocol command can decrypt another user's CH1 or onion payload. Pairwise identities remain valid endpoints. | Keep this as a compiled invariant: no transport or privacy crate may depend on an authority/unmasking interface. |
| Signed discovery records | **PASS** | `PeerAdvertisement` binds network id, address, endpoint id, delegated device, X25519 routing key, validity window, and nonce. A dialed peer must prove the same identity/key on the live channel through `verify_advertised`; a signed redirect or unrelated genuine endpoint is rejected. | Persist freshness/replay state in clients and add multi-source observation records without turning observation count into truth authority. |
| Bootstrap and eclipse resistance | **PARTIAL** | Local-seeded selection is input-order-independent, rejects duplicate endpoints/routing keys, caps IPv4 `/24` and IPv6 `/48` concentration, and uses bounded timeouts. Exact failure: one adversary can still obtain many prefixes/ASNs or control all initial discovery sources; IP diversity does not prove operator independence. | Add independently observed ASN/jurisdiction/operator diversity, multiple unrelated discovery paths, local-contact exchange, and route rotation. Never use majority-by-download or a canonical bootstrap list. |
| Tier-1 relay payload separation | **PASS** | `mini-relay` now builds exactly `Entry -> Rendezvous -> Delivery` with independent ephemeral X25519/AEAD layers, independent public hop identifiers, padded opaque next-hop tokens, per-hop expiry/replay checks, and a destination-encrypted fixed-size payload. A real three-socket test proves each relay forwards ciphertext and only the destination opens plaintext. | Keep relay roles independently selectable and add authenticated routing-key acquisition from secure advertisements. |
| Relay IP blocking | **PARTIAL** | Routes can rotate among independently advertised relays; no canonical relay registry or mandatory operator exists. Exact failure: TCP relay addresses remain visible and individually blockable; a censor that learns enough active addresses can deny availability. | Add private bridge distribution through pairwise invitations/local transfer, multiple bearer adapters, rapid route rotation, and non-public bridge pools. No single bridge distributor may be mandatory. |
| Protocol fingerprinting and DPI resistance | **FAIL** | CH1 and onion payloads are encrypted, but handshake lengths, TCP framing, packet-size classes, timing, and connection behavior remain recognizable. Encryption does not equal camouflage. | Implement a self-hostable pluggable-bearer interface with independently reviewed padding/camouflage profiles, randomized handshakes, and ordinary-protocol-shaped adapters. Avoid dependence on one commercial domain-fronting provider. |
| ISP throttling and blackholing resistance | **FAIL** | Current internet transport is TCP over known addresses. An ISP can throttle or blackhole those flows without decrypting them. Local BLE/Wi-Fi can bypass the ISP only when participants are physically or locally reachable. | Add multipath scheduling across local Wi-Fi, BLE, direct internet, private bridges, and future approved adapters; support resumable store-and-forward and route migration after stalls. |
| NAT and carrier-grade NAT reachability | **FAIL** | No production hole punching, rendezvous-assisted traversal, reconnect supervisor, or mobile background service exists. Most phones cannot reliably accept inbound internet connections. | Complete issue #24: authenticated rendezvous-assisted hole punching with relay fallback, bounded retries, address rotation, and no mandatory traversal operator. |
| Global timing, volume, intersection, and predecessor resistance | **FAIL** | Three-hop onion hides payload and separates endpoint knowledge, but fixed route length and observable timing/volume remain correlatable by a global or sufficiently broad observer. | Implement the already specified Sphinx/Loopix-style mix executor with fixed packets, stratified route selection, cover traffic, random delay, bounded replay state, and independent cryptographic review. |
| Mixed/Burst user-facing anonymity claim | **PASS (fail-closed)** | `mini_transport_security::executable_transport` refuses `PrivacyTier::Mixed` and `Burst`. The repository cannot silently route those tiers through Direct/Relayed and call the result anonymous. Exact failure: the intended mix capability is not operational. | Build the exact D-0305 executor, test it under hostile timing/volume conditions, obtain the external review required by #72, then change the gate in a separate reviewed decision. |
| App-store or binary-distribution blocking | **PARTIAL** | Reproducible builds and self-hosted Forge reduce trust in a single distributor. Exact failure: ordinary users still lack a mature, censored-region-safe side-loading and update-discovery path. | Finish peer-to-peer signed update distribution, offline transfer, multiple independent mirrors, and user-verifiable reproducible artifacts. |
| Local BLE/Wi-Fi blocking | **PARTIAL** | Local-first operation avoids an internet chokepoint, and store-and-forward can bridge intermittent access. Exact failure: OS policy, radio jamming, venue control, or device-vendor restrictions can suppress local bearers. | Maintain multiple interchangeable local bearers and test degraded operation under platform restrictions; never make one radio or vendor API authoritative. |

## State-censor attack paths

### 1. Block public relays and bootstrap peers

A censor can enumerate public endpoints, block their IPs, and poison or suppress
responses. Signed advertisements prevent redirection from becoming identity
authority, but signatures do not make an address reachable. Prefix-diverse local
selection raises the cost of capture; it does not solve public-endpoint
enumeration.

**Required design:** private bridge invitations, multiple unrelated discovery
sources, route rotation, local exchange, and no canonical list. A hosted bridge
directory would recreate the censorship point this work is intended to remove.

### 2. Fingerprint encrypted traffic

CH1 protects contents, not appearance. Stable framing, handshake order, packet
classes, burst patterns, and long-lived TCP behavior can be classified by DPI.
Padding only the application payload does not hide the transport transcript.

**Required design:** pluggable bearer adapters with explicit transcript-shape
profiles, bounded padding, timing jitter, and independent review. Camouflage
must remain optional and swappable; one cloud/CDN front is not acceptable as a
new root dependency.

### 3. Throttle rather than block

A censor can delay packets enough to break timeouts while avoiding a visible
hard block. Current bounded timeouts prevent an attacker from holding a task
forever, but can also make throttling an effective denial mechanism.

**Required design:** progress-aware timeout widening, resumable transfers,
multipath retries, and route migration. No peer's responsiveness may create
truth or governance weight.

### 4. Correlate entry and exit timing

Independent onion encryption prevents one relay from reading all layers, but a
broad observer can correlate when and how much data enters and leaves the
three-hop route.

**Required design:** the externally reviewed mix tier. Adding arbitrary delays
or ad hoc cover traffic to this PR would be inventing an anonymity protocol and
would create false confidence.

## Authority check

The implemented P0/P1 layer introduces **no centralized control point**:

- no certificate authority or mandatory name service;
- no canonical relay or bootstrap registry;
- no hidden administrator, freeze, kill, or unmasking key;
- no trusted first peer or majority-by-download rule;
- no payment, balance, storage, bandwidth, or service input into route,
  identity, personhood, validator, review, or governance authority;
- no requirement to reveal a global identity instead of a pairwise identity.

## Closure judgment

Issue #27's review deliverable is complete when this document and the D-0377
implementation merge. The censorship-resistance **system is not complete**:
DPI camouflage, ISP-throttling resistance, NAT traversal, private bridge
operations, and global-observer mix protection remain exact engineering work,
not marketing claims.

Refs #27, #24, #72, #291; D-0305, D-0306, D-0377.

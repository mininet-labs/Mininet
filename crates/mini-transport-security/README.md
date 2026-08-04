# mini-transport-security

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
- `diverse_dial_plan` is locally seeded, input-order-independent, rejects
  repeated endpoint ids, routing keys, visible roots, and visible devices, caps
  IPv4 `/24` and IPv6 `/48` concentration, and rejects more than 1,024 candidates
  before allocation/sort.
- `AuthenticatedConnection<B>` owns one bearer, the exact CH1 channel, and the
  peer verified on that channel as one object. It exposes authenticated `send`
  and `recv`, not detachable raw identity state, and permanently poisons itself
  after an ambiguous bearer/channel failure instead of risking counter reuse or
  stream desynchronization.
- `connect_authenticated_tcp` performs signed-advertisement dial, CH1, encrypted
  responder-first authentication, and exact advertisement/session binding.
  `connect_first_authenticated_tcp` retries a bounded local diverse plan and
  returns no partially accepted state from failed attempts.
- `authenticate_established_initiator` and
  `authenticate_established_responder` accept a channel established by any
  bearer, including `mini-bridge` adapters, without making the bridge an
  identity authority.
- `build_verified_onion_route` accepts three live same-network verified endpoints and
  rejects visible endpoint, routing-key, root, or device reuse before building
  the `Entry -> Rendezvous -> Delivery` onion in `mini-relay`. A permanent
  integration tests start with signed advertisements and local selection, then
  forward only ciphertext until the destination alone recovers plaintext. One
  full-chain test uses a typed `Relay`-purpose authenticated CH1 connection for
  client-to-entry, both relay-to-relay hops, and delivery-to-destination.
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
- The onion-v2 implementation in `mini-relay` protects payload confidentiality,
  separates endpoint knowledge, uses v2 cryptographic domains, bounds remaining
  lifetime with explicit clock-skew tolerance, retains a monotonic local
  time high-water mark against wall-clock rollback, and requires fail-closed
  relay/destination replay state. It is not Sphinx and does not
  defeat a global timing/volume observer; crash persistence and flood controls
  remain deployment responsibilities.
- The bridge seam reuses `mini-bridge::PluggableTransport` and
  `PtProcessManager`; no real obfs4/WebTunnel/Snowflake adapter is added here.
- NAT traversal, reconnect, private bridge distribution, multipath migration,
  and background service supervision remain deployment work.

See `docs/planning/privacy-transport-runtime-convergence.md`,
`docs/planning/privacy-transport-security.md`, and
`docs/audits/issue-27-censorship-resistance-review.md`.

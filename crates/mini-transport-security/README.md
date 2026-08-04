# mini-transport-security

Optional self-certifying endpoint authentication and secure peer-discovery
primitives above Mininet's anonymous `mini-bearer::Channel`.

## What is implemented

- `SessionAuthClaim` signs one exact CH1 channel binding with a delegated
  `did:mini` device for one endpoint role and typed transport purpose.
- Verification uses caller-supplied root/device KELs, delegation capability
  checks, highest-sequence freshness pins, bounded validity windows, and a
  bounded replay cache.
- `TransportEndpointId` binds the presented device or pairwise DID to its
  current X25519 routing key. Rotating the routing key rotates the endpoint id.
- `PeerAdvertisement` signs a network id, dial address, routing key, endpoint
  id, validity window, and replay nonce. Advertisements remain dial hints; the
  live CH1 session must still prove the same endpoint and routing key.
- `SecurePexResponse` carries a bounded canonical list of signed
  advertisements.
- `diverse_dial_plan` is locally seeded, input-order-independent, duplicate-
  resistant, and capped per IPv4 `/24` or IPv6 `/48` prefix.
- `executable_transport` permits the implemented Direct and Relayed executors
  and refuses Mixed/Burst until the exact mix executor receives independent
  review.

## Authority boundary

This crate creates no certificate authority, hosted directory, canonical relay
registry, hardcoded trusted peer, trust-on-first-use rule, admin key, recovery
key, identity-unmasking path, or download-majority rule. Payment, balance,
storage, bandwidth, and provider revenue are absent from every authority and
selection input.

Anonymous CH1 remains valid. A caller that needs unlinkability should present a
pairwise identity or use onion/mix routing rather than authenticating a global
root to every counterparty.

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
- NAT traversal, reconnect, pluggable/camouflaged transports, bridge
  distribution, and background service supervision remain deployment work.

See `docs/planning/privacy-transport-security.md` and
`docs/audits/issue-27-censorship-resistance-review.md`.

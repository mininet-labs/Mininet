# Privacy and transport security completion (#291)

**Status:** implementation in progress on `codex/privacy-transport-security`.
**Authority:** engineering work only. This document grants no production,
mainnet, anonymity, or external-audit claim.

## Current failure

The existing `mini-bearer::Channel` correctly provides anonymous,
forward-secret authenticated encryption, but deliberately authenticates no
endpoint identity. Legacy PEX exchanges unauthenticated dial hints. `mini-relay`
contains role, capability, and one-hop envelope primitives, but its live
store-and-forward path is not onion routing: an entry relay can see forwarded
application plaintext. `PrivacyTier::Mixed` describes a Sphinx/Loopix-style
mixnet, but no reviewed executor exists.

## Scope

### P0: optional self-certifying peer authentication

Add a layer above anonymous CH1 that binds a unique channel transcript to a
locally verified `did:mini` root/device delegation, a typed transport purpose,
a signed routing key, endpoint role, bounded validity window, and replay nonce.
The verifier uses its own KELs and persistent highest-sequence freshness pins.
There is no certificate authority, DNS authority, hosted identity directory,
trust-on-first-use rule, or mandatory disclosure of a global root. Anonymous
CH1 remains available for onion/mix hops; a pairwise root plus delegated device
is a valid authenticated endpoint.

### P0: signed discovery and eclipse resistance

Add versioned, expiring, network-bound peer advertisements. The signing device
binds a dial address and X25519 routing key to its self-certifying endpoint id.
Advertisements are availability hints only: a dial still has to complete the
channel-bound authentication exchange. Candidate selection is bounded,
input-order-independent, locally seeded, duplicate-resistant, and capped per
IPv4 /24 or IPv6 /48 prefix. No peer count, download majority, payment, stake,
storage, or bandwidth becomes authority.

### P1: three-hop onion execution

Add an exact three-role route (`Entry -> Rendezvous -> Delivery`) with one
independent ephemeral X25519 agreement and AEAD layer per hop. Each hop learns
only its own role, connection id, next-hop opaque token, expiry, and the next
ciphertext. The final delivery relay receives only a destination-encrypted
fixed-size payload. Role, connection id, size class, hop index, routing key,
nonce, next-hop token, expiry, and replay token are authenticated. Bounded replay
caches, fixed frame classes, strict decode caps, and all-or-nothing processing
are mandatory.

This is a compact Mininet onion format, not a claim to implement Sphinx or to
resist a global timing observer. The future mix executor remains a separate,
externally reviewed implementation of the existing Sphinx/Loopix research
profile.

### P1/P2: execution gate

Direct authenticated transport and Tier-1 onion relay may become executable
after tests and review. `Mixed` and `Burst` must fail closed in every execution
entry point until the exact mix implementation receives the external review
required by issue #72 and D-0305. A policy document naming mechanisms is not
runtime evidence that those mechanisms exist.

## Frozen boundaries

- No admin, law-enforcement, recovery, or operator unmasking key.
- No global traffic key, identity-correlation service, forced root disclosure,
  or protocol path that links pairwise identities.
- No certificate authority, canonical relay registry, hosted directory,
  hardcoded trusted peer, or mandatory bootstrap operator.
- No value input affects routing authority, discovery authority, review
  authority, personhood, validator weight, or governance.
- Availability records never become truth. KEL/delegation verification,
  channel transcripts, signed objects, state commitments, and consensus proofs
  remain authoritative in their own domains.

## Explicit residual floors

- A first-contact verifier that has never observed a fresher KEL cannot detect
  an unknown later revocation without witness evidence. Highest-sequence pins
  stop rollback below already observed state; they do not invent global
  freshness.
- Prefix diversity raises eclipse cost but does not prove independent ownership,
  ASN, or jurisdiction. Those dimensions need independently verifiable metadata,
  not self-asserted labels.
- Onion routing hides payload and separates endpoint knowledge. It does not by
  itself defeat timing, volume, intersection, predecessor, congestion, or global
  passive-observer correlation.
- Transport endpoint authentication reveals the presented pairwise/root and
  device to that counterparty. Callers needing unlinkability must use pairwise
  identities or anonymous onion/mix hops, not a global root.
- NAT traversal, reconnect, bridge/pluggable transports, background daemon
  supervision, and hostile-country censorship measurements remain separate
  deployment work.

## Merge floor

1. Permanent code and adversarial tests for every claim above.
2. Canonical bounded wire formats and exact replay/freshness semantics.
3. Real-socket tests for authenticated exchange and three-hop onion forwarding.
4. Decision/status/threat-model truth sync and regenerated repository navigation.
5. Full formatting, strict Clippy, workspace tests, dependency policy,
   governance, reproducibility, and Android workflows green at the final SHA.
6. Human review under the applicable repository rule. AI output has zero
   approval weight.

Refs #291, #24, #27, #72.

# Bridge / pluggable transport (MN-207, D-0309)

**Status:** Phase 0 (doctrine) + Phase 1 (typed interfaces) + one real
Phase 2 implementation shipped. Everything past that is deferred.

**Full research:** `docs/research/
MN207_BRIDGE_PLUGGABLE_TRANSPORT_RESEARCH_20260714.md` (founder-supplied,
2026-07-14). This document does not reproduce that report — it records
what was actually built from it and why, and links back for the full
threat model, transport-by-transport evaluation, and phased rollout the
report itself lays out.

## Decision

MN-207 is not "invent a Mininet obfuscation protocol." It is a small,
typed pluggable-transport interface — [`mini-bridge`] — that lets this
workspace reach relay/rendezvous services (`mini-relay`) through multiple
censorship-resistant entry mechanisms without coupling the core network
to any one disguise. Adapters to proven external systems (obfs4,
WebTunnel, Snowflake, Tor pluggable transports) are named in the type
system today but not implemented — each needs either an audited external
implementation this workspace would compose, or protocol work out of
scope for this decision.

## What's implemented

- `TransportId` — nine wire-stable, `#[non_exhaustive]` transport-kind
  tags the research report identifies (direct/QUIC/obfs4/WebTunnel/
  Snowflake/Tor/I2P/local-BLE/local-Wi-Fi).
- `TransportCapabilities`/`capabilities_for` — declared (not measured)
  policy facts per transport: stream/datagram support, active-probe
  resistance class, address agility, domain/broker dependency, local-only
  support, and a coarse overhead class. Populated for every named
  transport, including ones with no adapter yet, since policy code needs
  to reason about the whole portfolio.
- `BridgeDescriptor` — a self-signed, one-party reachability claim
  (bridge identity + transport + opaque endpoint + opaque transport
  parameters + optional distributor scope + validity window). Unlike
  this workspace's two-party capability grants (`mini_objects::
  CapabilityGrant`, `mini_relay::MailboxGrant`), nobody countersigns a
  bridge's claim about its own reachability — trust comes from how a
  descriptor was obtained, not from a second signature.
- `PluggableTransport` — the synchronous adapter trait (this workspace
  has no async runtime anywhere; `mini_bearer::Bearer` is the existing
  sync-trait precedent).
- `DirectBridgeTransport` — the one real, tested implementation: dials a
  real TCP socket and performs a genuine `mini_bearer::Channel` handshake
  (X25519 + HKDF-SHA256 + ChaCha20-Poly1305), verifying the descriptor's
  signature and validity window strictly before the socket is touched.

## What's deferred

obfs4/Lyrebird, WebTunnel, Snowflake, and Tor pluggable-transport
subprocess adapters (research report Phases 3-8) — each needs either an
audited external implementation this workspace would compose, or design
work not attempted here. Local BLE/Wi-Fi bridging depends on hardware
this environment cannot exercise (mirrors `mini-presence`'s existing
honest limits). Bridge-distribution channels, measurement/probing
detection, and traffic-shape policy are all named in the research report
but not built.

## Honest naming

`TransportId::DirectTlsV1`'s `Tls` is a wire-tag label carried over from
the research report's taxonomy — **not** a claim that `DirectBridgeTransport`
speaks real TLS. It composes this workspace's existing, already-reviewed
`mini-bearer` channel instead of adding a new TLS dependency, consistent
with CLAUDE.md's no-new-cryptography rule. A real TLS-mimicking transport
is future work this name reserves space for, not a claim this crate
makes today.

## What this does not provide

No measured probe resistance (only declared policy facts), no bridge
distribution mechanism, no transport-selection policy engine, no traffic
shaping, and no defense against an adversary who already knows a given
bridge's address and actively probes it beyond whatever
`active_probe_resistance` a transport declares for itself.

# External bridge adapter integration (post-MN-207, D-0097)

**Status:** Doctrine (this document) plus the generic Tor PT v1 process
manager (`mini-bridge::pt_process`) shipped in this PR. No real
circumvention binary (Lyrebird, WebTunnel, Snowflake) is integrated yet —
that is each its own future PR, per the phased plan below.

**Full research:** `docs/research/
BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md` (founder-supplied,
2026-07-15). This document does not reproduce that report — it freezes
the doctrine and records what's shipped vs. deferred, and links back for
the full comparison of subprocess vs. library-binding vs. reimplementation
for each of Lyrebird/obfs4, WebTunnel, Snowflake, and Tor.

## Decision

MN-207 (D-0309) named `Obfs4V1`/`WebTunnelV1`/`SnowflakeV1`/`TorStreamV1`
as reserved `TransportId` variants with no adapter. This decision fixes
*how* Mininet will eventually reach them: a restricted managed-subprocess
boundary implementing a strict client-side subset of the Tor Pluggable
Transport v1 protocol — never vendoring or reimplementing obfs4,
WebTunnel, or Snowflake's protocols in-house, and never shelling out to
an unconstrained command.

## The one rule everything else follows

**No shell, ever.** A managed PT process is spawned as
`execve(pinned_absolute_executable, fixed_argument_vector,
minimal_environment, dedicated_working_directory)` — never
`sh -c "<anything>"`. `std::process::Command::new(path)` never invokes a
shell on any platform this workspace targets, so this is enforced by
construction, not by convention: there is no code path in
`mini-bridge::pt_process` that builds a command line string and hands it
to a shell interpreter.

## Trust boundary (the second most important rule)

The PT process is network-facing and treated as potentially compromised.

**Trusted to:** transform bytes, connect to the configured bridge
endpoint, report transport-level readiness (a local loopback listener
address).

**Never trusted to:** authenticate the Mininet bridge, choose the
destination, choose privacy policy, touch identity keys, read
application plaintext, touch capabilities, modify governance state, or
select a weaker fallback. Every PT connection — once implemented in a
later PR — terminates into a **separate, independently authenticated**
Mininet bridge handshake (the same `mini_bearer::Channel` handshake
`DirectBridgeTransport` already performs). A compromised or malicious PT
process cannot silently redirect a client to a fake bridge, because the
inner handshake would simply fail.

## What's implemented in this PR: the generic PT process manager

`mini-bridge::pt_process` implements exactly the research report's PR2
scope (§18, §24) — a supervision layer with **no real PT dependency**:

- `VerifiedExecutable` — a pinned absolute path plus an expected BLAKE3
  digest (via `mini-crypto`, already in-tree — no new dependency).
  Verification hashes the file at the pinned path and compares before
  any spawn is attempted; a mismatch fails closed
  (`BridgeAdapterFailure::ExecutableDigestMismatch`) and nothing is
  executed.
- `PtProcessManager` — spawns via `std::process::Command` with a fixed
  argument vector and a minimal, explicitly-constructed environment
  (only the Tor PT v1 control variables:
  `TOR_PT_MANAGED_TRANSPORT_VER`, `TOR_PT_CLIENTTRANSPORTS`,
  `TOR_PT_STATE_LOCATION`) — no inherited environment, no shell, no
  PATH lookup (the executable path is always absolute).
- A minimal Tor PT v1 stdout line parser: `VERSION <n>` / `VERSION-ERROR`,
  `CMETHOD <name> <protocol> <addr:port>`, `CMETHODS DONE`,
  `PROXY-ERROR`/`ENV-ERROR` — the strict client-side subset the report's
  §18 names as required, not the full spec (server mode, extended
  ORPort, every optional variable are all explicitly deferred).
- `BridgeAdapterFailure` — the report's own suggested taxonomy (§20),
  narrowed to what a process manager alone can actually observe:
  `ExecutableUnavailable`, `ExecutableDigestMismatch`,
  `ProcessStartFailed`, `ProtocolNegotiationFailed`, `UnsupportedVersion`,
  `Timeout`, `ProcessExited`.
- Bounded startup timeout and explicit `terminate()` (graceful signal,
  then a hard-kill fallback if the process doesn't exit).
- A fake conformance-test executable (`src/bin/fake_pt_fixture.rs`,
  compiled as a workspace-internal test-only binary) emitting a correct
  PT v1 handshake, so the spawn/parse/terminate path is exercised
  end-to-end without any real, external circumvention binary anywhere in
  this PR — matching the report's explicit "no real PT dependency yet."

## What this PR does not do

No `PluggableTransport` implementation exists for `Obfs4V1`/
`WebTunnelV1`/`SnowflakeV1`/`TorStreamV1` yet — `PtProcessManager` only
proves the safety boundary (spawn, verify, parse, terminate). Dialing
through the resulting local endpoint and performing the inner bridge
handshake is deferred to the Lyrebird PR (PR3), the first PR that
actually needs a real upstream binary. No sandbox enforcement (seccomp,
namespaces, resource limits, restricted tokens) is implemented — the
report's platform-specific packaging models (§16) are each their own
future work. No binary supply-chain manifest/provenance tooling exists
yet (`ExternalAdapterManifest`, §6) — `VerifiedExecutable` today takes a
digest supplied by the caller, it doesn't yet look one up from a governed
manifest.

## Phased plan this repo commits to (see report §24 for full detail)

1. ~~External adapter doctrine~~ (this document) + ~~generic PT process
   manager~~ (this PR).
2. Lyrebird/obfs4 adapter — approved version manifest, real bundled
   binary, `PluggableTransport` implementation dialing through the PT's
   local endpoint, connection tests, first real sandbox pass.
3. WebTunnel adapter — same process boundary, TLS/HTTP deployment guide,
   decoy-behavior and reverse-proxy tests.
4. Tor compatibility bearer — SOCKS5 + stream isolation, no broad
   control-port dependency.
5. Snowflake via Tor — selected through Tor-managed transport rather
   than a Mininet-specific broker/proxy network.
6. Platform study — Android helper-service prototype, iOS native
   feasibility, Arti evaluation, native-adapter criteria.

## Hard rules carried forward from the report

- **Executable path, arguments, and environment are never descriptor-
  supplied.** A `BridgeDescriptor`'s `transport_parameters` describe
  protocol data only; which binary runs and how is always local policy,
  never remote input (§19).
- **No dynamic binary downloads at runtime.** PT executables ship inside
  an official Mininet release, package-manager install, or
  release-attested package — never fetched from an arbitrary URL after
  install (§14).
- **Honest audit terminology.** "Upstream-maintained and field-deployed"
  (true today for Lyrebird/WebTunnel/Snowflake) is a different, weaker
  claim than "independently audited for Mininet's integration boundary"
  (not true of anything in this PR) — never conflate the two in docs,
  comments, or user-facing text (§7).
- **No general plugin platform.** A small, approved adapter list with
  known binaries/protocols/resource requirements — never arbitrary
  user-supplied executables in production/high-assurance mode (§25).
- **A failed connection through one transport is not proof of
  censorship**, and a successful one is not proof of an uncensored
  network — failures are classified, never collapsed into one "blocked"
  signal (§20).

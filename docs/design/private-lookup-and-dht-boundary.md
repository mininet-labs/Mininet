# Private lookup and the public-DHT boundary (MN-208, D-0310)

**Status:** Phase 0 (doctrine) + Phase 1 (local-only primitive) shipped.
No network. Everything past that is deferred.

**Full research:** `docs/research/
MN208_PRIVATE_LOOKUP_DHT_RESEARCH_20260714.md` (founder-supplied,
2026-07-14). This document does not reproduce that report — it records
what was actually built from it and why, and links back for the full
taxonomy, PIR analysis, and phased architecture the report itself lays
out.

## Decision

MN-208 does not begin by building a general-purpose value DHT and adding
privacy around it afterward. `mini-net` has no provider-record or
value-storage DHT to restrict today — confirmed independently while
scoping `mini-relay` (D-0306, tracking issue #144). The correct MN-208
outcome is a design doctrine plus a narrowly scoped private-index
protocol: public network routing may use public peer-discovery data, but
private object discovery must never require broadcasting a sensitive
object identifier, capability, mailbox address, subscription, or content
interest to a public DHT.

## What's implemented

- `LookupPrivacyClass` — the report's frozen five-tier taxonomy
  (`Public` → `CapabilityScoped` → `PrivateProxied` → `PrivateBundled` →
  `PrivatePIR`), ordered so policy code can compare tiers instead of
  relying on caller judgment. Only `CapabilityScoped`'s primitive is
  implemented; the rest are named so later work has a stable vocabulary
  to target.
- `derive_lookup_label`/`CapabilitySecret`/`IndexEpoch`/`LookupPurpose` —
  capability-derived rotating lookup labels via HKDF-SHA256
  (`mini-crypto`'s existing KDF suite — no new cryptography). A
  `LookupLabel` is the opaque handle sent to an index service instead of
  a plaintext object ID, mailbox address, or capability. Nine disjoint
  purpose domains keep unrelated lookup kinds from ever colliding.
- `PrivateIndexRecord`/`RecordSizeClass` — a signed record a writer
  publishes at one lookup label, padded to one of three fixed size
  classes so a record's wire size doesn't itself leak which kind of
  descriptor it carries. The encrypted payload is opaque to this crate;
  content encryption is the caller's job.
- `LocalIndex` — a local, in-memory store enforcing: signature validity,
  one label cannot be hijacked by a different writer, and `sequence`
  must strictly increase (rollback/replay rejection). `lookup()` returns
  `None` for both a missing label and an expired record — indistinguishable
  through this API, matching the report's negative-lookup-indistinguishability
  goal at the local layer.

## What's deferred — "No network yet"

There is no wire protocol, no replicated private-index service, no
relay-based role separation (OHTTP-style query proxying so no single
index service learns both the client's address and the query), no query
batching or decoys, and no caching/prefetch layer. `LocalIndex` proves
the per-replica discipline a networked version would need to enforce; it
does not network anything.

## Hard rule: PIR stays gated

`LookupPrivacyClass::PrivatePIR` — genuine Private Information Retrieval,
where the index service itself cannot learn which record a query
resolved to — is explicitly gated behind external cryptographic review
(CLAUDE.md's no-new-cryptography rule, D-0047). Nothing in `mini-private-
index` may be described as providing PIR, and no future PR may claim
that tier without that review having happened first.

## What this does not provide

No hiding of client network address from an index service (that needs
the deferred role-separation layer), no protection against a single
non-colluding-assumption failure across replicas, no defense against
timing-correlation attacks on lookup patterns, and no encryption of the
record payload itself (this crate stores and forwards opaque bytes only).

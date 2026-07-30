# Native release retrieval — preliminary implementation report

**Date:** 2026-07-29
**Decision:** D-0408 (proposed)
**Issue:** #268
**Status:** code and tests are prepared for founder review; this is not an
external audit, a release approval, or a claim that the production network is
ready.

## Problem

The repository already has two different capabilities:

- `mini sync` can replicate a complete content-addressed store over the
  existing encrypted bearer/channel.
- `mini release fetch` can verify and assemble a release whose objects are
  already local.

That leaves a user who knows an exact release id with no native way to obtain
only that release from a reachable Mininet peer. Whole-store sync is useful for
replicas, but it is broader than a release request and makes the serving
peer's disclosure choice implicit.

## Preliminary decision

Use the existing `mini-sync` channel and object verification boundary for a
one-shot exact retrieval exchange. The serving application selects a bounded
release evidence closure with `mini-forge::release_retrieval_ids`; the client
explicitly echoes the selected ids, ingests the returned objects through the
same verify-before-insert path as ordinary sync, and only then invokes the
existing `verify_governed_release` gate.

The closure follows ordinary forward links and only the reverse relations the
release verifier reads:

- release → project, source commit, artifact manifest, and attestations;
- project/chain entries → every candidate governance successor;
- chain entries/PRs → review evidence and approvals;
- commits/trees/manifests → repository files and media chunks.

Identity KEL trust remains explicit and local. A peer serving bytes does not
silently become a trust anchor. Retrieval never activates a release, changes a
branch, recruits a contributor, assigns a team role, or changes governance.

The current implementation caps one closure at 4096 objects and the transfer
at the existing 512 MiB sync budget. It is intentionally one connection and
one request; it is not a daemon, discovery service, pagination protocol, or
background updater.

## Alternatives considered

### A. Reuse whole-store `mini sync`

**Description:** connect with the existing bucketed reconciliation and run the
existing local `release verify`/`release fetch` commands afterward.

**Pros:** already shipped and tested; no new application framing; excellent
for a replica that wants the same object history.

**Cons:** transfers every missing object, does not express the user's exact
release intent, can disclose unrelated store content to the chosen peer, and
does not provide a native release-facing command. It remains the right option
for full replication, but not the right primitive for this request.

### B. Put release bytes behind an HTTP/GitHub-style service

**Description:** add a conventional server endpoint that serves an artifact
file or archive.

**Pros:** familiar operational model; easy CDN and browser tooling.

**Cons:** creates a new hosting authority and a second trust/format path;
GitHub is temporary rather than canonical; raw artifact bytes do not carry the
governed source/review/attestation evidence that the existing verifier needs.
This is not selected for the canonical path.

### C. Invent a new authenticated release protocol or cryptosystem

**Description:** add release-specific endpoint authentication, a new signing
format, or a new key agreement.

**Pros:** could optimize a future production service around release traffic.

**Cons:** duplicates the existing content-addressed objects, anonymous
encrypted channel, typed provenance, and owner-verification gates; expands the
attack surface; and would violate the project's compose-vetted-primitives
discipline. Not selected.

### D. Exact retrieval over the existing channel (selected)

**Description:** reuse the current bearer/channel and verified object ingest,
add a bounded selection/echo/objects exchange, and keep release-specific
closure policy in `mini-forge`.

**Pros:** preserves the existing cryptographic and authority boundaries;
supports a real CLI workflow during a GitHub outage; gives the client an
explicit allow-list and a deterministic, bounded selection; leaves full sync
available for replicas.

**Cons:** the serving peer can still omit evidence or close the connection;
the client must have explicit KEL trust for the release participants; the
current closure is one-shot and capped rather than paginated; and transport
endpoint authentication remains outside this anonymous channel by design.
Those are stated limits, not silently treated as solved.

## What is not built or audited

- No background release daemon, peer discovery, relay scheduling, or provider
  availability layer.
- No GitHub import/export mirror automation; GitHub remains a temporary
  adapter, not canonical truth.
- No automatic identity-root or personhood conclusion; a verified identity
  root is not proof of unique personhood.
- No OS/package-manager integration or process supervision in
  `mini-installer`; activation remains an explicit owner action and is
  currently Unix-only.
- No external cryptography, protocol, hardware, legal, or production-scale
  audit has been completed.

## Review questions for the founder and external reviewers

1. Is the 4096-object/512 MiB one-shot limit appropriate for Beta, or should a
   later decision introduce an explicitly paginated closure protocol?
2. Should review findings and AI-assistance metadata remain in the default
   closure, or should a future disclosure policy let a user request only the
   authority-bearing subset while retaining an audit-inspection mode?
3. What peer admission/discovery and availability policy belongs above this
   anonymous transfer without turning a hosting operator into canonical
   authority?
4. What independent evidence is required before native retrieval can be used
   for a public update channel rather than Beta testing?

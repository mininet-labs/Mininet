# The self-hosted forge spine — adapted from the founder-adopted external audit (D-0066)

A founder-commissioned external technical assessment (2026-07-10) found that
this repository's implementation breadth (identity, presence, storage
rewards, confidential value, treasury custody, settlement, finality
verification, social objects, forge) has run ahead of *vertical
integration*: there is no complete path from a developer's change through
review, governed merge, reproducible build, release finality, safe
installation, health check, and rollback. The founder adopted the report's
recommended six-batch re-sequencing (D-0066). This document is that plan,
adapted to what is actually already real in this tree versus genuinely
missing — see each batch's "already have" / "genuinely missing" split.

**Batch 1's exit condition, the bar every piece below is measured against:**
*two developers can exchange a signed proposed commit, review the exact
commit, and reach a governed canonical branch head without GitHub being the
authority.*

## Correction to the audit

The report describes a "proposal/review/merge object model" as new work for
its first recommended PR. `crates/mini-forge/src/governance.rs` already
implements this, predating this session:

- `propose()` — a PR object binding an exact `head` commit and the `base`
  chain position it was built against.
- `approve()` — a verdict **bound to the exact head commit reviewed**
  (invalidated by any later commit swap) — exactly the property the report
  asks for.
- `merge()` / `amend()` — chain entries recording a merge or a
  self-amending policy change.
- `resolve_project()` — deterministically walks the chain and counts quorum
  in distinct verified identity roots (author excluded), with fork
  detection.

Real, tested, already-shipped code. What the report correctly identifies as
missing *around* it, addressed in Batch 1 below: review objects carry only
an approve/reject bit, not free-text findings or CI/test attestations bound
to the reviewed commit; no AI-assistance/human-owner metadata field exists;
and — the actual gap — there is no way for a human to drive any of this
without hand-writing Rust against the library API.

## Batch 1 — developer spine

**Already have:** the propose/approve/merge/amend/resolve_project object
model and quorum logic (`mini-forge::governance`); signed content-addressed
files/trees/commits/branches (`mini-forge` core); a real filesystem-backed
object store (`mini_store::FsBackend`); KEL export/import
(`Kel::to_bytes`/`from_bytes`) for building a local trust directory; secure
key-seed export (`SigningKey::to_seed_bytes`) for on-device persistence.

**Genuinely missing, this batch:**
- `mini-cli` — a real command-line tool wrapping the above so a human can
  actually use it (repo init/commit/checkout/status/branch, PR
  propose/approve/merge, identity init/show, KEL export/trust). Ships this
  batch.
- Review findings + AI-assistance/human-owner metadata on review objects.
  Ships this batch (extends the existing `approve()` payload format, does
  not replace the model).
- `mini-devd` (local daemon, socket IPC, event subscriptions) — deferred
  past this batch. The CLI can operate directly against a local `FsBackend`
  without a daemon; a daemon becomes necessary for background sync and live
  event subscriptions, neither of which Batch 1's exit condition requires.
  Tracked as a fast-follow once the CLI's command surface has proven out in
  practice.
- Git SHA-256 import/export bridge — deferred past this batch (already
  named as pending in `mini-forge`'s own docs before this audit; real work,
  not re-scoped here).
- Machine-readable `STATUS.md`/roadmap generation — deferred; the manual
  three-document reconciliation problem the audit names is real, but lower
  urgency than giving developers a working tool.
- Live network peer exchange for the CLI (`mini sync`) — deferred as a
  near-zero-effort fast-follow: `mini_bearer::TcpBearer` +
  `mini_sync::sync_bidirectional` already proved this composition live
  (D-0062, `mini-bootstrap`'s demo). Batch 1's own exit-condition
  demonstration instead uses a shared `FsBackend` directory (content-
  addressed signed objects are safe to share via any medium — a synced
  folder, a USB stick, later real `mini-sync` — the transport is
  interchangeable and out of scope for what Batch 1 is actually proving,
  which is the governance loop itself).

## Batch 2 — in-house scripting and builds

`mini-pipeline`: a declarative build/automation engine executing developer
hooks and build steps inside WASI Preview 2 / the WebAssembly Component
Model via Wasmtime, with per-component declared capabilities
(`workspace:read`, `network:none`, `secrets:none`, etc.) rather than ambient
filesystem/network/secret access. Produces in-toto/SLSA-style provenance
(exact source object, dependency lock digest, builder identity/environment
digest, commands run, output digests, reproducibility group). Not started.

## Batch 3 — release verification

Adapt TUF's role separation (root / targets / snapshot / timestamp,
delegated roles) rather than inventing a new trust model — Mininet's
existing release-registry design (timelock + independent attestation counts
in `mini-forge::release`/`verify_governed_release`) already covers part of
this; missing are metadata expiry, rollback protection, a release
transparency log, and requiring builder quorum from *administratively
independent* builders (not three containers on one host). Not started.

## Batch 4 — real installation

`mini-installer`, separated from `mini-update` (which stays policy/intent
only, per the existing no-forced-update/no-kill-path freeze). State
machine: `Discovered → Verified → Downloading → Staged → PreflightPassed →
AwaitingOwnerApproval → Activating → HealthChecking → Active` or
`RolledBack`. This is the audit's most safety-critical, most honestly-named
gap — `mini-update::AdoptionState::adopt` today records a decision, nothing
executes, fetches, or installs. Not started; the largest remaining piece of
this whole plan.

## Batch 5 — Mininet as the primary forge

P2P proposal/review synchronization (reusing `mini-sync`/`mini-net`, no new
wire protocol needed per the same composition insight D-0062 already
proved), local object indexing at scale, distributed build workers, native
release retrieval, GitHub import/export mirror automation so the roles
eventually invert (GitHub becomes the read-only mirror). Not started.

## Batch 6 — resume horizontal breadth

Only after Batch 4's exit condition (a deliberately broken release
detected, auto-recovered, with a verifiable event history in a test
environment) does substantial effort return to networked consensus, real
BLE/UWB hardware, personhood research, economic mechanisms, anonymous
value depth, and proof-of-replication depth (#36-#45, #22, #18/#19/#20/#21,
#46-#51, further `mini-value`/`mini-porep` work).

## Cryptography and governance notes from the audit, applied opportunistically

Several of the audit's cryptography recommendations (Noise-framework
handshakes instead of an ever-expanding custom one, hybrid post-quantum
signatures, per-key-role separation, multi-signature release attestation
over single-threshold FROST) are real improvements but are not blocking for
Batch 1 and are not scheduled as their own batch — they get applied where
the relevant subsystem is next touched (e.g. the channel handshake in
`mini-bearer` when that crate is next substantially revisited), rather than
as a standalone rewrite pass, per Directive 14 (don't touch working code
without a concrete reason).

The audit's policy-class table (stricter approval floors for
identity/consensus/forge governance, installer/update, and cryptographic
protocol code than for ordinary changes) is adopted as guidance for
`CONTRIBUTING.md`'s review checklist, tracked as part of Batch 1's metadata
work (AI-assistance/human-owner fields make the distinction visible on the
object itself).

# Launch integration and pull-request review — 2026-10-14

Status: AI engineering evidence, not human approval, external audit, gate closure,
or production readiness. Reviewed main checkpoint
`e88722744110aa35d323b63d98006447f6c0aecb` through connected GitHub.
The managed executor failed provisioning; no local builds or tests were run.

## Scope and method

Three agents reviewed cryptography/identity boundaries, open-PR readiness, and
founder/roadmap integration. Findings use current files, PR patches, metadata,
review threads and exact-head Actions evidence. This was a prioritized source
review, not an exhaustive audit of every merged line or all workspace crates.
Old successful workflows are historical evidence; fresh October checks reveal
common baseline failures and must be rerun on final integration candidates.

## Recent merges and actual integration

The recent set inspected for priorities includes #352, #350, #351, #349, #348,
#346, #340, #343, #341, #345, #333, #332, #327 and #331.
#352 supplies offline-credit/account primitives, not activated canonical private
credit or wallet integration. #349 scopes process-separated workspace integration;
#350 supplies only the first application-core slice. #346 connects desktop
sessions, but does not close real two-machine/hardware acceptance.
#333/#332 contain substantive crypto/security remediation; their retained audit
and hardware limits are not superseded by merge.

**#342 merged into #334's branch, not main.** The Forge-native Beta CLI is therefore
still part of #334's integration proposal. #353 is an explicitly untested
GrapheneOS feasibility spike, not a shipped operating system.

## Original open-PR inventory

| PR | Reviewed head | State and remaining engineering work |
| --- | --- | --- |
| #363 | `3edb632c47ab` | Conflicting/draft. Ordinary connection-card root can be published by private-follow UI; pseudonym root signing fails verified delegation ingest; plaintext linkage does not resist leaked-byte deanonymization. Needs delegated pseudonym device/KEL carriers and verified two-store sync coverage. |
| #362 | `38a0deef565f` | Mergeable/draft. Five September workflows succeeded; strict CRDT link validation should receive fresh checks after baseline repair and coordinated object-schema integration. |
| #361 | `05915ef7ab35` | Mergeable/draft. September governance failed on stale issue-201 work claim. Coordinate object schema with #357/#362 and rerun current checks. |
| #360 | `51003746c822` | Conflicting/draft. Named tiers are optional capability policy, not enforced replacement of arbitrary delegation or root recovery. Preserve honest API scope in merge resolution. |
| #359 | `f0d1d6bb0a65` | Conflicting/draft. Five historical workflows succeeded. Test-only partition coverage needs conflict resolution and fresh integration checks. |
| #358 | `2e898523ca4e` | Conflicting/draft. Address-book bound is useful; PostRateLimiter existence does not establish live client admission integration. |
| #357 | `14149c218269` | Mergeable/draft. This review pushed recomputed AI-object integrity plus mutable payload/provenance regression. Capability gates provenance verification, not low-level signing; mutable fields weaken constructor-guarantee wording. Fresh CI pending. |
| #356 | `cd85c5de8262` | Conflicting/draft. Default/new allocate different IDs; persistence loses IDs/counter and can reuse removed IDs. Newline-bearing rules need robust serialization. Local render integration remains separate. |
| #355 | `d6079e41a22d` | Conflicting. This review pushed key-authenticated prune with persistence regression. Missing rollback snapshot and replacing RootCore during pairing remain blockers; device enumeration advisory remains. No Actions evidence on conflicting head. |
| #353 | `228c1d73d10e` | Mergeable. Design spike only. Historical governance failure and current shared baseline require repair; no hardware/OS claim. |
| #334 | `60554aa03c14` | Mergeable. Five historical workflows succeeded; includes #342 branch merge. Fresh security/governance checks and gate-by-gate integration review remain necessary. |

No AI review above supplies approval weight. Keep technical review distinct from
human integration responsibility. Canonical document 52 supersedes bootstrap
calendar/approval procedure while preserving substantive protections, external
review evidence and the irreversible Go-Live boundary.

## Changes produced by this review

- #364 recomputes Object IDs from current canonical bytes and checks integrity
  before Store writes; tests protect original blobs and author/type/time indexes.
- #365 bounds validator evidence by entries, retained wire bytes and individual
  wire bytes, without implicit eviction. Genuine-proof tests cover concurrent
  admission, retries, replacement accounting and independent verification.
- #355 prune authorization now authenticates matching ciphertext before deletion.
- #357 AI-object integrity now rejects mutated fields with stale cached IDs.
- The issue-201 work lease is proposed closed because #333 already merged;
  its history is preserved. This does not close issue #201 or hardware acceptance.

## Current shared CI blockers

1. Wasmtime and wasmtime-wasi 46.0.3: RUSTSEC-2026-0314, -0316, -0321,
   -0322, -0323, -0324 and -0327. Upgrade both exact pins together to at least
   48.0.4 in the 48.x line (or 49.0.2+), regenerate Cargo.lock, and run runner
   adversarial tests plus dependency-deny. Do not suppress these advisories.
2. Governance unit tests still apply D-0083's October-12 sunset although
   canonical document 52 supersedes that calendar process and directs a
   machine-readable Pre-Go-Live replacement. Update enforcement coherently;
   never replace the failure with an arbitrary later date or disable checks.
3. Work claim #201 remains active in main despite merged #333 and an expired
   September-25 lease. Close the implementation lease, preserving open substantive
   hardware and external-review work.

## Ordered path toward integration

Repair shared baseline and dependency vulnerabilities first. Resolve and check
the narrow object/CRDT/partition PRs, incorporating #364. Remediate privacy,
capability, rollback and persistence defects before integrating affected clients.
For #365, retain R8 active: budgets are local operational limits, not canonical
validity or complete allocator/transient-memory bounds.

Then demonstrate real cross-crate paths: authenticated identity/delegation ->
signed object -> verified ingest -> persistent storage -> bounded sync -> client
display; private payment -> evidence verification -> canonical membership/
nullifier application -> complete-input wallet finality; mobile pairing ->
durable identity mutation -> crash/restart -> retained messaging state.

New safety/rate-limit crates need real opt-in callers and acceptance evidence.
External cryptography/DKG review, real-device testing, legal/economic review,
personhood research and actual activation decisions retain their own closure
requirements. More code and green CI alone cannot establish those outcomes.

## Follow-up engineering — same review, October 14

All six previously conflicting branches (#355, #356, #358, #359, #360, #363)
now incorporate main `e88722744110aa35d323b63d98006447f6c0aecb` through
leased, non-forced merge commits. Both independent append-only decision records
are preserved. #355/#356 also preserve disjoint Cargo.lock package changes and
both status-document additions. This repairs source conflicts, not security
approval or technical gate failures. Generated navigation still needs runtime
regeneration.

#355 additionally rejects missing snapshots as successful rollback and prevents
revocation/RootCore replacement during active pairing. Camera failure callbacks
cannot clear an in-flight pairing guard. Kotlin runtime acceptance remains open.
#363's ordinary-card private-follow action is disabled, preventing the identified
UI privacy leak; delegated pseudonym ingest and linkage confidentiality remain
unresolved.

#356 at `dbacdac80aeaf71a8dd7a9c00450887ede6b3f01` fixes Default/new
consistency, stable IDs/high-water persistence, escaped fields and checked
allocation. Legacy files remain readable. Rule allocation/import now return
Results, and import stages mutations to prevent partial application at
exhaustion. Five new regressions are proposed; exact-head CI is queued. This is
still a local primitive without a live client caller.

#364 at `c657c308957c3a477b6324d5e04887949ba5abb0` passed Linux formatting,
Clippy and full-workspace tests, Windows client/packaging, Android and
reproducibility workflows. Shared dependency-deny and governance remain red.
#365 at `9edf013ec551a53c32363fb9abc636b53ccbf189` passed Linux formatting,
Clippy and full-workspace tests after the concurrent-test Clippy correction.
Its shared security/governance blockers remain; do not infer overall CI success.

Draft #367 at `3fba39fd964be652a0ae3e8cd03b8be2fde74c5f` implements the
document-52 Pre-Go-Live profile with explicit Go-Live state, exact canonical
document pin and transition rollback protection. Historical v1 tests and
dependency exception expiry remain. Candidate validation is in progress; the
old canonical base evaluator cannot activate its own successor from proposal
code. See `governance/PRE_GO_LIVE_PROFILE.md` in that PR for the precise
canonical integration boundary. No expiry extension or technical-check
disablement is proposed.

The workspace restart remains pending/offline, so coordinated Wasmtime
48.0.4+ lockfile resolution, runner API adaptation/adversarial execution,
navigation regeneration and local Kotlin acceptance are not completed.
Continue with those tasks once execution becomes available, retaining the
explicit audit/hardware/activation gates.

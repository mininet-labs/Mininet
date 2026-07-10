# Implementation status

This is the living account of *what's actually built*, kept deliberately
separate from `docs/DECISION_LOG.md` (which records policy decisions and
their rationale, not ongoing status) per the founder's own review of these
two documents. If a decision log entry's "Implementation status" field and
this file ever disagree, **this file wins** — it's updated far more often
than any individual decision entry is revisited.

Organized by the same nine domains as `docs/INVARIANTS.md`, so a reviewer
can check "is this invariant enforced" and "how far along is the thing
that would enforce it" in one pass across both files.

Status legend: **shipped** (real code, tested) · **partial** (real code
for part of the claim, gap documented) · **prototype** (real code, but
explicitly founder-reviewed only, pending external audit) · **design-only**
(written design exists, no code yet) · **not started**.

## 1. Voice / value

- **shipped** — `ValidatorSet`/governance quorum counting has no weight
  field anywhere (`mini-chain`, `mini-forge::governance`).
- **shipped** — storage/seeding reward accrual (`mini-storage`,
  `mini-reward`), public walls (`mini-social::PublicWall`), base devices
  (`did-mini::BaseDeviceRole`) all confirmed to create no governance
  weight.
- **partial** — BFT finality *verification* is shipped
  (`mini-chain::verify_finality`); the networked consensus protocol that
  produces the votes it verifies is **not started** (roadmap Phase 5,
  [#36](../../issues/36)-[#45](../../issues/45)).

## 2. Personhood

- **prototype** — `mini-uniqueness::status` (D-0038): open-ended
  multi-signal `HumanRecord`/`TrustWeights`/`PromotionPolicy` accumulator.
  Real, tested code. Hardened per the #18 Sybil review (D-0054): reaching
  `FullHuman` now requires a *live* seed-anchored vouching-graph signal,
  closing a farm-saturation bypass — see
  `docs/audits/issue-18-sybil-social-graph-review.md`.
- **reviewed** — presence attack review ([#17](../../issues/17),
  `docs/audits/issue-17-presence-attack-review.md`): replay/binding/clone
  defended; active relay is NOT defended by software RTT alone (needs UWB
  distance-bounding) — presence is safe only as a *weighted* signal.
- **design-only / research-blocked** — signal (b), on-device behavioral/
  location entropy proved in zero-knowledge: the whitepaper itself calls
  this unsolved research. Not a code gap; a research gap
  ([#21](../../issues/21)).
- **HARD LIMITATION, not partial** — every "verified identity" counted
  anywhere in this tree today is a verified `did:mini` root, not a
  verified human. See `docs/INVARIANTS.md`'s hard-limitation section.
  Sybil-resistance at real-world scale is unproven
  ([#18](../../issues/18)).
- **partial** — co-presence attestation (`mini-presence`) is shipped;
  the software RTT bound has no hardware ranging backing it yet in
  production use (UWB trait scaffolded, not wired to real hardware).

## 3. Identity & key custody

- **shipped** — `did-mini`: KEL, pre-rotation, device delegation,
  detached signing, decoder hardening, and now **lost-device/death
  recovery** (`Controller::recover_from_kel`, D-0053) from escrowed
  next-key seeds. Security-audited ([#12](../../issues/12),
  [#13](../../issues/13)): 3 findings fixed
  (threshold-policy rewrite, delegated-acting-as-root, seed scrubbing).
  Logic-complete, hardened, tested.
- **partial / launch-blocking** — KEL freshness & duplicity detection: a
  stale root KEL still accepts a revoked device (audit #12 F4). Owned by
  M3 witnesses (SPEC-01 §7). Interim rule: pin highest sn seen per SCID.
- **not started** — post-quantum migration path
  ([#15](../../issues/15)), device
  hierarchy beyond current single-tier delegation
  ([#14](../../issues/14)), on-chain
  pre-rotation anchoring (needs the chain).

## 4. Money & finality

- **prototype** — `mini-value`: stealth addresses, linkable ring
  signatures, Bulletproofs confidential amounts (D-0036/D-0040). Real,
  tested, founder-reviewed, **pending external audit** — see `docs/
  audits/issue-8-constitutional-audit.md`'s A1 row.
- **prototype** — `mini-treasury`: FROST threshold signing (D-0041), live
  multi-process demo, real distributed key generation and committee
  resharing (D-0060, closes D-0048's DKG gap — Pedersen DKG with a
  complaint/rebuttal exclusion mechanism, plus `ReshareFromPreviousEpoch`).
  `trusted_dealer_keygen` remains, gated behind `AcknowledgedPrototypeOnly`,
  for tests/demos only. Both DKG and resharing require
  `AcknowledgedUnauditedDkg`; neither is externally audited yet — see
  `docs/gates/dkg-audit-scope.md` before treating this as production-viable
  at any value level.
- **prototype** — `mini-settlement` (D-0055, closes roadmap #41): the M1/M2/M3
  offline settlement protocol is real, tested code — signed
  `PaymentClaim`s, the `SettlementState` wallet vocabulary
  (pending/accepted/finalized as distinct types), local conflict detection
  (`ClaimWatcher`), and canonical reconciliation (`reconcile`) proving
  exactly one of two conflicting claims ever finalizes. `mini-reward`'s
  accrual bookkeeping remains ordinary, non-spendable value, unaffected by
  and separate from this crate.
- **prototype** — `mini-execution` (D-0061, closes roadmap #40): a real,
  chain-backed `CanonicalLedgerView` — `LedgerChain` only ever advances
  settlement state behind a verified `mini_chain::QuorumCertificate`, never
  speculatively. Closes `mini-settlement`'s own named gap: two independent
  `LedgerChain`s fed the same finalized blocks are proven (not just
  argued) to converge to bit-identical state (Directive 4), and a
  double-spend across two competing block proposals is proven to resolve
  to exactly one finalized winner end to end. Deliberately still not a
  networked chain — no proposer rotation, no vote gossip — that remains
  roadmap #36-#45's job; this crate answers "given a finalized block, what
  changed" precisely, not "how do nodes agree on the next block."

## 5. Updates & forks

- **shipped** — `mini-update::AdoptionState` (local adoption state
  machine, no forced update, no kill path) over `mini-forge`'s release
  registry: timelocked, independently-attested `RELEASE` objects
  (`mini_forge::release`/`verify_governed_release`), plus, as of D-0070
  (self-hosted forge spine Batch 3), four additional layered gates —
  rollback protection (`Version`, `check_no_rollback`), a release
  transparency log (`list_releases`, `detect_equivocation` for
  same-version/different-digest equivocation), a device-local freshness/
  staleness bound (`FreshnessPolicy`, refuses adoption on too-stale a
  synced view before any governance check runs), and an optional
  independent build-provenance quorum (`ProvenancePolicy` +
  `AdoptionState::evaluate_with_provenance`, wiring
  `mini-provenance::independent_agreement` as a second, independently-
  computed distinct-identity-root count alongside the existing
  attestation quorum). 25 tests across `mini-forge`/`mini-update` combined
  cover every new gate's rejection and passing paths.
- **shipped** — `mini-installer` (D-0071, self-hosted forge spine Batch 4):
  real local staging (`mini_media::assemble` against the actual store,
  independent digest re-verification), preflight (re-verifies staged bytes
  on disk immediately before activation), owner-approved atomic activation
  (`OwnerApproval` is a typed request naming the exact release id, never a
  generic "approve"; activation is a real `symlink`/`rename` swap), a
  caller-supplied health check, and automatic rollback on a failed check
  (clearing `current` entirely rather than leaving it on known-unhealthy
  software if there was nothing to fall back to). Unix-only; no process
  supervision; no real package-manager/OS integration -- honest limits
  stated in the crate's own docs. 10 adversarial/integration tests against
  real files on real disk.
- **partial** — `mini-bootstrap` (genesis/capsule protocol logic) is
  shipped, and now proven live over real TCP (D-0062, closes #23, see §8);
  real BLE/Wi-Fi radio adapters remain not started (need phone hardware).
- **not started** — the emergency-update-path question
  ([#53](../../issues/53)) and fork-legitimacy criteria (F1,
  `docs/INVARIANTS.md` §5) beyond the frozen statement of the requirement
  itself; wiring `mini-installer` into an actual running system (service
  restart, binary-on-`PATH` swap, etc. -- that integration is deliberately
  the caller's job, layered on top of this crate's atomic pointer flip).

## 6. Privacy

- **shipped** — `mini-bearer::Channel` (anonymous, forward-secret,
  handshake carries no identity); `mini-store`'s seed-on-view policy
  gating.
- **not started** — the storage fabric's P6 guarantees (no forced
  replication, no compelled decryption) have no owning subsystem yet.

## 7. Storage

- **prototype** — `mini-spacetime::storage_proof` (D-0039): Merkle/PDP
  challenge-response. Real, tested. **Proves possession, not replication
  uniqueness — see `docs/INVARIANTS.md`'s hard-limitation section.**
- **prototype** — `mini-porep` (D-0064, closes [#31](../../issues/31)):
  real Filecoin-style Stacked Depth-Robust Graph (SDR) proof-of-
  replication, coded in-house from the published construction (D-0063).
  Sequential stacked layered labeling + a registration-time probabilistic
  audit (the honest substitute for a zk-SNARK sealing circuit) close the
  replication-uniqueness gap the line above names: producing `k` sealed
  replicas now costs approximately `k` times the real sequential sealing
  work, so a warehouse cannot cheaply fake holding many independent
  copies. Ongoing possession is proven by composing (not duplicating)
  `mini-spacetime`'s own PDP challenge-response against the sealed
  replica's root; implements `ProofOfSpaceTimeSource` so
  `mini_spacetime::proposer_weight` needs no changes. Real, tested (30
  unit tests incl. adversarial cases), founder-reviewed,
  **unaudited** — same D-0047 gate as every other prototype here. DRG is a
  simplified construction, not parameter-identical with Filecoin's
  production `BucketGraph`; the audit is probabilistic, not a succinct
  proof — see the crate's own README for the honest limits in full.
- **prototype** — `mini-erasure` (D-0065, closes [#30](../../issues/30)
  and [#32](../../issues/32)): systematic Reed-Solomon erasure coding over
  `GF(2^8)` (normalized Vandermonde generator matrix, Gauss-Jordan decode
  from any `k` of `n` shards) plus a self-healing repair layer —
  `plan_repair`/`repair` detect missing *or corrupted* (BLAKE3-verified)
  shards and regenerate exactly the missing ones. An external review
  found the originally-shipped generator matrix (raw parity rows appended
  below an identity block) did not actually have the MDS property for
  all accepted parameters — a concrete counterexample failed to
  reconstruct from a within-tolerance shard loss; fixed in D-0072 by
  normalizing a full Vandermonde matrix against its own top block
  instead. Real, tested (29 tests incl. an end-to-end two-outage healing
  cycle, the fixed counterexample as a permanent regression, and a
  randomized larger-configuration sample), founder-reviewed. Proves the
  coding/repair logic only — actually distributing regenerated shards to
  network holders is `mini-net`/`mini-store`'s unstarted job.
- **not started** — cold/owner-only storage tiers, huge-file handling at
  scale (roadmap Phase 4).

## 8. Networking

- **shipped** — `mini_bearer::TcpBearer` (D-0042): real TCP transport,
  tested, proven live via `mini-net`'s three-process gossip demo.
- **shipped** — `mini-bootstrap`/`mini-sync` proven live over real TCP
  (D-0062, closes [#23](../../issues/23)): a genuinely fresh device (empty
  store, empty `KelCache`) bootstraps a signed genesis capsule from a seed
  peer over a real socket end to end, and `mini_sync::sync_bidirectional`'s
  own "over any bearer" claim is now tested against `TcpBearer`, not just
  `InProcessBearer`.
- **partial** — `mini-net`'s gossip logic is proven live over real
  sockets; peer *discovery* (`RoutingTable`) is unexercised over a real
  transport; not a mesh.
- **not started** — BLE/local-Wi-Fi radio adapters (needs real phone
  hardware, [#22](../../issues/22)); NAT traversal; local mesh routing.

## 9. AI & audit gates

- **shipped (as policy)** — D-0037's AI-authorship-with-human-review
  policy; `mini-forge::governance::PROTOCOL_MIN_APPROVALS`'s 2-approval
  floor.
- **tightened this pass** — D-0047 makes external audit a hard
  *production* gate (not "desirable") for value privacy, treasury
  custody, consensus, and personhood proofs specifically. No code
  path in this tree currently claims production-readiness for any of
  these, so this is a frozen constraint on the future, not a retrofit.
- **shipped** — a dedicated "this PR was AI-assisted" flag on PRs
  ([#78](../../issues/78)): `mini_forge::declare_ai_assistance`/
  `ai_assistance` (D-0067) — a signed, PR-author-only, purely
  informational declaration naming an accountable human owner, never
  counted toward merge quorum. `mini_forge::record_findings`/
  `list_findings` (D-0067) similarly makes free-text review findings a
  real, queryable object instead of PR-description prose.
- **not started** — an actual external audit engagement (not tracked in
  code at all — business/process work).

## 10. Self-hosted forge spine (D-0066, tracking issue #102)

Not one of the nine `docs/INVARIANTS.md` domains — a founder-adopted
external-audit-driven development-sequencing initiative
(`docs/design/self-hosted-forge-spine.md`). Batches 1-4 (developer →
review → governed merge → reproducible build → release finality → safe
install → rollback) are now shipped end to end; per CLAUDE.md, what comes
next — Batch 5 (Mininet as primary forge) vs. resuming Batch 6's
horizontal roadmap breadth — is a founder priority call, not decided here.

- **shipped** — Batch 1's first exit-condition demonstration: `mini-cli`
  (D-0067), a real command-line tool (`identity`/`kel`/`repo`/`pr`
  subcommands) over already-real `mini-forge::governance` primitives.
  `tests/two_developers.rs` proves three independent `mini` homes,
  sharing only a filesystem `--store` path (no networking, no daemon),
  reach a governed 2-of-3 merge and correctly refuse to merge under
  insufficient quorum first.
- **shipped** — Batch 2a: `mini-provenance` (D-0068). SLSA/in-toto-style
  build provenance as real, signed objects; `independent_agreement()`
  generalizes `mini_forge::release`'s independent-attestation pattern to
  the build stage, before a release is even proposed, with the subject's
  own author correctly excluded. 8 tests. Directly answers the audit's
  named critique that this repo's same-runner clean-rebuild CI check must
  never be called independent reproducibility.
- **shipped** — Batch 2b: `mini-pipeline` + `mini-pipeline-protocol` +
  `mini-build-runner-wasmtime` (D-0069). Wasmtime adopted as the reference
  executor for untrusted `wasm-component` pipeline steps, isolated to a
  dedicated runner process — `mini-cli`/`mini-forge`/`mini-chain`/identity/
  ordinary nodes never link Wasmtime. Deny-by-default capability model
  (filesystem/network structurally absent unless declared; clock/random
  are declared *policy*, not structurally removable in the `wasi:cli/
  command` world — stated honestly in the crate's own docs); fuel as the
  primary CPU limit, epoch interruption as an emergency wall-clock stop,
  a `ResourceLimiter` for memory; content-addressed component/workspace
  inputs re-verified by hash before execution. `tests/adversarial.rs`
  drives the real compiled binary as a subprocess against real,
  freshly-compiled WASI Preview 2 components and demonstrates 10 of
  D-0069's 12 exit criteria directly (signed-component execution,
  structural fs/network denial, path-traversal/symlink-escape refusal,
  fuel/memory/stdout limits actually enforced, provenance-field
  completeness, cross-invocation reproducibility); criterion 9 (runner
  crash doesn't corrupt the forge/provenance store) is demonstrated only
  partially, at this crate's own boundary, not against real `mini-forge`/
  `mini-provenance` storage; criterion 11 (native tools never
  trusted-provenance-eligible) is a `mini-pipeline` structural guarantee.
  `StepKind::NativeTool` (`cargo build`, `npm install`, ...) remains
  explicitly unsandboxed and never trusted-provenance-eligible until a
  separate, digest-pinned, OS-isolated execution mechanism is designed
  and decided the same explicit way D-0069 was.
- **shipped** — Batch 3: TUF-adapted release verification (D-0070). Four
  gates layered in front of `mini_forge::verify_governed_release`, unmodified
  underneath: rollback protection (`mini_forge::release::{Version,
  check_no_rollback}`, strict dotted-numeric parsing, zero-padded
  component comparison); a release transparency log
  (`mini_forge::release::{list_releases, detect_equivocation}`, built on
  the object store's own append-only nature — no separate signed snapshot
  format); a device-local freshness/staleness bound
  (`mini_update::FreshnessPolicy`, refuses adoption on a too-stale synced
  view before any governance check runs, capped by
  `FRESHNESS_MAX_ALLOWED_STALENESS_MS`); and an optional independent
  build-provenance quorum (`mini_update::ProvenancePolicy` +
  `AdoptionState::evaluate_with_provenance`, wiring
  `mini-provenance::independent_agreement` as a second,
  independently-computed distinct-identity-root count alongside the
  existing attestation quorum). See §5 for the full detail; 25 tests.
- **shipped** — Batch 4: real installation, `mini-installer` (D-0071), the
  audit's most safety-critical named gap. Type-state pipeline over the
  exact named state machine (`Discovered → ... → Active`/`RolledBack`):
  real staging from the store (`mini_media::assemble`, independent digest
  re-verification), preflight re-checking staged bytes on disk, owner-
  approved atomic activation (`OwnerApproval` is a typed request naming
  the exact release id — the typed-domain rule, not a generic "approve"),
  a caller-supplied health check with automatic rollback on failure (never
  leaving a known-unhealthy release marked `current`). Batch 6's exit
  condition (a deliberately broken release detected and auto-recovered
  with a verifiable event history) is demonstrated in this crate's own
  test suite, honestly caveated as a real local disk in a test
  environment, not yet a live distributed system. Honest limits: Unix-only
  (`symlink`/`rename` activation), no process supervision, no real
  package-manager/OS integration — see §5 for the full detail; 10 tests.
- **shipped** — Batch 5, first piece: `mini sync listen`/`mini sync
  connect` (`mini-cli::sync`), live network peer exchange over a real TCP
  `mini_bearer` + `mini_sync` connection — Batch 1's remaining deferred
  item. `tests/network_sync.rs` proves two `mini` homes with completely
  independent, unshared stores reach the same governed merge purely over
  the network. One connection per invocation (no daemon yet).
- **not started** — `mini-devd` (local daemon), Git SHA-256 bridge,
  machine-readable `STATUS.md`/roadmap generation (Batch 1's remaining
  deferred items); wiring `mini-installer` into an actual running system
  (Batch 4's own named next step, the caller's job by design); the rest of
  Batch 5 (local object indexing at scale, distributed build workers,
  GitHub import/export mirror automation).

## What has no client, at all

No mobile, desktop, or web application exists anywhere in this
repository. `docs/UI_BETA_PLAN.md` is a plan, not code. This is a
backend/protocol Rust workspace only.

## Where to look for more detail

- `README.md`'s repository-map table — per-crate one-line status, kept
  in sync with this file but intentionally shorter.
- `docs/BETA_STATUS.md` — the narrower, nearer-term two-phone keystone
  beta target specifically, not the whole system.
- `docs/audits/` — point-in-time audit findings that inform several of
  the statuses above.
- `docs/DECISION_LOG.md` — why each of these choices was made; this file
  only says what's true today, not why.

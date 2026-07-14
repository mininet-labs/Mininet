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
  (`mini-chain::verify_finality`), and `mini-consensus` now runs it as a
  **networked, multi-round Tendermint protocol** across processes (D-0200
  through D-0203): a full implementation of Algorithm 1 from
  Buchman/Kwon/Milosevic (arXiv:1807.04938) — proposer rotation,
  prevote/precommit steps, `nil` votes, `lockedValue`/`validValue` locking,
  POLC re-proposal, and round-timeout **view-change** — with the state
  machine kept clock- and socket-free and driven over a real, non-blocking
  `mini-bearer` TCP mesh. Two real-socket tests pass repeatedly: four
  independent ledgers converge to bit-identical state, and a four-validator
  cluster with **one proposer permanently offline** still finalizes every
  height by viewing-change to a fresh proposer (`tests/networked_consensus.rs`).
  Safety (never two conflicting decisions at one height) is complete,
  **proposals are signed** (D-0202: a node accepts a proposal only from a
  `VOTE`-capable device of the exact `proposer_for(height, round)`, closing
  the front-running gap), the mesh is **non-blocking and buffered**
  (D-0203, so a wedged peer cannot back-pressure honest nodes),
  **equivocation is detected** (D-0204: a validator that double-signs one
  `(height, round, phase)` is counted at most once and its conflicting vote
  is surfaced as verifiable `EquivocationEvidence`), and messages are
  **dedup-flooded (re-gossiped)** so consensus is live over any *connected*
  graph, not just a full mesh (D-0205 — proven by a real-socket four-node
  *line* topology test where endpoints reach quorum only via relay), and
  every link is now **confidential and tamper-evident**: each one runs a
  real `mini_bearer::Channel` handshake (ephemeral X25519 + HKDF-SHA256 +
  ChaCha20-Poly1305, forward-secret, anonymous) before any consensus byte
  crosses the wire, so an on-path observer can no longer read votes or
  proposals in cleartext or forge a frame the AEAD tag won't catch (D-0206,
  closing the founder's 2026-07-12 in-depth review's `5.3`/`5.4` "wire
  authenticated encrypted channels into consensus now" finding — no new
  cryptography, the same construction `mini-sync`/`mini-cli`'s `sync
  connect`/`listen` already use). **State-sync/catch-up is shipped**
  (D-0093): `mini_consensus::{CatchupRequest, CatchupResponse, FinalizedBlock}`
  plus `ConsensusNode::{history_since, catch_up}` let a node that missed
  heights pull already-finalized blocks from a peer and re-verify/apply
  them via the same `apply_finalized_block` call live consensus uses —
  never a trust shortcut. Proven over real TCP
  (`a_late_joining_node_catches_up_via_real_tcp_and_matches_the_clusters_state`):
  a fifth node that never runs a single Tendermint round reaches the exact
  state a four-node cluster converged on. First slice: history is
  unbounded in-memory (no pruning/persistence), and no peer-selection/retry
  policy. The equivocation evidence is no longer silently dropped by
  the network driver (D-0088: `mini_consensus::EquivocatorRegistry`
  independently re-verifies and records every flagged root instead of
  discarding the emit), but nothing yet *acts* on a flagged root — no
  exclusion, no economic penalty, no slashing — since dynamic validator-set
  transitions don't exist yet. Peers are supplied not discovered,
  `Channel`'s handshake is anonymous so it proves nothing about *which*
  validator is on the other end, and the demonstration is threads over
  loopback. Wiring `mini-net`'s PEX discovery into mesh peer supply, acting
  on equivocation, and dynamic validator sets are the named next slices
  (roadmap Phase 5, [#36](../../issues/36)-[#45](../../issues/45);
  `docs/design/networked-consensus.md`).

## 2. Personhood

- **prototype** — `mini-uniqueness::status` (D-0038): open-ended
  multi-signal `HumanRecord`/`TrustWeights`/`PromotionPolicy` accumulator.
  Real, tested code. Hardened per the #18 Sybil review (D-0054): reaching
  `EvidenceQualifiedHuman` now requires a *live* seed-anchored vouching-graph signal,
  closing a farm-saturation bypass — see
  `docs/audits/issue-18-sybil-social-graph-review.md`. Renamed from
  `FullHuman` (D-0086, founder review's `personhood-honesty` finding):
  the old name could read as a verified-personhood guarantee this crate
  does not provide — Sybil resistance is still unsolved.
- **reviewed** — presence attack review ([#17](../../issues/17),
  `docs/audits/issue-17-presence-attack-review.md`): replay/binding/clone
  defended; active relay is NOT defended by software RTT alone (needs UWB
  distance-bounding) — presence is safe only as a *weighted* signal.
- **design-only / research-blocked** — signal (b), redefined by D-0075
  from raw behavioral/location entropy into a "Private Human Continuity
  Proof" (`docs/design/human-continuity-proof.md`). The redefinition is
  decided; no `EvidenceStamp` type, pairwise-pseudonym derivation,
  nullifier registry, or aggregate ZK proof exists yet. Not a code gap
  alone anymore — five implementation phases plus a separate funded
  research program (Tracks A-F) ([#21](../../issues/21)).
- **HARD LIMITATION, not partial** — every "verified identity" counted
  anywhere in this tree today is a verified `did:mini` root, not a
  verified human. See `docs/INVARIANTS.md`'s hard-limitation section.
  Sybil-resistance at real-world scale is unproven
  ([#18](../../issues/18)).
- **partial** — co-presence attestation (`mini-presence`) is shipped;
  the software RTT bound has no hardware ranging backing it yet in
  production use (UWB trait scaffolded, not wired to real hardware).
- **doc-only** — `docs/design/credential-taxonomy.md` (D-0089, founder
  review's `credential-separation` finding) names and separates
  `ParticipantCredential`/`HumanEvidence`/`RoleCredential`/
  `ResourceCredential` against mechanisms that already exist above; it
  introduces no new type and states plainly that `UniqueHumanCredential`
  remains unbuilt Phase 2 work.

## 3. Identity & key custody

- **shipped** — `did-mini`: KEL, pre-rotation, device delegation,
  detached signing, decoder hardening, and now **lost-device/death
  recovery** (`Controller::recover_from_kel`, D-0053) from escrowed
  next-key seeds. Security-audited ([#12](../../issues/12),
  [#13](../../issues/13)): 3 findings fixed
  (threshold-policy rewrite, delegated-acting-as-root, seed scrubbing).
  Logic-complete, hardened, tested.
- **partial / launch-blocking** — KEL freshness & duplicity detection: a
  stale root KEL still accepts a revoked device (audit #12 F4). The
  interim rule (pin highest sn seen per SCID) is now real code —
  `did_mini::FreshnessPins` (D-0088) — not only a documented
  recommendation, closing the case where a verifier has already seen a
  fresher KEL. Real witness receipts and gossip-based duplicity proofs
  (SPEC-01 §7, M3) remain the owner of the harder case: a verifier who
  has *never* seen the fresher log.
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
- **design decided, unimplemented** — the treasury economic model (D-0073,
  `docs/design/treasury-economic-model.md`: XRPL/XMR bridge split,
  contribution epochs, oracle/vesting/issuance-ceiling mechanism) and the
  long-term issuance/anti-whale model (D-0074, `docs/design/
  inflation-and-whale-resistance.md`: 3%/2%/0.75%/0.25% envelope, formal
  anti-whale governance-input wall) replace the whitepaper's original BTC/
  XMR framing and #50's open question. Neither's parameters are wired into
  `mini-treasury::rate`/`receipt` or a chain state machine yet, and neither
  has run the adversarial simulation suite `docs/gates/
  economic-simulation-spec.md` still requires before real value depends on
  the calibration. §9's cellular custody design now states explicitly
  (D-0089, founder review's `custody-separation` finding) that a
  bridge-specific vault's signer committee and the general treasury's
  signer committee are always disjoint sets — no individual holds a seat
  on both; this was already implied by the cellular design, not a new
  rule.
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
- **shipped** — consensus edge-case attack review (D-0085/D-0087, closes
  roadmap #44): `LedgerChain::apply_finalized_block` and `mini-consensus`'s
  `validate_proposal` now require `timestamp_ms` to equal the block's own
  height exactly — deterministic logical time, tightened from an initial
  monotonicity-only bound after noting a merely-increasing value could
  still evade it; `mini_chain::Vote`'s signed transcript gained a
  domain-separation tag it was the one signed-transcript type in this
  workspace missing; `mini_value::fee::PriceHistory` and
  `fee_in_micro_mini` both reject a governed price of zero, and
  `fee_in_micro_mini` now rejects (rather than silently truncates) a fee
  quote too large for `u64`. All hardening within already-decided
  constructions — see `docs/THREAT_MODEL.md` §2 for the honest
  status/limits of each (a real wall-clock consensus protocol and a price
  rate-limit bound remain explicit, undecided follow-up, not silently
  claimed as done).

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
  software if there was nothing to fall back to), and, since D-0076, a
  persisted, hash-chained, independently-verifiable event log alongside
  the in-process type-state pipeline (`verify_install_event_log`; the log
  is evidence of what happened, never permission for anything to happen).
  Unix-only; no process supervision; no real package-manager/OS
  integration -- honest limits stated in the crate's own docs. 17
  adversarial/integration tests against real files on real disk.
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
- **shipped** — `mini-privacy-policy` (D-0094): the founder research's
  "cost doctrine" turned into typed vocabulary —
  `ProtectionProperty`/`Mechanism`/`ResidualFloor` (the five floors F1-F5
  no spend removes) — plus a Tier 0-3 (Direct/Relayed/Mixed/Burst)
  `PrivacyRequest`/`AchievedPrivacy` policy object with a hand-rolled wire
  codec. Pure policy data only — **no relay, mix, or erasure-replication
  mechanism exists yet for any tier above Direct**; `expected_cost`
  reproduces the research document's own estimates, not a benchmark.
  Founder research: `docs/research/MININET_RESEARCH_V2_20260713.md`;
  phase sequencing: `docs/research/PARALLEL_CONTRIBUTOR_PROGRAM_20260713.md`.
- **not started** — Tier 1+ relay/rendezvous transport, mix network,
  `ObjectEnvelope` v2 private-metadata boundary, capability/pseudonym
  primitives (the phase P1/P2 items this same research names next); the
  storage fabric's P6 guarantees (no forced replication, no compelled
  decryption) also have no owning subsystem yet.
- **planning artifact** — `docs/design/
  privacy-cost-doctrine-parallel-execution-plan.md` (D-0300): five
  disjoint-footprint lanes (L1-L5) for the immediately-unblocked next
  slice of this work, sized so several contributors can develop them
  concurrently and each still batches into one PR. Opens the `D-03xx`
  decision-number band for this track. No lane has started yet.

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
  `InProcessBearer`. Extended (D-0091, founder review P1 item "resumable
  peer-to-peer bootstrap capsule transfer"):
  `a_connection_killed_mid_transfer_over_real_tcp_is_safely_resumed_by_a_
  fresh_connection` kills a real TCP connection strictly mid-transfer
  (partway through a 300-object pull, not at the pre-handshake stage the
  older `interrupted_sync_resumes_by_idempotence` test used) and proves a
  fresh connection still converges the two stores completely — precisely
  scoped as "safe idempotent retry-from-scratch," not byte-offset resume
  within one transfer, since `pull()` only ingests after its whole
  want-round completes.
- **shipped** — local-network peer discovery over UDP multicast (D-0091,
  founder review P1 item "Local-Wi-Fi/mDNS adapter"):
  `mini_bearer::LocalAnnouncer`/`LocalScanner` — a minimal, Mininet-owned
  announce datagram (explicitly not full mDNS/DNS-SD RFC 6762/6763),
  carrying no identity, that discovers a peer's bearer address on the same
  local network with no central server and hands it straight to
  `TcpBearer::connect`. `docs/gates/wifi-bearer-test-protocol.md` still
  gates whether this signal is *trustworthy* co-presence evidence (needs
  real routers/phones/VPN attack testing, W1-W7) — this only builds the
  discovery mechanism the gate goes on to test, and makes no trust claim
  of its own.
- **shipped** — invitation/peer-exchange discovery over real TCP (D-0092,
  founder review P1 item "invitation and peer-exchange discovery with no
  required central server"): `mini_net::pex` —
  `PexMessage::{Request, Response}` (a minimal, hand-rolled wire
  protocol, the first real wire-message design in this crate),
  `AddressBook` (pairs a `PeerId` with a dialable `SocketAddr` —
  `RoutingTable` alone was never dialable), `build_response`/
  `absorb_response`. A node supplied only one peer's address is proven,
  over a real TCP socket
  (`a_node_discovers_a_second_peers_address_purely_through_pex_over_real_tcp`),
  to discover a *second* peer's dialable address purely through one PEX
  round with the first — and the discovered address is proven actually
  dialable, not just present in a data structure. `AddressBook::insert`
  is first-seen-wins so a later, hostile PEX response can never silently
  redirect who a caller dials for an id it already resolved; a response
  is capped at `MAX_PEX_RECORDS` so it can never become an unbounded
  memory/bandwidth sink. `mini-net`'s gossip logic is still proven live
  over real sockets separately from this; the two aren't wired together
  yet (that integration — routing PEX-discovered peers into gossip
  fanout — is follow-up, not done here).
- **partial** — `mini-net`'s gossip logic is proven live over real
  sockets; peer *discovery* (`RoutingTable`) is unexercised over a real
  transport as part of an actual mesh (PEX above proves the discovery
  *mechanism* over real TCP, but nothing yet drives gossip fanout or
  routing-table refresh from it end to end).
- **not started** — BLE radio adapter (needs real phone hardware,
  [#22](../../issues/22)); NAT traversal; local mesh routing.

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
  code at all — business/process work; founder review's `audit-program`
  P0 item, confirmed by the founder this session as staying entirely
  outside repository scope).
- **shipped** — `docs/CONSTITUTION_REGISTRY.json` (D-0090, founder
  review's `constitution-registry` P0 item): the seventeen
  `docs/FOUNDER_DIRECTIVES.md` directives, generated (not hand-maintained)
  into stable IDs (`FD-01`–`FD-17`) with an exact digest per directive by
  `tools/constitution_registry.py`, so future reviewers and tooling have
  one machine-readable source instead of the review-flagged
  six-vs-eleven-vs-seventeen ambiguity across SPEC-00/v2/this repo.

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
  environment, not yet a live distributed system — and, since D-0076,
  "verifiable event history" is now a real persisted, hash-chained,
  independently-verifiable log (`verify_install_event_log`), not just
  typed in-process return values. Honest limits: Unix-only
  (`symlink`/`rename` activation), no process supervision, no real
  package-manager/OS integration — see §5 for the full detail; 25 tests
  (17 pipeline/event-log tests plus 8 covering the cross-process
  reconstruction methods D-0077 added).
- **shipped** — `mini build`/`release`/`provenance`/`installer` CLI
  subcommands (D-0077), closing PR #109's own named gap ("no CLI
  subcommand yet"). `mini build run` spawns the real
  `mini-build-runner-wasmtime` binary as a genuine subprocess (never
  linked in-process, preserving D-0069's isolation boundary); `mini
  release`/`provenance` thinly wrap the already-real `mini-forge`/
  `mini-provenance` library calls; `mini installer <step>` drives the
  real `Installer` pipeline one step per CLI invocation, using three new
  `Installer` methods (`staged_release`/`preflight_passed`/
  `activation_record`) to reconstruct minimal typed pipeline state from
  the persisted D-0076 event log across the process boundary a
  stateless CLI can't otherwise cross — each refusing to reconstruct
  anything the log doesn't show genuinely happened. Proven through the
  real text-based CLI (not direct library calls) in
  `crates/mini-cli/tests/cli_spine_commands.rs`.
- **shipped** — stable `--json` output for `build`/`release`/
  `provenance`/`installer` (D-0078), closing the gap the D-0077 bullet
  above used to name. A global `--json` flag makes those commands emit
  `{"ok":true,"kind":"<verb.noun>",...fields}` /
  `{"ok":false,"kind":...,"error_code":...,"message":...}` instead of
  human text, with a real typed field per created/inspected object (a
  release id, a digest, an attester count) — a caller now chains
  commands by reading a field, never by scraping a human sentence.
  Hand-rolled emitter (`crate::json`, no `serde`/`serde_json`
  dependency, matching this workspace's established encoding
  convention). `identity`/`kel`/`repo`/`pr`/`sync` still have no
  `--json` support and cleanly reject the flag (a scripting caller must
  never silently get human text back). `crates/mini-cli/tests/
  cli_json_output.rs` proves a real field extracted from one command's
  JSON chains directly into a second command, and drives the actual
  compiled `mini` binary as a subprocess to prove the error-envelope
  path (which lives in `main.rs`, outside `mini_cli::run`'s own
  `Result<String>` contract).
- **shipped** — adversarial `release`/`installer` CLI fixtures (D-0079),
  fulfilling the follow-up D-0077/D-0078 both named. 10 tests drive the
  real CLI against specifically adversarial inputs — a lone real
  attester, an author's self-attestation, a duplicate attestation from
  one identity, an attestation naming the wrong digest, a too-early
  `release verify`, a wrong-branch `release verify`, `installer activate`
  before `preflight`, `installer preflight` on a never-staged release —
  proving D-0077's CLI-level state reconstruction introduces no bypass
  of any safety property the underlying libraries already enforce. A
  tenth sanity-anchor test confirms the identical setup verifies
  successfully once every condition is genuinely met, so the failures
  above are proven to fail for the right reason.
- **shipped** — Batch 5, first piece: `mini sync listen`/`mini sync
  connect` (`mini-cli::sync`), live network peer exchange over a real TCP
  `mini_bearer` + `mini_sync` connection — Batch 1's remaining deferred
  item. `tests/network_sync.rs` proves two `mini` homes with completely
  independent, unshared stores reach the same governed merge purely over
  the network. `listen` accepts one peer by default or exactly `--repeat
  <n>` peers sequentially (no daemon, no concurrency, no signal-based
  shutdown); `connect` always dials exactly one peer.
- **shipped** — Batch 5, second piece: the full spine reaches a peer
  purely over `mini sync`, not just the governed merge (D-0080). No new
  code — `mini_sync::sync_bidirectional` already replicates every signed
  object in the store type-agnostically — but `tests/
  network_sync_release.rs` is the first proof of it: three identities do
  governance/release/attestation entirely in one local store, a fourth
  identity whose store has never touched that filesystem connects once
  over real loopback TCP, and then — using only what arrived over that
  one connection — independently runs `release verify` and the full
  `installer stage → preflight → activate → health-check` sequence to a
  genuinely active, passing install.
- **shipped** — the no-GitHub outage demo (D-0081). `tools/
  no_github_outage_demo.sh` is a real, narrated shell script — driving
  the compiled `mini` binary, never a library call — that carries three
  identities through the entire spine in one continuous run: identity,
  KEL trust, governed merge, release, two independent attestations,
  install, a passing health check — then a second, deliberately broken
  release through the identical path that fails its health check,
  auto-rolls back with no manual intervention, and leaves an
  independently-verifiable clean event log. Exercised by `cargo test
  --workspace` via `tests/no_github_outage_demo.rs`, which runs the
  script itself as a real subprocess so a broken demo fails CI like any
  other regression. Honest limit: this environment has no controlled
  way to actually sever GitHub reachability and verify nothing breaks —
  the claim rests on the codebase's dependency graph (no GitHub-API
  client dependency exists anywhere) plus this script's own successful
  run, not a live firewall drill.
- **shipped** — Git SHA-256 export bridge (`mini_forge::git_export`),
  Batch 1's remaining deferred item. Exports a commit chain (commit → tree
  → blobs, recursively through every ancestor) as real git SHA-256-object-
  format bytes/ids — verified in `tests/git_export.rs` against the actual
  `git` binary (`git hash-object`, `git mktree`, `git commit-tree`), not
  just self-consistency. Export only, one direction; import (parsing an
  arbitrary git repository into this tree's own signed object model)
  remains genuinely unstarted.
- **not started** — `mini-devd` (local daemon), machine-readable
  `STATUS.md`/roadmap generation (Batch 1's remaining deferred items);
  wiring `mini-installer` into an actual running system (Batch 4's own
  named next step, the caller's job by design); the rest of
  Batch 5 (local object indexing at scale, distributed build workers,
  GitHub import/export mirror automation).
- **partly active, mostly specified** — the founder-supplied Governance Pack
  v1.0 plus the v1.1 charter delta (`docs/governance/`, `forge-native/`,
  `governance/`; D-0082–D-0084): ~50
  normative process/specification documents, five RFCs, and JSON Schemas
  for a future signed Forge-native governance-object encoding, all
  explicitly subordinate to `docs/FOUNDER_DIRECTIVES.md`/
  `docs/INVARIANTS.md`/`docs/DECISION_LOG.md`. The only things actually
  *active* are the GitHub issue forms, the content-addressed non-authorizing
  Primary AI Engineer Charter and `AGENTS.md` adapter, security/dependency
  settings, a temporary Founder-operated pull-request-only `main` profile,
  a blocking candidate baseline, a canonical base-branch evaluator for later
  PRs, and live CODEOWNERS routing to the Founder with zero required approval.
  The scoped-team `CODEOWNERS.template` remains inert until those humans exist. See
  `docs/GOVERNANCE_PACK_INTEGRATION.md` for the full compatibility
  matrix and what's staged vs. founder-only.

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

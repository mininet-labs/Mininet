# CLAUDE.md — agent context for working on Mininet

> **Session authority boundary:** Read repository-root `AGENTS.md` before using
> this Claude-specific context. This file adds tool and current-code context
> only. It grants no Mininet Authority and cannot override the canonical
> Constitution, invariants, decisions, activated governance, or the exact
> charter identified by `AGENTS.md`. If the `AGENTS.md` activation fields are
> empty, the proposed charter is not active.

This file is loaded automatically at the start of every Claude Code session.
It exists so the agent starts *oriented* instead of re-deriving the project's
structure, current code, and engineering rituals from scratch each time. The
2026-07-08 Founder direction applied to the completed work recorded in canonical
history; it is not standing authorization for future scope. Keep this context
current: when a convention changes, change it in the same proposal.

## What this project is

Mininet: a constitutional P2P protocol — identity, personhood, money,
storage, governance — built in Rust as ~33 `mini-*` crates (two,
`mini-cli` and `mini-build-runner-wasmtime`, are binaries), designed to
outlive its creators (think in centuries, not releases). The founder may give
engineering direction via chat and currently performs the mechanical GitHub
PR merge action under the active bootstrap operating decision. Chat direction
is task authorization, not AI approval or protocol authority. GitHub is the
temporary operational canonical surface until a governed Forge cutover; it is
never constitutional authority. The long-term source of truth is the network
governing itself (`mini-forge`).

## Canonical sources — load what the task requires

Use repository-root `AGENTS.md` to resolve precedence and session scope. Read
the relevant sources below for ordinary work and the complete set for broad,
cross-system, constitutional, or governance-sensitive work. Reading order is
not authority precedence.

1. `docs/FOUNDER_DIRECTIVES.md` — 17 directives; the WHY under everything.
   Never contains implementation detail. Every judgment call traces here.
2. `docs/INVARIANTS.md` — what can NEVER be broken. Stable IDs (P1…, M1…,
   ID1…, X1…) + a **Directive** column: the traceability chain is
   `Directive → Invariant → Source (SPEC/D-number) → Enforced by (crate+test)`.
   Two "hard, temporary limitations" at its top must never be papered over:
   identity-root ≠ verified human (Sybil unsolved), and proof-of-space-time
   proves possession, not replication uniqueness.
3. `docs/DECISION_LOG.md` — append-only. D-0001–D-0084 so far. **Never edit
   old entries**; supersede with a new one. From D-0045 on, entries use the
   7-field template (Decision/Reason/Constitutional impact/Implementation
   status/Failure point/Required follow-up/Supersedes). Constitutional impact
   must cite IDs ("Directive 4, M2"), not prose.
4. `docs/FAILURE_BOOK.md` — paths tried and rejected. Check it before
   proposing anything, so rejected designs aren't re-proposed.
5. `docs/THREAT_MODEL.md` — civilization-scale threats (human/technical/
   economic/political/civilization) with per-threat "stopped by" invariants.

Supporting: `docs/STATUS.md` (living what's-built account — update it when
shipping), `docs/design/` (design notes that close roadmap issues —
`self-hosted-forge-spine.md` is the current top priority, D-0066),
`docs/governance/` (the founder-supplied Governance Pack — normative
process/specification material, explicitly **subordinate** to the five
canonical documents above; never edit it into a rival constitution — see
`docs/GOVERNANCE_PACK_INTEGRATION.md` for the compatibility matrix and
what's activated vs. staged vs. founder-only),
`docs/audits/` (audit deliverables, `issue-N-*.md` naming),
`docs/ADDRESSING.md` (no-DNS addressing), README's repo map.

## Hard rules (violating any of these is the only real failure mode)

- **Voice/value wall (P1, Directive 16):** no dependency edge may ever
  connect value crates (mini-value, mini-bounty, mini-treasury) to
  governance/review crates (mini-forge, mini-chain voting) in either
  direction. Check `Cargo.toml` diffs for this on every PR.
- **Frozen invariants are frozen.** Adding rules is fine; weakening any
  Tier-F row in INVARIANTS.md requires the lawful constitutional amendment
  and unfreezing process, an exact-state canonical Decision, and every
  applicable external gate. Founder direction, repository ownership, the
  temporary D-0083 integration exception, or ambiguity cannot substitute for
  that process; ambiguity defaults to denial.
- **Append-only history:** never rewrite merged commits, never reformat old
  decision-log entries, never delete a threat/failure entry (mark
  resolved/superseded instead).
- **Honesty over polish:** every crate/doc states plainly what is NOT built,
  NOT audited, NOT anonymous, NOT enforced. Overclaiming is treated as a
  bug. Prototypes stay gated behind D-0037/D-0047 (external audit before
  real value) — never soften that language.
- **No inventing cryptography.** Composing already-reviewed *primitives*
  (mini-crypto's Ed25519/X25519/AEAD/BLAKE3) or implementing an already-
  *published, peer-reviewed, real-world-deployed construction* end-to-end
  in-house (Bulletproofs in mini-value D-0036/D-0040; SDR-style proof-of-
  replication in mini-porep D-0064) is fine — that's composition of prior
  art the wider field has already vetted, done ourselves rather than
  outsourced to another project's codebase, to keep governance in-house
  (D-0063). What's forbidden is a *genuinely novel, unreviewed*
  cryptographic design nobody outside this repo has ever analyzed.
  Simplicity is security (Directive 14): prefer the smaller, well-trodden
  construction over a bespoke one whenever either would do.
- **Never claim "one human, one vote."** Everything today counts identity
  roots. Say "identity root" until SPEC-02 personhood actually lands.
- **Typed domains, never generic `sign(bytes)`/`finalize(state)`.** Any
  function that exercises real authority (signing, finalizing money,
  marking a status, adopting a release, deleting content) must take a
  specific, named request type (`sign_release_attestation(ReleaseAttestation)`,
  not `sign(&[u8])`) so the set of things that authority *can* do is fixed
  at compile time, not by whatever bytes a caller assembles. A generic
  authority-shaped signature is a standing invitation to grow an
  undocumented capability later — reject it in review the same way a
  voice/value dependency edge gets rejected.

## Workflow ritual (what the founder expects every batch)

1. Work on the designated contribution branch; while D-0083 is active, the
   Founder performs the mechanical GitHub merge after the PR's required
   checks and exact-head review evidence are complete ("I merged" = sync
   from main and continue). AI reviews carry zero approval weight.
2. Batch related work into one PR; update the PR title/body as scope grows.
3. Before every commit: `cargo fmt --all` →
   `cargo clippy --all-targets --all-features --workspace -- -D warnings` →
   `cargo test --workspace --all-features` → regenerate the nav index:
   `python3 tools/mininet_nav.py build`.
4. Ship each decision as a D-number; bump README's `D-0001–D-00xx` range and
   repo map when docs/crates are added. **Parallel tracks are banded** to
   avoid colliding on the same next number: the main/operational line uses
   `D-00xx`; the networking & consensus track (roadmap #36–#45) allocates
   from `D-0200` up. Full policy at the top of `docs/DECISION_LOG.md`
   ("Decision-number allocation across parallel tracks").
5. GitHub issues: the roadmap is #8–#93 with hub/index issue **#92** — keep
   its checklist current. Close issues only when merged work genuinely
   discharges them; use "Ready to close once PR #N merges" comments and let
   ambiguous ones stay open for founder review. Never close partially-done
   issues — mark them 🟡 in #92 instead.
6. Commit messages: descriptive, reference issues/D-numbers. No model IDs in
   anything pushed to the repo.

## Codebase map (the 30-second version)

- `mini-crypto` — hashing/signing/AEAD/KDF foundation; zeroize discipline.
- `did-mini` — KERI-style identity: SCIDs, KELs, pre-rotation, delegation,
  recovery (`recover_from_kel`), pairwise pseudonyms. Everything roots here.
- `mini-presence` / `mini-uniqueness` — co-presence attestation / personhood
  signal fusion (Sybil resistance = THE open question, roadmap #18).
- `mini-chain` — BFT finality verification, equal weight per identity root.
  `mini-settlement` — offline payment claims, M1/M2/M3 (D-0055).
  `mini-execution` — chain-backed `CanonicalLedgerView` tying the two
  together (D-0061, closes #40); still not networked consensus (#36-#45).
- `mini-value` — stealth addresses, ring signatures, Bulletproofs (D-0036
  prototypes). `mini-bounty` composes them for anonymous dev bounties.
- `mini-treasury` — FROST threshold custody; real DKG + resharing now
  exist (D-0059/D-0060) but are unaudited (#93). `mini-spacetime` —
  possession-only storage proofs (Merkle/PDP). `mini-porep` — real
  proof-of-replication (D-0064, closes #31): sequential SDR-style sealing
  distinguishes many honest holders from one warehouse; unaudited.
  `mini-erasure` — Reed-Solomon erasure coding + self-healing shard repair
  (D-0065, closes #30/#32; generator-matrix MDS bug an external review
  found fixed in D-0072 — normalize the full Vandermonde matrix against
  its own top block, don't just append raw parity rows to an identity
  block); coding logic only, not wired to real network distribution.
- `mini-forge` — code governance: per-root approvals, 2-approval protocol
  floor, KelDirectory oracle, plus informational (never quorum-counted)
  AI-assistance declarations and review findings (D-0067); timelocked,
  independently-attested release registry plus rollback protection and a
  release transparency log (`release` module: `Version`,
  `check_no_rollback`, `list_releases`, `detect_equivocation`; D-0070,
  spine Batch 3); git SHA-256 export bridge (`git_export`, real-git-
  verified, export-only — import unstarted). `mini-cli` — the
  `mini` binary, a real developer tool over `mini-forge` (D-0067,
  self-hosted forge spine Batch 1, #102); `mini sync listen`/`connect`
  (spine Batch 5) reaches the same governed merge over a real TCP
  connection with no shared filesystem, one connection per invocation, no
  daemon yet; `mini build run`/`release create|attest|verify|list`/
  `provenance record|verify`/`installer stage|preflight|activate|
  health-check|rollback|status|history|verify-log` (D-0077) wire the rest
  of the spine into real subcommands — `installer`'s subcommands
  reconstruct minimal typed pipeline state from the persisted event log
  across separate CLI invocations (`Installer::staged_release`/
  `preflight_passed`/`activation_record`), since a process boundary can't
  carry a type-state value the way an in-process caller can; a global
  `--json` flag (D-0078) makes those four command groups emit a
  single-line `{"ok":true,"kind":...,...fields}` /
  `{"ok":false,"kind":...,"error_code":...,"message":...}` envelope
  instead of human text — hand-rolled (`crate::json`, no serde), a real
  typed field per created/inspected object (a release id, a digest, an
  attester count) so chaining reads a field instead of scraping a
  sentence; `identity`/`kel`/`repo`/`pr`/`sync` still have no `--json`
  support and cleanly reject the flag rather than silently ignoring it.
  `mini-provenance` — SLSA/in-toto
  build provenance signed objects + independent-builder agreement
  counting (D-0068, spine Batch 2a); records/counts claims, runs no build
  itself. `mini-pipeline`/`mini-pipeline-protocol` — pure pipeline
  manifest/policy/capability types and content-addressed request/result
  messages (D-0069, spine Batch 2b.1); no Wasmtime dependency, ever.
  `mini-build-runner-wasmtime` — the isolated Wasmtime executor for
  `wasm-component` pipeline steps (D-0069, spine Batch 2b.2/2b.3); the
  ONLY crate in this tree permitted to link `wasmtime`/`wasmtime-wasi`,
  deny-by-default capability model, fuel/epoch/memory limits, 12-point
  adversarial exit-criteria suite driving the real compiled binary.
  `native-tool` (raw shell) pipeline steps remain unsandboxed and never
  trusted-provenance-eligible until a separate OS-isolated mechanism is
  designed and decided. `mini-net` — DHT/gossip over real TCP.
- `mini-bearer`/`mini-bootstrap`/`mini-sync`/`mini-update` — transport,
  BLE-first bootstrap, CRDT sync, self-contained updates. `mini-update`'s
  `AdoptionState` layers device-local freshness/staleness bounds
  (`FreshnessPolicy`) and an optional independent build-provenance quorum
  gate (`ProvenancePolicy` + `evaluate_with_provenance`, over
  `mini-provenance`) in front of `mini-forge`'s release verification
  (D-0070, spine Batch 3); still no forced update, no kill path, nothing
  here executes/fetches/installs anything. `mini-installer` — the separate
  layer that actually does (D-0071, spine Batch 4): stage/preflight/
  activate/health-check/rollback over an already-verified release, a
  type-state pipeline mirroring `Discovered → ... → Active` or
  `RolledBack`; `activate` requires a caller-constructed `OwnerApproval`
  naming the exact release id (typed-domain rule), and a failed health
  check auto-rolls-back to whatever was already running rather than
  forcing anything forward. Unix-only (symlink/rename activation), no
  process supervision, no real package-manager/OS integration.
- `mini-store`/`mini-storage`/`mini-reward`/`mini-social`/`mini-objects`/
  `mini-media`/`mini-crdt`/`mini-keystone` — storage tiers, receipts,
  rewards, walls, object model, the two-device keystone demo.

Find anything: `python3 tools/mininet_nav.py map` (see `docs/NAVIGATION.md`).

## Current priority (D-0066 — Batches 1-5 shipped; widening into Batch 6/Branches A-D is the founder's call)

A founder-adopted external audit found implementation breadth had run
ahead of vertical integration: no complete path existed from developer
change → review → governed merge → reproducible build → release finality
→ safe install → rollback. Batches 1-4 of `docs/design/
self-hosted-forge-spine.md` closed that path end to end (`mini-cli` →
`mini-provenance`/`mini-build-runner-wasmtime` → `mini-forge::release`
rollback/transparency-log/freshness/provenance gates →
`mini-installer`'s real stage/activate/health-check/rollback). Batch 5
(Mininet as the primary forge) is now also shipped: `mini sync
listen`/`connect` reaches a governed merge with no shared filesystem
(D-0066 Batch 5 piece 1); `mini_sync::sync_bidirectional`'s existing
type-agnostic replication was proven to carry the *entire* spine —
release, attestation, install — to a peer over a real TCP connection
alone (D-0080); `mini build`/`release`/`provenance`/`installer` CLI
subcommands (D-0077) plus stable `--json` output (D-0078) plus
adversarial CLI fixtures (D-0079) closed the "still too library-internal"
gap the audit named; and `tools/no_github_outage_demo.sh` (D-0081) is a
real, runnable, narrated script proving the whole lifecycle — including
a deliberately broken release's automatic rollback — completes with
GitHub never named or required. **Do not re-propose "build a
proposal/review/merge object model" as new work: it already exists in
`mini-forge::governance`** (`propose`/`approve`/`merge`/`amend`/
`resolve_project`), predating the audit. What's next — resuming Batch 6's
horizontal roadmap breadth vs. Batch 5's remaining pieces (local object
indexing at scale, distributed build workers, GitHub import/export mirror
automation) vs. Branches A-D (hardware #97/#98, economics #47/#50,
personhood #21, DTN #28) — is a priority call for the founder to make,
not something to pick unilaterally; see the design doc for what each
entails.

## Current launch blockers (keep these in view as horizontal work resumes)

1. Sybil/personhood economics — #18, the sharpest open question.
2. KEL freshness/witnesses (M3) — stale-KEL revocation gap, audit #12 F4.
3. FROST DKG — implemented and tested (D-0059/D-0060); external audit still
   open, #93 (P0, D-0048).
4. Real BLE transport + client app — needs hardware, not startable here.
5. External crypto audit — #72, gates everything value-bearing (D-0047).

## Session hygiene for the agent

- Scratch work goes in the session scratchpad, never committed.
- `target/` noise: ignore it in searches (`--glob '!target'`).
- The account/repo was renamed `britak420/matej` → `britak420/Mininet` →
  `mininet-labs/Mininet`; git remotes may still use an old slug — GitHub
  redirects them, so all work. In-repo doc links use repo-relative form
  (`../../issues/N`) so they survive any future rename.
- When the founder gives a large multi-part directive, create tasks
  (TaskCreate) immediately and tick them as you go; he reads the checklist.
- When uncertain whether something is decided or open: DECISION_LOG first,
  then FAILURE_BOOK, then ask — never guess a policy into existence.

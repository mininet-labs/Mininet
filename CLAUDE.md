# CLAUDE.md — agent context for working on Mininet

This file is loaded automatically at the start of every Claude Code session.
It exists so the agent starts *oriented* instead of re-deriving the project's
structure, rules, and rituals from scratch each time. Founder-approved
(2026-07-08: "design it and implement how you see fit"). Keep it current:
when a convention changes, change it here in the same PR.

## What this project is

Mininet: a constitutional P2P protocol — identity, personhood, money,
storage, governance — built in Rust as ~32 `mini-*` crates (two,
`mini-cli` and `mini-build-runner-wasmtime`, are binaries), designed to
outlive its creators (think in centuries, not releases). The founder directs
via chat and merges via GitHub PRs. GitHub is the UAT/mirror; the long-term
source of truth is the network governing itself (mini-forge).

## The five canonical documents — read order for any non-trivial task

1. `docs/FOUNDER_DIRECTIVES.md` — 17 directives; the WHY under everything.
   Never contains implementation detail. Every judgment call traces here.
2. `docs/INVARIANTS.md` — what can NEVER be broken. Stable IDs (P1…, M1…,
   ID1…, X1…) + a **Directive** column: the traceability chain is
   `Directive → Invariant → Source (SPEC/D-number) → Enforced by (crate+test)`.
   Two "hard, temporary limitations" at its top must never be papered over:
   identity-root ≠ verified human (Sybil unsolved), and proof-of-space-time
   proves possession, not replication uniqueness.
3. `docs/DECISION_LOG.md` — append-only. D-0001–D-0070 so far. **Never edit
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
`docs/audits/` (audit deliverables, `issue-N-*.md` naming),
`docs/ADDRESSING.md` (no-DNS addressing), README's repo map.

## Hard rules (violating any of these is the only real failure mode)

- **Voice/value wall (P1, Directive 16):** no dependency edge may ever
  connect value crates (mini-value, mini-bounty, mini-treasury) to
  governance/review crates (mini-forge, mini-chain voting) in either
  direction. Check `Cargo.toml` diffs for this on every PR.
- **Frozen invariants are frozen.** Adding rules is fine; weakening any
  Tier-F row in INVARIANTS.md is not, without an explicit founder decision
  recorded as a D-number.
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

1. Work on the designated `claude/...` branch; founder merges PRs himself
   ("I merged" = sync from main and continue).
2. Batch related work into one PR; update the PR title/body as scope grows.
3. Before every commit: `cargo fmt --all` →
   `cargo clippy --all-targets --all-features --workspace -- -D warnings` →
   `cargo test --workspace --all-features` → regenerate the nav index:
   `python3 tools/mininet_nav.py build`.
4. Ship each decision as a D-number; bump README's `D-0001–D-00xx` range and
   repo map when docs/crates are added.
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
  (D-0065, closes #30/#32); coding logic only, not wired to real network
  distribution.
- `mini-forge` — code governance: per-root approvals, 2-approval protocol
  floor, KelDirectory oracle, plus informational (never quorum-counted)
  AI-assistance declarations and review findings (D-0067); timelocked,
  independently-attested release registry plus rollback protection and a
  release transparency log (`release` module: `Version`,
  `check_no_rollback`, `list_releases`, `detect_equivocation`; D-0070,
  spine Batch 3). `mini-cli` — the
  `mini` binary, a real developer tool over `mini-forge` (D-0067,
  self-hosted forge spine Batch 1, #102). `mini-provenance` — SLSA/in-toto
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
  here executes/fetches/installs anything.
- `mini-store`/`mini-storage`/`mini-reward`/`mini-social`/`mini-objects`/
  `mini-media`/`mini-crdt`/`mini-keystone` — storage tiers, receipts,
  rewards, walls, object model, the two-device keystone demo.

Find anything: `python3 tools/mininet_nav.py map` (see `docs/NAVIGATION.md`).

## Current priority (D-0066, supersedes the item below until Batch 4 lands)

A founder-adopted external audit found implementation breadth has run ahead
of vertical integration: no complete path exists from developer change →
review → governed merge → reproducible build → release finality → safe
install → rollback. **Until Batch 4 of `docs/design/
self-hosted-forge-spine.md` is done, new work goes there, not into more
horizontal roadmap breadth** — see that doc for the six-batch plan and what
in each batch is already real vs. genuinely missing. Do not re-propose
"build a proposal/review/merge object model" as new work: it already
exists in `mini-forge::governance` (`propose`/`approve`/`merge`/`amend`/
`resolve_project`), predating the audit.

## Current launch blockers (keep these in view once Batch 4 lands and horizontal work resumes)

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

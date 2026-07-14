# Parallel contributor program — intake summary (2026-07-13 package)

> Founder-supplied planning artifact, uploaded 14 July 2026 alongside
> `MININET_RESEARCH_V2_20260713.md` and adopted by D-0094
> (`docs/DECISION_LOG.md`). This is a **summary**, not a reproduction — the
> full 90-file package (phase docs, one GitHub-issue-shaped spec and one
> PR spec per work item, a machine-readable work registry) lives outside
> this repository until/unless the founder asks for its ~70 `MN-xxx` items
> to actually be published as GitHub issues. Per the package's own stated
> authority order, it is refinement of sequencing/scope, never a source of
> constitutional authority, and it may not weaken any frozen invariant.

## What the package is

A decomposition of the research doc's direction into 9 phases (`P0`-`P8`)
and ~70 work items (`MN-001`-`MN-805`), each with a GitHub-issue-shaped
spec and a matching PR spec, meant to let many contributors (human or AI)
claim independent, dependency-ordered slices without colliding.

## Phase list (exit criteria per phase: completion evidence or explicit
deferred/external-gate disposition; STATUS, not aspiration, decides what's
implemented; no frozen invariant weakened; no unresolved high-risk claim
hidden by green CI)

- **P0** — repository execution and governance activation (mostly docs/
  process: work registry, issue forms, CODEOWNERS, governance CI). Largely
  already satisfied by this repo's existing Decision Log/Failure Book/
  roadmap-issue practice; not re-done here.
- **P1** — cost doctrine and common object policy. `MN-101` (protection-
  property/resource-cost vocabulary) + `MN-102` (privacy tier policy
  object) shipped as `mini-privacy-policy` in this same batch (D-0094).
  `MN-103` (`ObjectEnvelope` v2 private-metadata boundary) and `MN-104`
  (capability rights/scoped pseudonyms) are the named next slice.
- **P2** — transport privacy ladder: `TransportRequest` policy router,
  Tier 1 relay/rendezvous, packet size classes, Sphinx-style mix research,
  mix node state machine, cover scheduler, bridge/pluggable transport,
  private lookup/DHT restriction.
- **P3** — private distribution and storage: private chunking/manifests,
  erasure placement across failure domains, anonymous upload/custody
  receipts, shard-repair coordination, private retrieval/PIR, huge-file
  pipeline.
- **P4** — human evidence and personhood honesty: credential classes and
  evidence registry, `EvidenceStamp` interface, private continuity proof,
  context nullifier/pairwise pseudonym design, aggregate proof prototype,
  external uniqueness adapter, Sybil-farm/coercion simulation. **Must
  reconcile with the already-shipped `mini_uniqueness::HumanStatus`/
  `EvidenceQualifiedHuman` naming (D-0086) rather than introduce a rival
  taxonomy** — not attempted in this batch.
- **P5** — consensus, settlement and dynamic membership: authenticated
  validator channels, persistent finalized history/bounded catch-up
  (`MN-502` — **already shipped**, independently, as D-0093's
  `mini_consensus::catchup` module, PR #130), dynamic validator-set
  transitions, equivocation consequence/restitution, partition/outage/
  rejoin harness.
- **P6** — economics and anonymous resource payment: price vector/quote
  engine, blind prepaid credential protocol review, anonymous redemption,
  privacy-pool subsidy policy, treasury/inflation/whale simulation.
- **P7** — forge, release and reproducible delivery: forge-native issue
  dependency objects, draft-proposal publication, integration dashboard,
  reproducible-build recipe expansion, governed release end-to-end test.
- **P8** — client, hardware and adversarial validation: real BLE bearer,
  local Wi-Fi/hotspot validation, protection/residual-risk UI, two-phone
  keystone beta harness, external audit/red-team readiness bundle.

## What was adopted this batch, and what wasn't

Adopted: the cost-doctrine vocabulary and Tier 0-3 policy object exactly as
`MN-101`/`MN-102` describe them (see `crates/mini-privacy-policy`), and this
phase list as the forward sequencing reference for `docs/STATUS.md` §6.

Not adopted here, left for founder/future-session call: publishing the
package's ~70 issues to GitHub (a large, mostly-ceremony action this
session deliberately did not take, consistent with standing "more code,
less documents" direction); `MN-103`/`MN-104` (ObjectEnvelope v2, capability/
pseudonym primitives — the next P1 code slice); any P2-P8 work.

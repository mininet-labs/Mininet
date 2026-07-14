# Privacy/cost-doctrine parallel execution plan

D-0300 (see `docs/DECISION_LOG.md`'s `D-03xx` band). Tracking issue:
[#132](../../issues/132), with one child issue per lane
([L1 #133](../../issues/133), [L2 #134](../../issues/134),
[L3 #135](../../issues/135), [L4 #136](../../issues/136),
[L5 #137](../../issues/137)) — claim a lane by commenting on its issue
before starting. Related: [#122](../../issues/122) asks for durable,
CI-enforced parallel-contributor infrastructure (Project board, issue
leases, atomic D-number allocation); these lanes are a lighter, hand-
maintained first step for one specific track and should migrate onto
#122's tooling if/when it lands, not duplicate it. Companion to
`docs/research/PARALLEL_CONTRIBUTOR_PROGRAM_20260713.md` (the phase list)
and `docs/research/MININET_RESEARCH_V2_20260713.md` (the source research).
That summary lists ~70 `MN-xxx` work items across nine phases; this
document turns the *next* ready slice of them into concrete **lanes** —
groups of work sized and scoped so that (a) several people or agents can
run lanes at the same time without their PRs touching the same files, and
(b) each lane still lands as **one PR**, not one PR per `MN-xxx` item,
per the founder's direction to batch more work per PR rather than
fragment it.

## Why lanes, not a flat issue list

A flat backlog of 70 issues either serializes (one PR at a time, slow) or
collides (two PRs touching the same crate at once, constant rebase pain).
A **lane** is a work grouping chosen so its file footprint is disjoint
from every other currently-open lane. Two lanes can be developed,
reviewed, and merged in either order with zero merge conflicts between
them — only ordinary rebase-onto-main noise in shared generated files
(`docs/_generated/`, `Cargo.lock`), which is mechanical, not a design
collision.

## Lane table (first wave — everything here is unblocked today)

| Lane | Issue | Work items | Footprint (crates/paths touched) | Blocked by | One PR? |
|---|---|---|---|---|---|
| **L1 — Object privacy boundary** | [#133](../../issues/133) | `MN-103` (ObjectEnvelope v2 private-metadata boundary), `MN-104` (capability rights + scoped pseudonym primitives) | `crates/mini-objects`, `crates/mini-crypto` (read-only reuse), `crates/did-mini` | `MN-101`/`MN-102` — **done**, D-0094 | Yes — `MN-104` is a thin layer over `MN-103`'s types; one PR ships both, same as `mini-privacy-policy` shipped `MN-101`+`MN-102` together |
| **L2 — Transport policy router** | [#134](../../issues/134) | `MN-201` (`TransportRequest` policy router) | new crate `mini-transport-policy` (depends on `mini-privacy-policy` only) | `MN-102` — **done**, D-0094 | Yes — single new crate |
| **L3 — Mix protocol research** | [#135](../../issues/135) | `MN-204` (Sphinx-style mix packet research and protocol specification) | `docs/design/` only — **zero Rust footprint** | `MN-101` — **done**, D-0094 | Yes — one design doc |
| **L4 — Resource pricing** | [#136](../../issues/136) | `MN-601` (resource price vector and quote engine) | new crate `mini-resource-pricing` (depends on `mini-privacy-policy` only) | `MN-101` — **done**, D-0094 | Yes — single new crate |
| **L5 — Human evidence taxonomy reconciliation** | [#137](../../issues/137) | `MN-401` (Human Evidence Credential classes and evidence registry), scoped *first* to reconciling naming against `mini_uniqueness::HumanStatus`/`EvidenceQualifiedHuman` (D-0086) before any new type lands | `crates/mini-uniqueness` only | none (P4 root item), but **higher scrutiny**: must not introduce a rival taxonomy — see D-0094's Required follow-up | Yes, if scoped to reconciliation + at most one new confidence-class type; a full aggregate-proof prototype (`MN-405`) is explicitly a later, separate lane |

No two lanes in this table share a crate. `L1` and `L5` are the only
lanes touching an *existing* crate at all; every other lane is additive
(a brand-new crate or a docs-only deliverable), which is what makes them
safe to run at the same time as everything else, including whatever
lane(s) run after this wave.

## Sequencing after wave 1

Once `L1` merges, `MN-104`'s capability primitives unblock `MN-202`
(Tier 1 relay/rendezvous protocol) and `MN-208` (private lookup/DHT
restriction) — those become **lane L6** (new: relay/rendezvous logic,
likely a new crate or `mini-net` extension — footprint decided when `L1`
lands and its actual public types are known, not guessed now).

Once `L2` merges, `MN-207` (bridge/pluggable transport interface) and
`MN-208` become buildable against a real router type instead of a
speculative one.

Once `L3` (research) lands, `MN-205` (mix node state machine) becomes a
lane — but per the source research's own Phase D gate ("do not market as
globally anonymous before" external crypto review exists), `MN-205`
should not start production-style implementation without the same
external-review posture already applied to `mini-value`/`mini-treasury`
(D-0047 gate). Flagging this now so wave 2 doesn't skip it.

This document is intentionally not a full 70-item schedule — it commits
to what's unblocked *today* and states the rule (disjoint footprint,
batch multiple `MN-xxx` per lane) so whoever plans wave 2 doesn't have to
re-derive it.

## Coordination mechanics for parallel lanes

- **Claim before starting.** Comment on the lane's tracking issue (or the
  hub issue if no per-lane issue exists yet) with which lane you're
  taking, so two contributors don't duplicate work — same protocol as
  `docs/research/PARALLEL_CONTRIBUTOR_PROGRAM_20260713.md`'s source
  package already specifies for `MN-xxx` claims.
- **D-number collisions within the D-03xx band are handled the same way
  every other collision in this repo's history has been handled**: grab
  the next free `D-03xx` number when you open your PR, not in advance.
  If two lanes finish and both claim, say, `D-0301`, the second one to
  merge rebases onto main and renumbers — the same mechanical fix already
  used twice in this repo's history (see D-0091→D-0092 and the earlier
  D-0090 collision in `docs/DECISION_LOG.md`'s own allocation-policy
  section). This is friction on merge order, never on development order —
  lanes still develop fully in parallel.
- **A lane's PR batches every work item assigned to that lane.** Do not
  open a separate PR per `MN-xxx` inside one lane; that defeats the
  purpose of lane-grouping. Do split into a follow-up PR if a lane turns
  out larger than expected mid-implementation — better a clean second PR
  than one unreviewable giant diff.
- **Generated files (`docs/_generated/`, `Cargo.lock`) will conflict
  trivially across lanes merging close together.** This is expected and
  mechanical: rebase onto the latest main, regenerate
  (`python3 tools/mininet_nav.py build`), do not hand-merge those files.

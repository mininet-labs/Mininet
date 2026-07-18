# The Mininet Canon: documentation architecture vision

**Status: vision/proposal document, not a decision to reorganize.** This
records founder direction on how Mininet's documentation should scale
across decades, in the founder's own structure and language. It is
**not** a canonical decision log entry, does not rename or move any
existing document, and does not commit the project to any specific
timeline. Treat it the way `docs/design/self-hosted-forge-spine.md` was
treated before D-0066 activated it piece by piece: a map of where things
could go, adopted incrementally, one real deliverable at a time — never
implemented wholesale in one PR.

## Why this exists

Most projects publish a whitepaper, a README, and maybe API docs.
Mininet is attempting something closer to "how should civilization-scale
digital infrastructure work" than "let's build a blockchain" — and the
founder's position is that its documentation should match that
ambition: not just what the system does, but why it was designed that
way, which alternatives were rejected, which assumptions have been
tested, and how a contributor joining decades from now can inherit that
reasoning without reverse-engineering it from chat logs.

The founder's own framing, kept verbatim because the reasoning matters
more than any paraphrase:

> Mininet should have the equivalent [of Linux's kernel + ABI + POSIX +
> filesystem + scheduler + documentation + governance + maintainership +
> coding standards + release process].

> A huge well-indexed corpus is manageable. A small contradictory corpus
> is expensive.

> I want people to say "Mininet is a constitutional digital civilization
> framework," not "Mininet is a cryptocurrency." The token is one
> chapter. The protocol is one chapter. The software is one chapter. The
> vision is much larger.

## The retrieval architecture principle (already partly true today)

The founder's own answer to "wouldn't a huge corpus be wasteful to load
into context" is the right one, and it already describes how this
repository's `tools/mininet_nav.py` index plus CLAUDE.md's "load what the
task requires" reading-order guidance actually works:

- keep the canonical corpus on disk;
- keep only compact indexes, hashes, summaries, and dependency maps
  readily loaded (`docs/_generated/REPO_INDEX.json`/`.jsonl`/
  `REPO_MAP.md`, regenerated every commit per CLAUDE.md's workflow
  ritual);
- retrieve detailed sections only when needed (CLAUDE.md's own
  "Canonical sources — load what the task requires" section already does
  this for `docs/FOUNDER_DIRECTIVES.md`/`INVARIANTS.md`/
  `DECISION_LOG.md`/`FAILURE_BOOK.md`/`THREAT_MODEL.md`);
- avoid repeating the same rationale across many documents (the existing
  convention of design docs linking back to their research report
  instead of reproducing it is this principle already in practice);
- archive superseded material while keeping it searchable (the
  append-only, never-delete rule for `DECISION_LOG.md`/`FAILURE_BOOK.md`/
  `THREAT_MODEL.md` is this principle already in practice).

The danger named is not document volume — it's duplication and poor
indexing. This vision document's job is to name the remaining gaps in
that structure, not to declare the structure doesn't exist yet.

## The proposed Canon (founder's structure, mapped onto what exists)

The founder's proposed seven "Books," with an honest note on what this
repository already has for each — most books are partially built under
different names, not starting from zero:

| Book | Founder's description | Closest existing thing today |
|---|---|---|
| I — Constitution | Immutable philosophy: why humans, equality, privacy, governance, AI | `docs/FOUNDER_DIRECTIVES.md`, `docs/INVARIANTS.md`, `docs/CONSTITUTION_REGISTRY.json` (D-0090) |
| II — Economics | Money, Human Share, treasury, inflation, storage/compute markets, AI economy | `docs/economics/` (D-0073–D-0075), `mini-resource-pricing` (D-0302), tokenomics sim harness |
| III — Engineering | Every protocol, packet, API, message, consensus rule | The `docs/design/*.md` corpus + crate-level docs; no single unified spec yet |
| IV — Law | Constitutional legal position | **New**: `docs/LEGAL_DISCLAIMER.md` (this batch) |
| V — Operations | Releases, security, audits, incident response, treasury ops, maintainership | `docs/design/self-hosted-forge-spine.md`, `docs/audits/`; no dedicated operations manual yet |
| VI — Research | Every unsolved problem, rejected solution, simulation, paper, experiment | `docs/research/*.md`, `docs/FAILURE_BOOK.md` |
| VII — History | Why each major decision was made, with full rationale, preserved forever | `docs/DECISION_LOG.md` (D-0001 through the current number) — this is already exactly the founder's worked example ("someone in 2043 asks why Human Share, opens Book VII → Decision D-0074") |

The founder's other named components, and their closest existing analog:

- **Founder Notebook** (question/context/alternatives/tradeoffs/
  reasoning/unknowns/review-date/status, per major founder decision) —
  closest today is `docs/DECISION_LOG.md`'s 7-field template
  (Decision/Reason/Constitutional impact/Implementation status/Failure
  point/Required follow-up/Supersedes), which already captures most of
  this shape at the decision level rather than the raw-reasoning level.
- **Living Encyclopedia** (Human Share, Governance, AI Wallet, Identity
  Root, Treasury, Bridge, Storage, Privacy, Stealth Address, Receipt
  Proof, etc., Wikipedia-style but canonical) — no equivalent exists yet;
  the closest is scattered crate-doc-comments and `docs/NAVIGATION.md`.
- **Simulation Library** — the tokenomics simulation harness exists;
  no unified library of every simulation/graph/parameter/result yet.
- **Decision Library** — this **is** `docs/DECISION_LOG.md`, already
  operating at real scale (300+ entries across the D-00xx/D-02xx/D-03xx
  bands as of this document).
- **Threat Library** — this **is** `docs/THREAT_MODEL.md`, already
  structured as threat/likelihood/impact/mitigation/residual-risk.
- **Assumption Library** (every load-bearing assumption, with evidence,
  counterarguments, review schedule — e.g. "humans remain the source of
  legitimacy") — no equivalent exists yet. This is the founder's own
  candidate for "the most valuable" addition, and the two hard-limitation
  callouts already pinned at the top of `docs/INVARIANTS.md` (identity-
  root ≠ verified human; proof-of-space-time ≠ replication uniqueness)
  are the closest existing precedent for what an assumption-register
  entry should look like.

## The founder's proposed five parallel streams

Recorded verbatim as the founder's proposed organizing frame for
future work, distinct from (and higher-level than) the per-crate roadmap
tracks already in flight (self-hosted-forge-spine Batches, the MN-2xx
networking track, the native-intake/commons/search tracks A-F):

- **Stream A — Engineering.** Complete the implementation until every
  issue becomes code.
- **Stream B — Constitution.** Turn every founder decision into a
  durable constitutional document with rationale and invariants.
- **Stream C — Economics.** Finish simulations, calibration, treasury
  design, AI economy, long-horizon monetary policy.
- **Stream D — Law.** Build the full counsel-ready legal corpus, with
  assumption registers and jurisdictional analysis.
- **Stream E — Civilization.** Document institutional memory: history,
  research, decision records, threat models, operational manuals, future
  governance.

And the founder's proposed five "pillar" document sets, with the
founder's own completion estimates recorded as stated (not verified by
this repository — they are the founder's characterization, not an
audit finding):

| Pillar | Founder's stated status |
|---|---|
| 1. Technical Architecture | ~80–90% (`docs/design/*.md` + crate docs) |
| 2. Constitutional Specification | ~90% (`FOUNDER_DIRECTIVES.md`, `DECISION_LOG.md`, `INVARIANTS.md`, governance pack) |
| 3. Economic Specification | ~75% (`docs/economics/`, tokenomics sim) |
| 4. Legal Architecture | 100% → target complete (`docs/LEGAL_DISCLAIMER.md`, this batch) |
| 5. Operations Manual | "almost completely missing" — repository owner/maintainer duties, emergency playbooks, key ceremonies, release/CI-CD process, treasury operations, governance elections, foundation transitions, disaster recovery, incident communication, external review process |

## What this document does NOT do

It does not rename `docs/DECISION_LOG.md` to "Book VII," does not create
a `docs/canon/` directory, does not move any existing file, and does not
open a Stream A-E tracking issue. Per the same discipline used
throughout this session for founder-supplied research (D-0096 through
D-0312): when a document's own conclusion is "this is ambitious, and
should be done incrementally," the correct first artifact is recording
the vision honestly — not silently executing a repository-wide
reorganization inferred from a conversational proposal. The one concrete
gap this document identifies as clearly missing and not yet mapped to
anything existing is the **Operations Manual** (Book V / Pillar 5) and
the **Assumption Library** — both real, scoped, and buildable as their
own future design docs if the founder confirms this direction.

## Required follow-up

- Founder confirmation on whether to open tracking issues for Stream
  B–E work, or treat this purely as background orientation.
- If confirmed: a first scoped deliverable would be either (a) an
  Operations Manual outline (`docs/OPERATIONS_MANUAL.md`, Book V), since
  it is named as the most complete gap, or (b) an Assumption Library
  first entry, formalizing the two existing `INVARIANTS.md`
  hard-limitation callouts into the proposed evidence/counterargument/
  review-schedule format as a worked example before generalizing.

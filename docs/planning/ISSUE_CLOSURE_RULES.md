# Issue closure rules

Defines when an issue may actually be closed, so closure evidence always
matches the nature of the issue — this is the general form of the
discipline #99 already applies to the External Legitimacy Gates
specifically (see #99's own "How to use this issue" section).

## General rule

An issue may be closed only if its closure evidence matches its closure
category (`docs/planning/PRE_CODING_ISSUE_MATRIX.md`).

Code tests do not close research questions. Research memos do not close
hardware validation. Hardware logs do not close legal questions. Founder
decisions do not close security proofs. A simulation harness existing
does not close a question that needs an external specialist's judgment
on the results.

## Closure labels

Use these in issue comments and PR bodies:

- `closed-by-code`
- `closed-by-spec`
- `closed-by-simulation`
- `closed-by-hardware-validation`
- `closed-by-external-review`
- `closed-by-founder-decision`
- `mvp-deferred`
- `needs-revisit-after-mainnet`

## Required closure evidence, by category

### Code-closeable
Implementation; unit tests; integration tests where relevant; a threat
model delta if security-relevant; a changelog/STATUS.md entry if
user-visible.

### Spec-closeable
Problem statement; founder constraints; non-goals; threat model; state
machine or data model where relevant; acceptance criteria; explicit
deferred work. (D-0073/D-0074/D-0075 and their `docs/design/*.md`
companions are the reference examples of what this looks like done.)

### Simulation-closeable
Adversary assumptions; input parameter table; sweep range; failure
thresholds; a reproducible script; a summary of failures as well as
passes; a Failure Book entry for any parameter set rejected outright.
(`tools/sim/tokenomics_sim.py` plus `docs/gates/
economic-simulation-spec.md` is the reference example — note its own
documented limitation is *part of* the required evidence, not something
to omit because it's inconvenient.)

### Hardware-gated
Exact devices used; OS versions; test environment; test procedure; raw
logs; negative tests; relay/fraud drills where presence-related;
conclusion stated as strong/weak/unusable, not just "it worked once."

### External-gated
Scope package; reviewer identity or organization class; questions asked;
the report or written result; founder response to the report; follow-up
issues for unresolved findings.

### Founder-decision-gated
Options; tradeoffs; recommended default; risks; irreversible
consequences; exact decision text; a link to the Decision Log entry.

## False-closure examples — do not do these

- Do not close #21 because a prototype liveness signal exists. Human
  uniqueness is not solved by one signal — D-0075 explicitly rejects
  that framing.
- Do not close #47/#50 because the simulation harness runs cleanly and
  produces plausible-looking numbers. Passing an internal sweep is
  necessary, not sufficient — external mechanism-design review is still
  required before the calibration is trustworthy.
- Do not close #97/#98 because the trait seam (`RangingSource`,
  `Bearer`) compiles and passes unit tests. These require physical relay
  and false-positive testing on real hardware — an API existing is not
  evidence about the physical world.
- Do not close #28 because a DTN design section now exists
  (`docs/gates/dtn-design-constraints.md`). It requires a domain expert
  confirming the regime scope and an actual operating-mode
  implementation with partition/finality constraints wired in.
- Do not close #102 until the full path is demonstrated **end to end and
  currently**: developer change → review → governed merge → reproducible
  build → release finality → safe install → health check → rollback.
  Batches 1-4 demonstrate this today (Batch 4's exit condition is met per
  `self-hosted-forge-spine.md`) — but #102 tracks all six batches, and
  Batch 5/6 remain open, so #102 itself stays open until those are too.
- Do not close #72/#93/#96 by merging real code. They were deliberately
  closed *as not-planned* (2026-07-10 founder decision, #99) rather than
  worked toward — a different closure path than any of the above,
  recorded as `closed-by-founder-decision` for the deferral itself, with
  the actual external review still pending at "day 0."

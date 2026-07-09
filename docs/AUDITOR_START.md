# Start here — for an auditor or skeptic

You are the reader this project most wants. Mininet's core claim is not "trust
us" — it's "here is exactly what we guarantee, exactly how it's enforced, and
exactly where it isn't done yet." This page points you at the evidence and,
just as importantly, at the honest gaps.

## The claims, and where each is enforced

Start with [`INVARIANTS.md`](INVARIANTS.md). Every frozen invariant carries a
stable ID and a full traceability chain: **Directive → Invariant → Source
(Spec/D-number) → enforcing crate + test.** You can walk it forward from a
principle ("what protects the voice/value wall?") or backward from a failing
test ("which founding principle does this protect?").

The two **hard, temporary limitations** are stated at the very top of that
file on purpose — read them first, because they bound what any other claim can
mean:

- Every "verified identity" counted today is a verified `did:mini` **root**,
  not a verified **human**. Nothing may be read as enforcing one-human-one-vote
  until personhood is actually solved.
- Proof-of-space-time proves continuous **possession**, not replication
  **uniqueness** — one warehouse can still answer for many claimed identities.

## What could kill it — the threat model

[`THREAT_MODEL.md`](THREAT_MODEL.md) is a civilization-scale threat catalog
(human, technical, economic, political, civilization), each threat
cross-referenced to the invariant that defends it — and honestly marked
**"explicitly unresolved"** where nothing does yet (Sybil resistance,
storage-consolidation resistance, coordinated governance capture,
founder-authority limits). If you find a threat it misses, that's a finding
worth filing.

## The gates — what more code cannot close

[`gates/`](gates/) is the register of work that engineering *cannot* finish
alone: external cryptography audit ([crypto-audit-scope.md](gates/crypto-audit-scope.md)),
FROST DKG review ([dkg-audit-scope.md](gates/dkg-audit-scope.md)), legal
counsel ([legal-review-brief.md](gates/legal-review-brief.md)), personhood
research ([personhood-signal-b-decision.md](gates/personhood-signal-b-decision.md)),
hardware validation ([hardware-test-protocol.md](gates/hardware-test-protocol.md)),
and economics modeling ([economic-simulation-spec.md](gates/economic-simulation-spec.md)).
Each package is a ready-made scope brief for the outside reviewer who *can*
close it. Tracking issue: [#99](../../issues/99).

**The hard rule you should hold the project to:** no real-value mainnet,
bridge, treasury, contribution, or bounty payout before the cryptography audit
(#72) and legal review (#96) close (D-0037/D-0047). If you ever see a claim
that value is live before those gates close, that is a violation to call out.

## Internal audit deliverables already written

[`audits/`](audits/) holds point-in-time review documents for specific
questions — constitutional compliance, frozen-invariants adversarial review,
CID integrity, memory safety, the did-mini security audit (with three fixed
findings), identity recovery, presence attacks, and the Sybil/social-graph
review (with a real farm-saturation bypass found and fixed). These are
internal AI-drafted reviews under D-0037 — they raise the floor, they do **not**
substitute for the external audit gated above.

## Verify it yourself

- **Reproducibility:** CI runs a same-machine reproducible-build check
  (D-0044); the full cross-machine, K-independent-builder standard SPEC-11 §8
  wants is still open.
- **Memory safety:** all crates `#![forbid(unsafe_code)]`; the dependency
  tree's unsafe usage is enumerated and explained in
  [`audits/issue-71-memory-safety-audit.md`](audits/issue-71-memory-safety-audit.md).
- **Dependency posture:** `rustsec/audit-check` runs in CI (D-0044).
- **Run the suite:** `cargo test --workspace --all-features` (see
  [`DEVELOPER_START.md`](DEVELOPER_START.md)); the money-safety and identity
  claims each have named adversarial tests you can read and run.

## The honesty policy itself

Overclaiming is treated as a **bug** in this project (Directive-level rule).
Every crate and document is expected to state plainly what is NOT built, NOT
audited, NOT anonymous, NOT enforced. If you find a place where the code claims
more than it delivers, you've found the kind of defect the project considers
most serious — please file it.

## Where to go next

- The living build-status ledger: [`STATUS.md`](STATUS.md).
- Why any specific choice was made: [`DECISION_LOG.md`](DECISION_LOG.md)
  (search the D-number).
- What was already tried and rejected: [`FAILURE_BOOK.md`](FAILURE_BOOK.md).
- The values under all of it: [`FOUNDER_DIRECTIVES.md`](FOUNDER_DIRECTIVES.md).

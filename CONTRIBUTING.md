# Contributing

Mininet is public domain and has no owner — so "contributing" means proposing
changes that the network (eventually, itself) chooses to run. Until the on-chain
forge is live (SPEC-11), we use GitHub as a temporary host. The intent is to
migrate this very repository *into* Mininet, at which point pull requests are made
from inside the network and merge authority is governed, not granted by a platform.

**Before anything else, read `docs/FOUNDER_DIRECTIVES.md`.** It is not a
rulebook alongside this one — it's the reasoning underneath every rule
here and in `docs/INVARIANTS.md`. When a PR touches something no existing
document anticipated, the directives are what a reviewer (human or AI)
reasons from. This applies equally to AI-assisted contributions: an AI
drafting code under the D-0037 policy is expected to have read and be
reasoning from the same seventeen directives a human contributor would.

## Principles that apply to contribution itself

- **Voice / value wall (SPEC-11 §2 \[FREEZE\]).** Funding work is free and unequal;
  *deciding* what merges is equal and one-human-one-vote. No bounty, sponsorship,
  or holding may ever confer merge authority, counted reviewer standing, or
  governance weight. Money can fund every bounty in existence and buy zero control.
- **No founder privilege over the constitution.** The six frozen principles bind
  everyone, including the founding cohort. A change that money-buys-power, installs
  an owner, adds an off switch, or breaks privacy/sovereignty is wrong by
  definition and is not a valid change — see `docs/INVARIANTS.md`.
- **Default-deny on frozen domains.** If a change touches a Tier-F invariant and
  it's ambiguous whether it's permitted, the answer is no.
- **Two approvals, for now (D-0033).** Protocol-critical repos — this one
  included — require at least two distinct maintainer approvals before merge,
  at least two independent release attestations before release, and at least
  two approvals on crypto-sensitive AI-assisted code. There is no 1-of-1
  canonical merge path. AI may draft sensitive code; a human review is always
  required regardless. See `mini-forge::governance::PROTOCOL_MIN_APPROVALS`.

## Practical checklist for a PR

1. `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` are clean.
2. `cargo test` passes (tests are deterministic; they must stay that way).
3. `Cargo.lock` is committed if dependencies changed (reproducibility, D-0006).
4. If you touched a frozen domain, update the **Enforced by** cell in
   `docs/INVARIANTS.md` and add a `D-00xx` entry to `docs/DECISION_LOG.md`.
5. Keep the dependency surface small and auditable, especially in `mini-crypto`.

## Signing

In the target system every action is signed by a `did:mini` device key and
verifiable by any peer (SPEC-11 §2). Until that lands here, please sign your Git
commits (`git commit -S`) so there's a continuous authorship chain we can later
bind to did:mini identities.

## Scope of a batch

We land the system in small, self-contained, mergeable batches along the Phase
0/1 critical path (`docs/DECISION_LOG.md` D-0001). Prefer one coherent crate or
capability per PR over large mixed changes.

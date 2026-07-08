# Security Policy

Mininet has no owner, no company, and no admin key — but real cryptography
lives in this repository, and some of it is explicitly not
production-ready yet. This file is where to report a problem responsibly
and where to check before assuming something is safer than it is.

## Before anything else: read the honest limits

Several crates in this workspace are AI-authored cryptography prototypes,
founder-reviewed but **not externally audited** (`docs/DECISION_LOG.md`
`D-0036`, `D-0037`, `D-0040`, `D-0041`; see the root `README.md`'s status
table for the current list — stealth addresses, ring signatures, and
Bulletproofs confidential amounts in `mini-value`; FROST threshold custody
in `mini-treasury`; Merkle/PDP storage proofs in `mini-spacetime`). If
you're evaluating whether something here is safe to depend on, start with
that table and each crate's own "Honest limits" section, not just its
test suite passing.

## Reporting a vulnerability

If you find a security issue — a cryptographic flaw, a way to defeat a
constitutional invariant (`docs/INVARIANTS.md`, e.g. a path from money to
governance weight, or a way to unmask a user), a memory-safety issue
despite `#![forbid(unsafe_code)]`, or anything else that could hurt a real
user if this code were deployed as-is:

- **For anything already covered by an existing "pending audit" honest
  limit** (see above): open a regular GitHub issue. These are known,
  labeled gaps, not surprises — public discussion is fine and expected.
- **For a genuinely new finding** — something not already disclosed as a
  known limit, especially one that would be exploitable even accounting
  for the "not yet audited" caveat: open a private
  [GitHub security advisory](../../security/advisories/new) on this
  repository instead of a public issue, so it can be assessed before
  wide disclosure. There is no dedicated security email; this repository
  has no company or foundation behind it to route one through
  (Directive 2, `docs/FOUNDER_DIRECTIVES.md`).

Please include: which crate/module, a minimal reproduction or a clear
description of the attack, and which constitutional invariant or safety
property it breaks (if applicable).

## What to expect

There is no SLA and no dedicated security team — this is a small,
ownerless, in-development project (see `docs/BETA_STATUS.md` for what
stage it's actually at). A maintainer will triage and respond as capacity
allows. Given the number of unaudited cryptography prototypes in this
tree, the most valuable reports right now are ones that help prioritize
which prototype needs external audit most urgently, not just individual
bugs in already-known-incomplete code.

## Scope

In scope: anything in `crates/`, the build/CI configuration, and the
protocol design itself where it deviates from what the Whitepaper or
`docs/DECISION_LOG.md` claims. Out of scope: this repository's temporary
GitHub hosting (SPEC-11 notes GitHub is a mirror, not the trust root) and
any third-party dependency's own upstream vulnerabilities — report those
upstream, though flagging them here so `Cargo.lock` gets updated is
welcome too.

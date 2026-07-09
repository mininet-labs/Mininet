# Fork legitimacy criteria — what makes a fork *the* Mininet

Closes [roadmap #57](../../issues/57) (Phase
7.6). D-0046 froze the *principle* (invariant **F1**) and explicitly handed
this issue the job of turning it into "a fully checkable definition." This
document is that definition.

Grounded in Directive 7: *"the canonical network is defined by continuous
adherence to the Constitution, the verified-human community, and the chain
of legitimate governance — not by a repository or a trademark."*

## The distinction this document makes checkable

There are three things a "fork" can be, and only one of them is Mininet:

1. **A legitimate derivative.** Someone copies the code and builds a
   different network with different rules. This is explicitly allowed and
   encouraged (P3/P7: no owner, nobody forced to participate). It is *not*
   Mininet, and does not claim to be. No conflict.
2. **A legitimate continuation.** A fork of the *software* that preserves
   continuity of the network — same constitutional invariants, same
   personhood-root history, same release-registry and chain lineage. If the
   original repository disappeared tomorrow, this is what "Mininet" would
   continue as. This *is* Mininet.
3. **An illegitimate impersonation.** A code copy that breaks continuity but
   claims to *be* the canonical Mininet — the case F1 exists to rule out.

Forking the software is always free (case 1 is a feature, not a threat).
Inheriting *legitimacy* is what these criteria gate. The mistake F1
prevents is treating a repository or a trademark as the source of
legitimacy; the source is **continuity**, defined below.

## The four continuity criteria (all four required)

A fork is the canonical Mininet **if and only if** it satisfies all four.
Failing any one makes it a legitimate derivative (case 1) if it is honest
about being different, or an impersonation (case 3) if it claims otherwise.

### C1 — Constitutional-invariant continuity

**Check:** every Tier-F frozen invariant in `docs/INVARIANTS.md` holds in
the fork, with the same meaning. Not "has a document named INVARIANTS.md" —
the actual invariants (P1-P6, M1-M3, F1, V/ID/U/PR/S/N/AI/X series) are
still enforced by code, and the voice/value wall (P1) in particular is
intact.

**Fails if:** the fork relaxes, removes, or reinterprets any frozen
invariant — e.g. lets balance buy governance weight, lets money CRDT-merge
(M1), lets an offline payment be treated as final (M2), or removes the
personhood-per-root vote counting. A fork may *add* rules; it may not
*subtract* a frozen invariant and remain canonical.

**Checkable because:** invariants are "encoded as checks, not conventions"
(the Enforced-by column). Run the fork's own test suite and confirm the
invariant tests are present and passing, then confirm none were deleted or
weakened against this tree's set.

### C2 — Personhood-root history continuity

**Check:** the fork recognizes the same verified-human/identity-root
history. The community of verified humans (their `did:mini` inception and
KEL history) carries forward; a person who was verified on the canonical
network is still the same root on the legitimate continuation.

**Fails if:** the fork resets, re-issues, or re-keys the personhood set —
i.e. starts a fresh population and asks everyone to re-verify from zero.
That may be a perfectly good new network (case 1), but it is a *new*
community, not the continuation of the existing one, so it is not Mininet.

**Checkable because:** identity roots are self-certifying (`did:mini`
SCIDs re-verifiable with no central lookup — see `docs/ADDRESSING.md`
Layer 1). Sample known-existing roots and confirm their inception/KEL
verifies identically on the fork.

### C3 — Release-registry continuity

**Check:** the fork's release/governance lineage descends from the
canonical release registry through legitimate governance steps (the
`mini-forge` approval/merge process, honoring the protocol-repo two-approval
floor, D-0033). Each release is reachable from the prior one by a chain of
approvals that themselves satisfy the governance rules.

**Fails if:** the fork's releases branch off through a step that did not go
through legitimate governance — e.g. a release nobody with authority
approved, or one that bypassed the approval floor. `mini-forge`'s
`adoption_refuses_on_governance_forks` test is the shape of this check: a
governance fork is detected and refused, not silently adopted.

**Checkable because:** the forge governance chain is append-only and its
approvals are bound to exact commits (`approvals_are_bound_to_the_exact_commit`).
Walk the release lineage and confirm each step's approvals verify against
the vouched maintainer set of its time.

### C4 — Canonical chain-state continuity

**Check:** the fork continues the same canonical chain — the same ledger
history, the same finalized blocks, the same resolution of any
double-spends via canonical ordering (M3). Money and history carry forward
intact.

**Fails if:** the fork rewrites finalized history, forks the ledger at a
point and diverges (a chain split — both sides may be internally valid, but
at most one can be the continuation, decided by the finality/fork-choice
rules, not by which repo is more popular), or reassigns balances.

**Checkable because:** finality is verifiable (`mini-chain::verify_finality`
requires >2/3 distinct delegated validator roots — invariant V1). Confirm
the fork's chain tip is reachable from the canonical finalized history
without any rewrite of a finalized block.

## The edge case D-0046 flagged: letter vs. spirit

D-0046's own failure point warned that a fork could satisfy all four listed
criteria "but violate the *spirit* of continuity through some mechanism
this entry didn't anticipate." This document does not pretend the four
criteria are provably exhaustive. Two guards against gaming them:

- **The criteria are conjunctive and continuity-based, not snapshot-based.**
  C1-C4 each require an unbroken *chain* (of invariants, roots, releases,
  chain state), not a one-time match. A fork can't clone today's state and
  claim continuity; it has to have *descended* from it. This is much harder
  to fake than a snapshot.
- **Directive 7 remains the tie-breaker for anything the criteria don't
  anticipate.** If a fork satisfies C1-C4 by the letter but has plainly
  severed the verified-human community or the legitimate governance chain in
  substance, `docs/FOUNDER_DIRECTIVES.md` is the document a reviewer reasons
  from — exactly the role it is built to play. The criteria make the common
  case checkable; the Directive covers the adversarial residue.

## What "legitimacy" does and does not buy

Legitimacy is *not* enforced by a kill switch, a trademark, or an admin key
— there is none, and adding one would violate P3. A legitimate continuation
cannot forcibly shut down an illegitimate impersonation. What the criteria
provide is a **shared, checkable standard** any verified human, client, or
service can apply independently to decide which network they treat as
Mininet — the same way self-certifying addresses let anyone verify an
identity with no central authority. Legitimacy is a property others can
*verify and choose to honor*, never one that is *imposed*.

## Status

- **Design/criteria: complete** (this document + F1 + D-0046).
- **Machine-checkable end-to-end: partial.** C1 (invariant tests) and C2
  (`did:mini` verification) are checkable against the current tree today. C3
  and C4 depend on a networked release registry and chain that exist as
  logic (`mini-forge`, `mini-chain`) but not yet as a live, populated
  network — so the *criteria* are concrete now, while *running* them at
  scale waits on the same networking work invariant V1 and roadmap
  #36-#45 track. This matches D-0046's "design-only / criteria-only"
  implementation status, now discharged into an explicit definition.

Recorded as **D-0052**.

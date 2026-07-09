# Bounty & review system design — money funds work, never a decision

Closes [roadmap #66](../../issues/66) (Phase
9.2). The issue's one hard requirement: *"funding a bounty must remain
completely separate from any merge/review authority — money funds work,
never buys a decision."* This document shows how the system is structured
so that separation is a property of the code, not a promise in a README.

It sits under Directive 16 ("Preserve the Voice/Value Wall at All Costs")
and invariant **P1** (no balance maps to governance or validator vote
weight — `docs/INVARIANTS.md` §1). The review side is `mini-forge`; the
funding side is `mini-bounty` (D-0049); this note is the design that ties
them together and proves the wall between them holds.

## Two authorities that must never touch

A contribution moving from "proposed" to "merged and paid" crosses two
completely different kinds of authority. The whole design is about keeping
them in separate crates with no path from one to the other.

| | **Review / merge authority (voice)** | **Funding authority (value)** |
|---|---|---|
| Crate | `mini-forge` | `mini-bounty` (payout math from `mini-value`) |
| Unit of power | one verified identity root, counted at most once | MINI, held by whoever funded a pool |
| What it decides | whether a commit is approved and merged | how much a claimant is paid, and to which address |
| What it must **never** do | be weighted, bought, or bypassed by any balance | confer, imply, or unlock any approval/merge capability |
| Enforced by | `mini-forge::approve`/`merge` counting per identity root | `mini-bounty` carrying no `Capabilities` bit at all |

The point of the table is the bottom two rows. Voice is counted per human
root and is deaf to money. Value is denominated in coin and is mute in
governance. Neither can reach across.

## The structural wall (the part that's actually enforced today)

The separation is not a runtime check that could be forgotten — it is the
**dependency graph**:

- `mini-bounty`'s only dependency is `mini-value` (see its `Cargo.toml`).
  It does not depend on `mini-forge`, `did-mini` capabilities, or any
  governance type. A `BountyGrant` is *only* a one-time public key and an
  amount; there is no field on it, and no method in the crate, that can
  produce a `Capabilities::VOTE` bit, an `approve` call, or a `merge` call.
  **Funding a bounty cannot, by construction, express a merge decision,
  because the crate that handles money has no reference to the crate that
  handles merges.**
- `mini-forge` governance is symmetrically money-blind: its own module
  docs state "no balance, stake, or payment" enters approval counting, and
  approvals are tallied per vouched identity root. It has no dependency on
  `mini-value`, `mini-bounty`, or `mini-treasury`. **A merge decision
  cannot read, require, or be swayed by any balance, because the crate
  that handles merges has no reference to the crate that handles money.**

This is the voice/value wall reduced to a fact any reviewer can verify in
ten seconds: `grep` the two `Cargo.toml`s and confirm there is no edge
between them. A future PR that tried to make merges depend on funding (or
funding depend on approval capability) would have to *add* that dependency
edge — a large, obvious, reviewable change that trips the frozen-domain
checklist, not a subtle one.

## The review side, as it exists in `mini-forge`

The review-tracking half of #66 is already built and tested. Approval and
merge authority is counted per identity root, and the relevant properties
have named tests (`crates/mini-forge/tests/governance.rs`):

- `author_never_counts_and_one_identity_root_counts_once` — the person who
  wrote a change cannot approve their own change, and controlling many
  devices under one root still yields exactly one approval. (This is the
  per-human, money-irrelevant counting that P1/P2 require.)
- `approvals_from_unvouched_authors_do_not_count` — an approval only counts
  from an identity root the maintainer set actually vouches for; you cannot
  manufacture approval weight by spinning up fresh roots (nor by paying).
- `protocol_floor_projects_actually_require_two_distinct_approvers_to_merge`
  and `protocol_repo_floor_is_two_approvals_for_now` — the protocol repo's
  two-approval floor (D-0033) is machine-enforced, not a convention.
- `approvals_are_bound_to_the_exact_commit` — an approval is bound to the
  exact commit it reviewed, so a merge can't inherit approvals granted to
  different code.
- `competing_valid_merges_resolve_deterministically_and_are_flagged` — the
  CRDT resolves competing merges deterministically rather than by whoever
  pushed hardest (or paid).

None of these read a balance. That is the review system honoring the wall
from its own side.

## The funding side, as it exists in `mini-bounty`

The funding half is D-0049. A contribution that a human maintainer
approves on GitHub results in the maintainer publishing the contributor's
one-time claim public key into a `BountyPool`. The contributor later claims
anonymously via a ring signature over the whole pool, directing payout to a
fresh stealth address; the key image prevents double-claims. The crate
introduces no new cryptography — it reuses `mini-value`'s ring signatures
and stealth addresses (D-0036). See `crates/mini-bounty/README.md` for the
construction and its honest limits.

## End-to-end flow, and where the wall sits in it

```
   PR opened on GitHub  (identity: never anonymous — GitHub/Microsoft knows)
            │
            ▼
   Maintainers review & approve   ← VOICE. Counted per identity root in
            │                        mini-forge. No balance is read here.
            ▼
   Merge under the repo's policy   ← VOICE. 2-approval floor for protocol
            │                        repo, machine-enforced.
            │
   ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄  THE WALL  ┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄
            │
            ▼
   Approver publishes a BountyGrant (the contributor's one-time key)
            │                      ← VALUE begins. This step *records* that
            │                        an already-made merge decision happened;
            ▼                        it can never *cause* one.
   Contributor claims anonymously  ← VALUE. Ring signature + stealth payout.
                                     Mininet's ledger never learns which
                                     approved contributor was paid.
```

The merge decision is complete *before* any value moves. Publishing a grant
is downstream of the decision and cannot feed back into it — there is no
code path from `mini-bounty` back to `mini-forge::approve`/`merge`.

## Adversarial questions

- **Can someone buy a merge by funding a large bounty?** No. Funding a pool
  produces `BountyGrant`s and moves MINI; it produces no approval capability
  and cannot call `approve`/`merge`. The maintainers who approve are counted
  per identity root regardless of who funded the work, or whether anyone
  funded it at all.
- **Can a rich actor drown out reviewers by funding many competing pools?**
  Pools are just value. Merge outcomes are decided by
  `competing_valid_merges_resolve_deterministically_and_are_flagged`, which
  reads no balance. More money buys more *bounties to be worked*, never more
  *say in what merges*.
- **Can the payout reveal who a reviewer favored?** The anonymity boundary
  is stated honestly in `mini-bounty`: GitHub itself is never anonymous (the
  maintainer knows whose PR they approved), but Mininet's public ledger
  never links the payout to the approved contributor. The wall is about
  *authority*, not about hiding that a review happened.
- **Can withholding funding coerce a decision?** A maintainer's approval is
  not conditioned on funding existing — nothing in `mini-forge` checks for a
  pool. Refusing to fund work is an economic choice that leaves the review
  process untouched; it cannot block or force a merge. (Coercion of
  *people* is a real, separate threat — logged in
  `docs/THREAT_MODEL.md` §1, not solved here.)

## Status and what remains

- **Built & tested:** the review/merge authority (`mini-forge`, per identity
  root, 2-approval floor) and the anonymous funding/claim cryptography
  (`mini-bounty`, D-0049). The structural wall (no dependency edge) holds
  today.
- **Not built:** the GitHub-reading integration that would mint a
  `BountyGrant` automatically from an approved PR — this is deliberately
  out of scope for the claim cryptography and is noted as unfiled follow-up
  in D-0049. When it is built, it must sit *downstream* of the merge
  decision (it reacts to an approval that already happened) and must never
  become an input to `mini-forge`, or it would breach the wall this document
  exists to protect.
- **Gated:** real payouts are D-0047-gated (external audit) like every other
  `mini-value`-derived prototype.

This design is recorded as **D-0051**.

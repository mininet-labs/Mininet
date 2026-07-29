# Day-0 monetary chain integration

**Status:** proposed, tested by source/CI evidence; not activated
**Decision:** proposed D-0414
**Depends on:** D-0074, D-0413, roadmap #18, #48, #50, A1

## Purpose

This slice connects the merged D-0413 policy kernel to Mininet's existing
finalized execution state without changing existing `PaymentClaim` wire
semantics. It establishes the minimum deterministic supply boundary:

- one monetary epoch at most per finalized block;
- strictly sequential epoch numbers;
- exact binding to the finalized opening circulating supply;
- recomputation of every D-0074 cap, allocation, and vesting rule;
- cumulative policy-epoch time instead of proposer or device wall time;
- aggregate Human Share snapshot commitments rather than public identity
  lists; and
- one state commitment covering settlement high-water marks and monetary
  supply/vesting state.

It is not yet an account-balance ledger and therefore does not make MINI
spendable or production-ready.

## Why policy time

`mini-chain::BlockHeader::timestamp_ms` is deliberately equal to block height.
It is logical ordering evidence, not a trustworthy wall clock. Using that
field to enforce “365 days” would either unlock grants after 365 blocks or
give proposers control over time.

The monetary ledger instead defines:

```text
policy_time_ms = sum(duration_ms of every finalized monetary epoch)
```

Each accepted epoch has a non-zero duration no greater than one policy year.
Vesting starts at that epoch's end. The next epoch must declare the exact
circulating supply computed at the previous boundary, including amounts that
became vested during finalized policy time. No local clock, network time
server, validator timestamp, or AI chooses the result.

This mechanism orders and measures adopted monetary periods. It does not prove
that the physical world actually experienced the declared duration. Production
epoch advancement therefore still needs a separately governed, censorship-
resistant finality rule.

## Privacy and scale

The chain transition uses `ScalableEpochPlan`, not D-0413's bounded
explicit-recipient ceremony plan. Human Share is represented by:

```text
snapshot root
eligible count
equal amount per human
aggregate issued amount
unissued integer remainder
vesting schedule
```

The block does not enumerate human identities. `MonetaryLedger` creates one
aggregate vesting position for the snapshot. A future claim protocol must
prove private snapshot membership, derive an epoch-specific unlinkable
nullifier, prevent duplicate claims, and credit the claimant without revealing
the snapshot's identity mapping. That protocol is a research/security
dependency and is not invented here.

Service and treasury beneficiaries remain explicit because those channels
require auditable evidence and receipts. Their authorization is still outside
this slice.

## Transition validation

For a candidate epoch, every node:

1. requires the epoch number to be zero at first use or exactly one greater
   than the last accepted epoch;
2. requires `opening_circulating` to equal the ledger's computed circulating
   supply at the current policy boundary;
3. reconstructs the entire scalable plan from the snapshot and optional
   grants under `IssuancePolicy::d0074()`;
4. requires byte-for-structure equality with the candidate plan;
5. advances cumulative policy time with checked arithmetic;
6. adds aggregate Human Share and optional vesting positions;
7. recomputes total, circulating, and locked supply; and
8. includes the exact monetary state in `LedgerState::commitment`.

Malformed plans, modified grants, reordered epochs, stale opening supply,
more than one epoch per block, overflow, or a false post-state root fail the
entire state transition. Existing invalid/stale payment claims retain their
previous behavior and do not become monetary issuance.

## Supply identities

At every accepted state:

```text
total_supply = genesis_circulating + total_issued
locked_supply = total_supply - circulating_supply
circulating_supply = genesis_circulating + sum(vested positions)
```

All arithmetic is checked `u128` micro-MINI. Partial vesting uses quotient and
remainder decomposition so a mathematically valid result does not overflow
from an intermediate `amount × elapsed` product.

## What remains before spendable MINI

1. Canonical serialization/decoding and bounded input lengths.
2. Private Human Share membership, nullifier, claim, and recovery protocol.
3. Account/commitment balances with debit, credit, insufficient-funds, and
   locked-funds checks.
4. Binding `PaymentClaim` finalization to those balances. Today
   `mini-execution` records winning claims but does not transfer balances.
5. Evidence roots and authorization for service and treasury grants.
6. A governed epoch-advancement/finality rule resistant to stalls and
   accelerated-time attacks.
7. Production genesis selection and reproducible ceremony.
8. State snapshots, proofs, pruning, migration, rollback, and recovery.
9. Independent cryptographic, economic, consensus, privacy, accessibility,
   and implementation review.

No UI should display this proposal as an available balance or claim that
vesting is spend-enforced until items 2–4 exist.

## Rollback

Before activation, rollback is removal of the proposed monetary fields and
dependency; existing settlement claim formats remain unchanged. After any
future activation, rollback cannot erase issued property. A versioned state
migration and governed chain transition would be required.

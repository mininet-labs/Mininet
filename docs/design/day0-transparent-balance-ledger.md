# Day-0 transparent balance ledger

**Status:** proposed Tier-0 implementation; stacked on PR #272
**Decision:** proposed D-0415
**Roadmap:** payment portion of #61; marketplace objects deferred

## Outcome

This slice makes a finalized `PaymentClaim` transfer actual transparent MINI
balances for the first time. It preserves the existing signed claim bytes and
M1/M2/M3 reconciliation behavior while adding the ownership checks previously
missing from `mini-execution`:

- exact genesis allocation;
- payer debit and payee credit;
- insufficient-funds rejection;
- canonical in-block ordering;
- per-payer sequence/replay behavior;
- balance and allocation commitments in the finalized state root; and
- the invariant that accounts plus unallocated circulating MINI equal the
  monetary ledger's circulating supply.

This is deliberately called **transparent Tier 0**. Account identifiers,
balances, amounts, and transfers are visible to a node executing the state.
It does not activate or weaken the unaudited `mini-value` privacy prototypes.

## State

`LedgerState` contains:

```text
finalized claim high-water marks
monetary supply and vesting state
sorted opaque-account -> u128 micro-MINI balances
consensus-tracked allocated circulating total
unallocated circulating MINI
```

A Tier-0 account must parse as the currently supported default verifying-key
suite and is additionally capped at 4,096 bytes for future suite headroom.
This prevents sending value to arbitrary bytes that no implemented signer can
ever spend. Empty, unsupported, zero-valued, duplicate, oversized,
underallocated, or overallocated genesis entries fail. Stealth and future
post-quantum accounts require versioned transaction/account types rather than
being guessed from opaque bytes.

`genesis_with_supply` remains useful for supply-only tests and leaves the full
circulating amount unallocated. `genesis_with_balances` requires allocations
to sum exactly to genesis circulating supply. No hidden founder, operator, or
default account is introduced.

## Transfer algorithm

Claims remain ordered by their position in the finalized block:

1. Verify the existing `PaymentClaim` signature.
2. Require the payee to be a supported, bounded verifying-key account.
3. Require the sequence to exceed the payer's finalized high-water mark.
4. Require the payer's current spendable balance to cover the amount.
5. Debit payer and credit payee with checked arithmetic.
6. For self-transfer, leave the balance unchanged.
7. Record the exact finalized claim digest and sequence.
8. Recompute supply conservation before accepting the resulting state.

Malformed, stale, conflicting, or insufficient claims are not finalized and
do not consume a sequence. If two individually valid claims would jointly
overspend, canonical body order decides: the first affordable claim transfers;
the later unaffordable claim does not.

The current canonical view proves finalized claims, not every reason a claim
was omitted. An unaffordable claim can therefore remain `pending` in
`mini-settlement::reconcile` until expiry (or until a later finalized sequence
supersedes it). A bounded, consensus-authenticated rejection receipt is needed
before wallets can distinguish this case immediately without trusting an
operator.

There is no local/offline mutation path. A claim remains a promise until the
existing quorum-finalized block application commits the post-transfer state.

## Supply conservation

Every accepted state enforces:

```text
sum(transparent account balances)
  + unallocated circulating
  = monetary circulating supply
```

Transfers preserve the left-hand total. When a finalized D-0414 epoch makes
additional vested MINI circulating, the delta enters `unallocated
circulating`; it is not silently assigned to an account. Human Share,
service, and treasury crediting require separately authenticated claim/evidence
transitions.

Locked monetary supply is absent from both balances and unallocated
circulating, so it cannot be spent through `PaymentClaim`.

Ordinary block execution checks conservation in O(1) using the tracked
allocated total. `verify_balance_map_total` performs an O(number of accounts)
full recomputation for audit, snapshot import, and recovery. The current state
commitment still walks the full sorted map; replacing that with an
authenticated incremental tree belongs with #45 state proofs/sync.

## Privacy boundary

This state is a correctness baseline, not the intended final privacy tier.
Integrating confidential MINI requires a separate transaction type that
validates:

- input ownership without publishing the owner;
- key images/nullifiers against canonical spent state;
- range proofs for every hidden output;
- exact commitment balance including fees;
- no inflation through malformed commitments;
- bounded proof sizes and verification cost;
- recovery and post-quantum migration; and
- compatibility with offline-promise risk labels.

The existing `mini-value` stealth, ring-signature, and Bulletproof prototypes
are founder-reviewed but externally unaudited. This proposal does not route
real value through them or market transparent accounts as private.

## Explicit non-goals

- No production genesis allocation.
- No Human Share membership/nullifier claim.
- No service or treasury credit authorization.
- No fees, nonce reservation, mempool, or transaction gossip.
- No canonical rejection receipt or immediate insufficient-funds wallet result.
- No confidential amount or sender/recipient privacy.
- No marketplace order, escrow, swap, refund, or dispute object.
- No legacy-currency custody or guaranteed redemption.
- No balance-derived governance, personhood, ranking, or public right.

## Migration and rollback

Before activation this is removable proposal code. Existing claim signatures
do not change. A future activated transparent ledger could migrate to a
confidential representation only with a supply-conserving, independently
audited state transition. It may not reset balances, confiscate funds, or
silently reinterpret account identifiers.

# mini-bounty

Anonymous developer-bounty claims — founder direction (2026-07-08):
"how can we make it so that they all can get their own pieces without
everyone knowing who they were, in a way that they sign on GitHub and its
read by Mininet on approved push and bounty released."

## The construction

No new cryptography — this crate composes two prototypes already built
and reviewed in `mini-value` (D-0036):

- **`BountyPool`/`BountyGrant`** — when a contribution is approved (a
  human maintainer reading GitHub, never anonymous at that step), only the
  contributor's one-time claim public key is published as a grant. The
  ring is every grant ever issued to the pool, claimed or not — shrinking
  it to "still unclaimed" would unmask the last claimant.
- **`claim`/`verify_claim`** — the contributor proves membership in the
  pool via a `mini_value::MininetRingSignature`, directing payout to a
  fresh `mini_value::MininetStealthAddress`-derived address. The message
  signed is a length-prefixed encoding of `(pool_id, payout_address)`, so
  a valid claim can't be replayed against a different pool or have its
  payout address swapped in transit.
- **`KeyImageLedger`** — the ring signature's key image prevents the same
  grant paying out twice, tracked the same shape `mini_presence::
  ReplayGuard` uses for nonce tracking. `InMemoryKeyImageLedger` is for
  tests only; production needs durable, consensus-backed storage.

## What this crate is not

- **Not a GitHub integration.** Reading PR-approval events and minting
  grants from them doesn't exist here — this is the claim cryptography
  only.
- **Not anonymous from GitHub.** GitHub/Microsoft always knows who pushed
  what. The anonymity here is from Mininet and the public ledger, the same
  scoping `mini-bearer`'s channel already uses for network observers.
- **Not cleared for real value.** Gated by D-0047 like every other
  `mini-value`/`mini-treasury` prototype — external audit required before
  production payouts, even though no new primitive is introduced.

## Build & test

```sh
cargo test -p mini-bounty
```

License: CC0-1.0 (public domain).

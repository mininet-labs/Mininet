# mini-spacetime

Block-production selection weight from committed storage capacity — the
proof-of-space-time half of the whitepaper's hybrid consensus (§8.1),
deliberately separate from `mini-chain`'s equal-weight-per-human finality
voting (`docs/DECISION_LOG.md` D-0035 point 3).

## Two axes that must never be confused

- **Block production** (this crate): weighted by *proven* storage capacity
  — a concave curve (integer square root), a per-identity cap, and a bounded
  diversity bonus, so doubling capacity never doubles weight.
- **Finality** (`mini-chain`, already shipped): a committee sampled from
  verified humans, equal weight per human, never stake.

`proposer_weight` returns a plain `u64` with no connection to
`did_mini::Capabilities::VOTE` and no shared type with
`mini_chain::ValidatorSet` — storage capacity can make a node more likely to
*propose* a block, never make a human's vote count for more (P1 [FREEZE]).

## Honest limits

This crate computes weight from *already-proven* capacity; it does not
prove capacity itself. `ProofOfSpaceTimeSource` is the seam a real
proof-of-space-time/proof-of-replication protocol fills in. Per the
whitepaper ("the most demanding engineering in the value layer...
implemented human-only and externally audited") and D-0035 point 5, that
protocol requires human cryptographic authorship and external audit before
real deployment — not AI-authored code. `NoProof` is the correct, permanent
implementation until that work exists. Proposer rotation (turning weights
into an actual leader-election mechanism), the state machine, and
networking are further work.

## Build & test

```sh
cargo test -p mini-spacetime
```

License: CC0-1.0 (public domain).

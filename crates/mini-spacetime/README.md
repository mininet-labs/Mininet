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

## Proving capacity: start simple, real proof-of-replication later

`ProofOfSpaceTimeSource` is the seam; `storage_proof::MerkleStorageProof`
is a real, working implementation (D-0037/D-0038's founder direction —
start with the simpler, well-documented construction now): a Merkle tree
(`merkle::MerkleTree`) over stored blocks, periodic random challenges, and
proof of *actual* possession (the responder must return real block bytes
that hash into the previously-committed root). `ProofHistory`/
`StorageWindowPolicy` require this to succeed repeatedly, without too
large a gap, over a real span of time (month-scale by default) before
capacity counts as proven — the "time" half of proof-of-space-**time**.

**What this does not prove: replication uniqueness.** This scheme cannot
tell a thousand honest small devices each holding their own copy apart
from one well-resourced server answering every challenge from a single
copy — exactly the warehouse-consolidation attack the whitepaper's
"thousand cheap machines beat one warehouse" thesis (§7) depends on
resisting. Real proof-of-replication (Filecoin-style sequential/time-
locked encoding) is the construction that closes that gap, and is
deliberately treated as a separate, later, dedicated project rather than
compressed into this pass. `NoProof` remains available as the fail-closed
reference. Proposer rotation (turning weights into an actual leader-
election mechanism), the state machine, and networking are further work.

## Build & test

```sh
cargo test -p mini-spacetime
```

License: CC0-1.0 (public domain).

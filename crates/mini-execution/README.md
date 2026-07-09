# mini-execution

The smallest deterministic state machine that ties [`mini-chain`](../mini-chain)'s
finality verification to [`mini-settlement`](../mini-settlement)'s offline-payment
reconciliation — a real, chain-backed `CanonicalLedgerView`. Closes
[roadmap #40](../../issues/40) ("double-spend reconciliation rules") and the
required follow-up D-0055 named: `mini-settlement`'s `reconcile`/
`CanonicalLedgerView` split, now backed by something real instead of only
`InMemoryLedgerView`.

## The construction

- **`SettlementBlockBody`** — an ordered list of `PaymentClaim`s proposed at
  one height. Order **is** the canonical order M3 requires.
- **`LedgerState`** — for each payer, only the *latest* finalized
  `(sequence, digest)` pair — deliberately all `mini_settlement::reconcile`
  ever reads from a `CanonicalLedgerView`, so this state carries no more
  history than the protocol it backs actually needs. Implements
  `CanonicalLedgerView` directly; `commitment()` gives a block header's
  `state_root` real meaning.
- **`apply_block`** — the state transition: a claim wins its
  `(payer, sequence)` slot only by strictly exceeding that payer's current
  high-water-mark; a bad signature, a stale sequence, or a second claim at
  an already-decided slot is silently dropped, never merged (M1).
- **`LedgerChain`** — the one thing that matters most: state only ever
  advances behind a *real, verified* `mini_chain::QuorumCertificate`. There
  is no path to apply a block's claims without first proving it final —
  M2's "offline payment is never final until canonical inclusion" made
  structurally impossible to bypass, not just documented.

## What this proves, and how

- **Double-spend resolution** (`tests/end_to_end.rs`): two conflicting
  claims at the same `(payer, sequence)`, proposed in two competing block
  bodies — only the one that actually gets a real quorum certificate ever
  finalizes; `reconcile()` against the resulting `LedgerState` reports
  exactly one `Finalized`, the other `RejectedConflict`.
- **Two honest nodes never disagree** (Directive 4): two independent
  `LedgerChain`s, fed the identical sequence of `(header, body, qc)`
  inputs, are proven — not just argued — to reach bit-identical
  `LedgerState` commitments at every height.
- **An unfinalized block is never applied**: fewer than quorum precommits,
  a wrong height, a wrong parent hash, or a header lying about its
  resulting `state_root` are each rejected before the state changes at
  all.

## What this crate is not

- **Not networked consensus.** No proposer rotation, no vote gossip, no
  round timeouts/view-change — `mini-chain`'s own stated non-goals, which
  this crate inherits rather than closes. Given a `(header, body, qc)`
  triple from *somewhere* (a real network, eventually — roadmap #36-#45),
  this crate answers "is this the next state" precisely and
  deterministically.
- **Not a general execution engine.** One transaction type:
  `PaymentClaim`. Governance, storage receipts, bounty claims, and
  whatever else eventually anchors to a real chain are separate,
  further work — the same relationship `mini-forge`'s docs describe
  ("the chain replaces the counting, not the objects").
- **Not gated behind D-0047.** No new cryptography — this composes
  `mini-chain`'s finality verification and `mini-settlement`'s claim
  verification; the only new content is deterministic bookkeeping and one
  content hash (`LedgerState::commitment`).

## Build & test

```sh
cargo test -p mini-execution
```

License: CC0-1.0 (public domain).

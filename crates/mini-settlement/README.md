# mini-settlement

Offline transaction settlement (roadmap [#41](../../issues/41)) —
founder direction, Directive 5: *"during outages, users exchange signed
promises — not final ownership. Ownership changes only when accepted into
canonical consensus."*

## The construction

- **`PaymentClaim`** — a signed promise: payer, payee, amount, a monotonic
  per-payer sequence, a validity window, and a reference to the chain state
  the payer last saw. The message is length-prefixed and domain-tagged
  (`mini-settlement/payment-claim/v1`), the same discipline `mini-bounty`
  uses, so no two distinct claims can ever collide on the wire.
- **`SettlementState`** — the wallet-facing state machine M2 requires:
  `SignedLocal → AcceptedLocal → PendingCanonical → Finalized |
  RejectedConflict | Expired`. Only `Finalized` is final —
  `SettlementState::is_final()` is the one function a wallet should ever
  call to decide "is this money mine."
- **`ClaimWatcher`** — local, offline-capable conflict detection: catches
  the cheapest double-spend attempt (a payer showing two different signed
  claims for the same sequence to two different recipients) before either
  recipient wastes trust on it. Same shape as `mini_presence::ReplayGuard`.
- **`CanonicalLedgerView`** — the seam to the not-yet-built chain-execution
  engine (roadmap #36-#45). `reconcile()` is fully specified and tested
  against this trait today; a real chain-backed implementation plugs in
  later with no change to the reconciliation rules.

## The three frozen invariants this implements (`docs/INVARIANTS.md` §4)

- **M1** — money never CRDT-merges. There is no merge function anywhere in
  this crate.
- **M2** — offline payment is a pending claim, never final, until canonical
  inclusion.
- **M3** — canonical ordering alone resolves conflicting spends; nothing in
  this crate finalizes a claim on its own authority.

## What this crate is not

- **Not a ledger.** `CanonicalLedgerView` is a trait; the real balance/
  execution engine is separate, future work.
- **Not a payment channel.** Direct signed claims, not bilaterally-signed
  revocable channel state — the simpler primitive Directive 5's own wording
  implies.
- **Not confidential.** Plain `u64` micro-MINI amounts, same convention as
  `mini-bounty`/`mini-reward`. Wiring `mini-value`'s confidential amounts in
  is separate future work.
- **Not D-0047-gated** — no new cryptography, only already-reviewed
  `mini-crypto` primitives composed into a state machine. The audit gate
  that matters is on whatever real ledger eventually backs it.

## Build & test

```sh
cargo test -p mini-settlement
```

License: CC0-1.0 (public domain).

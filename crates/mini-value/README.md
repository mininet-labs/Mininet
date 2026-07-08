# mini-value

Transaction privacy for the one MINI ledger (whitepaper §8, D-0035 point
4) — the most conservative crate in this batch, built last, per founder
direction: highest risk, real value, real cryptography.

## Ordinary bookkeeping, no cryptography

- `fee` — the governed fee mechanism (whitepaper §8.4): a real-world value
  target converted into a MINI amount at a governed price, so a view costs
  a steady fraction of a cent regardless of MINI's market price. Same shape
  and safety class as `mini_treasury::rate`.

## Founder-overridden (D-0036/D-0037), AI-authored prototypes

Real implementations, not stubs — but explicitly pending external
cryptography audit, per the founder cohort's direct decision to override
D-0035 point 5 (D-0036 for ring signatures/stealth addresses specifically,
D-0037 generalizing the policy to confidential amounts too):

- `stealth_impl::MininetStealthAddress` — a CryptoNote-style stealth
  address scheme: a fresh one-time output address per payment, unlinkable
  to the recipient's real address, recognized by the recipient's view key
  alone (spend key stays colder).
- `ring_impl::MininetRingSignature` — a single-layer MLSAG/AOS-style
  linkable ring signature: proves one of N public keys authorized a spend
  without revealing which, plus a key image for double-spend detection.
- `confidential_impl::MininetConfidentialAmount` — a single-value
  Bulletproofs range proof: a Pedersen commitment `V = blinding*G + value*H`
  plus an `O(log n)`-size proof that `value ∈ [0, 2^64)`, via bit
  decomposition, a folded polynomial `t(X) = <l(X), r(X)>`, and the inner
  product argument (`bp_ipa`) to compress the opening. `verify_balance`
  needs no separate proof beyond the commitments themselves: Pedersen
  commitments are additively homomorphic, so checking inputs balance
  outputs is exactly an elliptic-curve point-sum equality check.

All three are built on `curve25519-dalek`'s Ristretto group (the same
audited primitive-layer crate `ed25519-dalek`/`x25519-dalek` already use,
D-0014's precedent) — the group arithmetic is depended on, the protocols
on top are Mininet-owned, referencing published designs rather than any
existing ring-signature/range-proof crate. Ristretto (not raw
Edwards/Curve25519 points) avoids the cofactor-related subtle-bug class
ad-hoc protocols are prone to. The Bulletproofs range proof additionally
uses independent, hash-derived (nothing-up-my-sleeve) generators separate
from the signing-key basepoint, so the commitment scheme never shares a
discrete-log relationship with anything signature-related — and its two
load-bearing algebraic identities (the IPA folding relation, and the
`t0 = value·z² + delta(y,z)` constant-term relation) were hand-derived and
cross-checked term-by-term before implementation, documented in
`bp_range`'s module docs.

`NoStealthAddress`/`NoRingSignature`/`NoConfidentialAmount` remain
available as fail-closed references for anyone not opting into the
prototypes.

**[FREEZE reminder — D-0036/D-0037]** These prototypes are founder-
reviewed, not externally audited. Nothing in this crate should be read as
"privacy achieved" for real value until that audit happens.

Not a second currency: these are transaction-privacy primitives for MINI,
the same one currency `mini-reward`'s vesting accrual feeds into.

## Build & test

```sh
cargo test -p mini-value
```

Note: the Bulletproofs range-proof tests are noticeably slower in debug
builds (unoptimized big-integer curve arithmetic) — correct and fast
(well under a second) under `cargo test --release`.

License: CC0-1.0 (public domain).

# mini-value

Transaction privacy for the one MINI ledger (whitepaper §8, D-0035 point
4) — the most conservative crate in this batch, built last, per founder
direction: highest risk, real value, real cryptography.

## Ordinary bookkeeping, no cryptography

- `fee` — the governed fee mechanism (whitepaper §8.4): a real-world value
  target converted into a MINI amount at a governed price, so a view costs
  a steady fraction of a cent regardless of MINI's market price. Same shape
  and safety class as `mini_treasury::rate`.

## Founder-overridden (D-0036), AI-authored prototypes

Real implementations, not stubs — but explicitly pending external
cryptography audit, per the founder cohort's direct decision to override
D-0035 point 5 for these two primitives specifically:

- `stealth_impl::MininetStealthAddress` — a CryptoNote-style stealth
  address scheme: a fresh one-time output address per payment, unlinkable
  to the recipient's real address, recognized by the recipient's view key
  alone (spend key stays colder).
- `ring_impl::MininetRingSignature` — a single-layer MLSAG/AOS-style
  linkable ring signature: proves one of N public keys authorized a spend
  without revealing which, plus a key image for double-spend detection.

Both are built on `curve25519-dalek`'s Ristretto group (the same audited
primitive-layer crate `ed25519-dalek`/`x25519-dalek` already use, D-0014's
precedent) — the group arithmetic is depended on, the protocols on top
(key derivation, challenge/response construction, key-image linkability)
are Mininet-owned, referencing published designs rather than any existing
ring-signature crate. Ristretto (not raw Edwards/Curve25519 points) avoids
the cofactor-related subtle-bug class ad-hoc protocols are prone to.

`NoStealthAddress`/`NoRingSignature` remain available as fail-closed
references for anyone not opting into the prototypes.

**[FREEZE reminder — D-0036]** These prototypes are founder-reviewed, not
externally audited. Nothing in this crate should be read as "privacy
achieved" for real value until that audit happens.

## Still a stubbed seam (D-0035 point 5, untouched by D-0036)

- `confidential` — RingCT-style confidential amounts (homomorphic
  commitments + range proofs hiding amounts while still proving no value
  was created). `NoConfidentialAmount` still fails closed: range-proof
  soundness is exactly the kind of property with no safe middle ground,
  and D-0036 named only ring signatures and stealth addresses.

Not a second currency: these are transaction-privacy primitives for MINI,
the same one currency `mini-reward`'s vesting accrual feeds into.

## Build & test

```sh
cargo test -p mini-value
```

License: CC0-1.0 (public domain).

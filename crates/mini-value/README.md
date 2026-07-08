# mini-value

Transaction privacy for the one MINI ledger (whitepaper §8, D-0035 point
4) — the most conservative crate in this batch, and the last one built,
per founder direction: highest risk, real value, real cryptography.

## Safe to build now (ordinary bookkeeping, no cryptography)

- `fee` — the governed fee mechanism (whitepaper §8.4): a real-world value
  target converted into a MINI amount at a governed price, so a view costs
  a steady fraction of a cent regardless of MINI's market price. Same shape
  and safety class as `mini_treasury::rate`.

## Deliberately not built here — trait seams only (D-0035 point 5)

- `ring` — ring signatures (prove one of N keys authorized a spend, without
  revealing which, plus a key image for double-spend detection).
  `NoRingSignature`.
- `stealth` — stealth addresses (a fresh one-time output address per
  payment, unlinkable to the recipient's real address). `NoStealthAddress`.
- `confidential` — RingCT-style confidential amounts (homomorphic
  commitments + range proofs hiding amounts while still proving no value
  was created). `NoConfidentialAmount`.

Every stub in this crate **fails closed**: none of them sign, derive,
commit, or verify anything as valid. Nothing here should be read as
"privacy achieved" — these are the seams a human-authored, externally-
audited implementation fills in. Getting any of the three wrong is
catastrophic in a way that a stub cannot be: a flawed ring signature can
deanonymize a signer, a flawed stealth-address scan can misattribute an
output, a flawed range proof can let value be created from nothing. See
each module's own honest limit.

Not a second currency: these are transaction-privacy primitives for MINI,
the same one currency `mini-reward`'s vesting accrual feeds into.

## Build & test

```sh
cargo test -p mini-value
```

License: CC0-1.0 (public domain).

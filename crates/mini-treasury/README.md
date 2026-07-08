# mini-treasury

Community-governed BTC/XMR-to-MINI contribution bookkeeping (whitepaper
§8.2 "how the rich contribute" / §10 treasury custody), split by risk class
the same way `mini-uniqueness` and `mini-spacetime` split their own novel-
cryptography pieces (`docs/DECISION_LOG.md` D-0035 point 5).

## Safe to build now (ordinary bookkeeping and arithmetic)

- `rate` — a governed exchange-rate history and the multiplication that
  turns a contribution into a minted amount at whatever rate was in effect.
- `receipt::ContributionReceipt` — the bookkeeping record of a claimed
  contribution (asset, amount, rate, minted MINI).
- `signers` — **who** is authorized to approve treasury actions and whether
  enough of them agreed (`TreasurySignerSet`, `meets_threshold`), mirroring
  `mini-forge`'s governance approval-counting pattern: distinct-identity
  counting only, no weight field, no path to extra voting power for being a
  signer (P1 unchanged).

## Deliberately not built here

Whitepaper §11: "bridge and treasury custody is a permanent honeypot by
nature." D-0035 point 5 requires human authorship and external audit for:

- `receipt::ExternalReceiptOracle` — verifying a Bitcoin or Monero
  transaction actually paid the treasury is real cross-chain engineering
  (confirmation depth, reorg safety, Monero's view-key/output-scanning
  machinery). `NoExternalReceiptOracle` is the correct, permanent stand-in.
- Real threshold-signature custody (e.g. FROST) over actual treasury funds.
  `meets_threshold` answers "did enough authorized people agree," never
  "here is a valid signature the treasury would accept" — that scheme does
  not exist in this crate.

This crate is bookkeeping and governance-membership data, not a deployable
treasury.

## Build & test

```sh
cargo test -p mini-treasury
```

License: CC0-1.0 (public domain).

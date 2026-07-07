# mini-reward

A deterministic, **non-spendable** local reward-accrual stub. It makes "presence becomes protocol value" visible in the demo before any chain exists — a
pure function over verified `mini-presence` verdicts.

## Model

Per **identity root** (never per device — P2), from co-presence events:

- **Diversity-weighted.** A fresh counterparty is worth `base_points`; repeated
  encounters with the *same* counterparty halve (`base >> k`) and stop after a cap.
  Meeting many distinct identity roots pays; farming one partner does not.
- **Rate-capped.** Accrual within any time window is capped (the P4 slow brake).
- **Maturation.** A contribution only vests after a delay, so recent presence
  can't be cashed in immediately (P4, presence-conditioned).

`accrue(identity_root, verdicts, params, now)` returns one account; `ledger(...)` returns
all identity roots, sorted, reproducibly.

## What it deliberately is not

- **Not money** — no transfer, no balance ledger, no spend. The chain reward module
  is the real thing later.
- **Not a vote** — a `RewardAccount` has no governance weight (P1: money never buys
  voice).
- **Not Sybil resistance** — diversity-weighting and caps only blunt farming;
  proving humanness is personhood's job (SPEC-02).

## Build & test

```sh
cargo test -p mini-reward
```

License: CC0-1.0 (public domain).

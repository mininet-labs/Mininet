# mini-economy

Deterministic monetary-policy primitives for MINI.

This crate implements the D-0074 annual issuance envelope with checked,
integer-only arithmetic:

- up to 3% gross issuance per year;
- a protected 2% Human Share, divided equally among the epoch's eligible
  verified humans and vested for 365 days;
- up to 0.75% for evidenced network services;
- up to 0.25% for treasury contributions, vested for 90 days; and
- expiry of unused optional capacity.

It also constructs content-addressed, equal-allocation genesis manifests.
The bootstrap quantity and eligible set are governance/personhood inputs;
this crate cannot decide or activate either.

For ordinary epochs, `plan_human_share` consumes only a finalized snapshot
root and eligible-human count, so the policy calculation remains constant
space at a population of billions. It does not define or verify membership
proofs for that root.

`Amount` uses `u128` atomic units while existing payment claims remain
wire-compatible `u64` micro-MINI. Moving a balance into this accounting type
is lossless. A wire-format migration requires a separate versioned decision.

This crate does not move funds, prove personhood, select reward recipients,
verify external assets, supply an oracle, activate genesis, or make MINI a
redeemable claim on anything.

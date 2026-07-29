# mini-econ-sim

Deterministic cohort simulator for the D-0074 MINI issuance envelope.

Run:

```text
cargo run -p mini-econ-sim > economy-200y.csv
```

The built-in run covers 200 years and emits integer CSV. The library exposes
scenario inputs for population growth, dormancy, verified Sybil identities,
whale opening share, and optional-channel utilization.

Unlike the earlier Python prototype, Human Share is allocated equally per
active identity rather than in proportion to existing holdings. The harness
is still not an economic proof: it does not model prices, markets, external
asset shocks, service-reward concavity, transaction demand, or personhood
costs. Those remain explicit review work.

# Day-0 economy engineering review

**Review type:** internal adversarial self-review
**Maturity:** proposal evidence, unaudited
**Independent review:** required before real value

## Reviewed surface

- `crates/mini-economy`
- `crates/mini-econ-sim`
- D-0073 and D-0074 compatibility
- existing `u64` micro-MINI settlement compatibility

## Positive findings

- Issuance uses checked integer arithmetic and explicit epoch duration.
- Human allocation is equal per canonicalized beneficiary, independent of
  holdings.
- Optional channels cannot exceed their own or aggregate ceiling.
- Unused capacity is not reassigned.
- Genesis ordering is deterministic and has no privileged-recipient field.
- Existing `u64` amounts widen losslessly to the new `u128` accounting type.
- The new simulator represents equal Human Share and makes assumed verified
  Sybils visible instead of hiding the personhood dependency.

## Blocking findings

| ID | Severity | Finding | Required closure |
|---|---|---|---|
| ECON-01 | Critical | No canonical chain execution consumes these plans. A plan is not a mint. | Versioned encoding, authorization, replay protection, supply invariant, consensus integration. |
| ECON-02 | Critical | Personhood is unresolved; a supplied Sybil set receives equal Human Share. | Independent personhood research and adversarial deployment evidence. |
| ECON-03 | Critical | No production genesis amount or eligible snapshot is authorized. | Public proposal, independent modeling, governance decision, reproducible snapshot ceremony. |
| ECON-04 | Critical | External treasury/bridge custody and receipt verification remain unaudited. | Close A1/#47/#93/#99 gates with independent reviewers. |
| ECON-05 | High | Service allocations are precomputed inputs; useful-work verification and concavity are outside the kernel. | Integrate reviewed evidence roots and reward-curve policy. |
| ECON-06 | High | Simulator omits market prices, fees, liquidity, storage economics, treasury reserves, oracle shocks, collusion, and off-protocol influence. | Extend scenario engine and obtain mechanism-design review. |
| ECON-07 | High | `u128` is not yet a versioned wire/ledger amount. | Keep current wire unchanged until explicit migration and compatibility vectors exist. |
| ECON-08 | High | Vesting grants have no chain-enforced spend lock. | Implement and test consensus-enforced vesting before activation. |
| ECON-09 | Medium | A single opening-circulating input could be dishonest or stale. | Bind requests to a finalized supply checkpoint and reject duplicate epochs. |
| ECON-10 | Medium | Rounding leaves dust unissued and may make realized Human Share fraction slightly below 2%. | Keep deterministic floor rule; document and test aggregate dust accounting. |

## Threat review

- **Inflation forgery:** bounded in planning, not prevented at consensus.
- **Duplicate epoch:** not prevented by this crate.
- **Beneficiary duplication:** exact duplicate strings are rejected; aliases
  and multiple identity roots are a personhood-layer problem.
- **Overflow:** checked and tested; no saturation in monetary calculations.
- **Governance capture by wealth:** no API exists here for governance weight;
  this does not stop off-network bribery or dependencies elsewhere.
- **Long-horizon capture:** perpetual equal entry dilutes passive early
  concentration, but market acquisition and service concentration still need
  modeling.
- **External-value deception:** the design rejects guaranteed redemption and
  distinguishes representation from custody; UI/provider enforcement is not
  implemented here.

## Evidence run

```text
cargo test -p mini-economy
cargo test -p mini-econ-sim
cargo run -p mini-econ-sim > economy-200y.csv
```

The tests establish deterministic arithmetic properties only. They do not
establish that D-0074’s parameters are economically safe, that identities are
humans, that external assets exist, or that a future chain will enforce the
plans.

## Recommendation

Accept this as a narrow policy-kernel and calibration-harness proposal. Do not
activate issuance or describe the economic system as production-ready until
all critical findings have independent closure evidence and the exact reviewed
release passes the repository’s governed release process.

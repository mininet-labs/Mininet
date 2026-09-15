# Stable MINI purchasing power across generations

**Status: research prototype and proposal for review. No policy activation, payment authorization, price peg, backing, external audit or launch forecast.** Prepared 12 September 2026 against repository checkpoint `8d7dbe8512720ef2a61c4d2a94127fb8ddf66623`.

## The objective and the correction

One MINI should continue to buy roughly the same basket of useful goods and services even as new MINI is issued. Stability of purchasing power **per MINI** is the objective. Stability of an equal annual grant's value per person is a different property and does not satisfy that objective.

The offline review tool at `tools/sim/economics-planning/index.html` now compares two explicit paths:

1. **Existing issuance envelope:** protected Human Share and optional service/contribution ceilings, with the proposed strict annual guard shown separately from monthly-only limits. Calculate what happens to buying power under the assumed real wealth path.
2. **Stable-unit target:** hold opening purchasing power constant and calculate the net supply path compatible with that target. This counterfactual does not implement the existing issuance floor, vesting, contraction mechanism or a market peg. Divergence is a finding, not a hidden policy change.

The earlier 200-year operating/compensation study remains in [economics-compensation.md](economics-compensation.md). Its reference-EUR budgets and compensation engine are separate from this reference-USD wealth exercise. Its older circulating-supply price convention is not used for the new full-wealth-interest target.

## 1. The arithmetic that must hold

Let `W(t)` be external real net wealth, expressed in a constant purchasing-power reference basket; `a(t)` the fraction of that value economically attributed to MINI; and `S(t)` total issued MINI, including vested and still-locked grants. Under this particular wealth-valuation premise:

```
P(t) = a(t) × W(t) / S(t)
P(t+1) / P(t) = [a(t+1)/a(t)] × [W(t+1)/W(t)] / [S(t+1)/S(t)]
```

For a fixed purchasing-power target `P*`:

```
S*(t) = a(t) × W(t) / P*
net issuance compatible with the target = S*(t+1) − S*(t)
1 + supply growth = (1 + attribution growth) × (1 + real wealth growth)
```

These identities are necessary **inside the chosen valuation premise**, not a causal law guaranteeing a traded currency's market price. Constant attribution and 3% supply growth require 3% real supporting-value growth to preserve purchasing power. If supporting value grows 2%, buying power changes by `1.02 / 1.03 − 1 = −0.9709%`; if it stays flat, the change is `1 / 1.03 − 1 = −2.9126%`. Repeating even small annual losses is not long-horizon stability.

If supporting value shrinks, constant purchasing power requires net contraction or another source of demand/support. Zero issuance alone may be insufficient. No mandatory confiscation, burn, unfunded buyback or treasury intervention is introduced here.

Population alone does not set price. Write `W(t) = N(t) × w(t)`, where `w` is real wealth per person. At constant attribution, compatible supply growth is `(1 + population growth) × (1 + real per-capita wealth growth) − 1`. Population decline can coexist with increasing real wealth if productivity/wealth per person grows sufficiently.

## 2. What “all human wealth” can and cannot mean

Three different claims must remain distinct:

| Meaning | Consequence |
|---|---|
| All assets and services are quoted in MINI | MINI is a unit of account. This alone says nothing about MINI capitalization or ownership of those assets. |
| MINI is a broadly held settlement/store-of-value asset | Willing holdings, trade demand, substitutes, velocity, liquidity and confidence matter. Wealth attribution `a` is a scenario, not measured automatic adoption. |
| MINI is an enforceable pro-rata claim on external assets | Requires actual rights, assets and an enforceable settlement/custody arrangement. The repository does not grant that claim, and this proposal does not create it. |

Use `a=100%` to test the user's full-wealth premise and 10%, 1%, 0.1% and 0% as sensitivities. Attribution is independent of the fraction of humans participating. It is not legitimate to infer full-world wealth backing from a small initial participant cohort. At zero attribution, price is zero and a nonzero purchasing-power target cannot be inferred from it.

`W` must exclude double-counting MINI claims over the same underlying assets. Do not add asset value, claims on those assets and MINI capitalization together as newly created wealth. Wealth is a stock; GDP, wages and service costs are flows. Human dignity, people themselves and the future output of unborn people are not token-owned assets. Homes, ecosystems and knowledge are not all immediately saleable reserves. A large quoted capitalization cannot pay an operating bill without willing counterparties and real spendable resources.

FD-18 and the Failure Book reject protocol-owned legacy-currency issuing, exchange and custodial relationships. The full-wealth arithmetic is therefore a valuation thought experiment, not a proposal to put humanity's assets in a protocol-controlled vault. A currency's purchasing-power objective needs demand and monetary-mechanism research, not a claim that Mininet owns humanity.

## 3. Population and wealth inputs

The included population extract is from UN DESA Population Division, **World Population Prospects 2024**, GEN/01/REV1, medium variant, World, total population on 1 July. The official workbook was retrieved on 12 September 2026; `population.json` records its SHA-256, license and URL. All 75 annual values from 2026 through 2100 are included; thousands are converted to whole people. These are projections from the 2024 revision, not live counts.

| Checkpoint | People | Evidence status |
|---|---:|---|
| 2026 | 8,300,678,395 | UN medium projection for the current year |
| 2084 | 10,289,315,244 | Peak within the extracted UN series |
| 2100 | 10,180,160,751 | End of the UN series |
| 2126 | Scenario-dependent | 100-year horizon; beyond the UN series |
| 3026 | Scenario-dependent | 1,000-year stress horizon; no demographic forecast claim |

After 2100, compare stable population, −0.1% annual decline, and +0.2% annual expansion. The latter is compatible with an imagined expanding human settlement footprint but is not a forecast of space settlement. A fourth control holds 2026 population fixed throughout.

Default external wealth is an **illustrative USD 500 trillion** in constant reference purchasing power. This is not an asserted measured 2026 total. The [UBS Global Wealth Report 2026](https://www.ubs.com/us/en/wealth-management/insights/global-wealth-report.html) is contemporary context; its report covers 56 markets estimated to represent over 92% of world wealth and revises some methodologies. Its scope is personal wealth, not every definition of “all human wealth.” We do not extrapolate its annual USD growth into a millennium, mix nominal growth with real buying power, or claim its methodology values all human and natural capital.

Real wealth per person defaults to zero growth. Sensitivities permit −2% to +2% annually for a selected number of years, then flat. The default growth-duration limit is 100 years, not perpetual compounding. A 1% rate sustained for 1,000 years multiplies wealth per person about 20,959 times; that is an extreme mathematical scenario requiring physical and ecological justification, not an expected value. No scenario probabilities are supplied because we have no defensible calibration for them.

## 4. Concrete stable-unit supply paths

To make arithmetic reviewable, assume opening supply of **1 billion MINI**, full wealth attribution, USD 500 trillion opening external wealth, and constant real wealth per person. The implied opening unit price is USD 500,000 solely because of that chosen denomination. Neither the opening supply nor that price is an adopted genesis setting.

Under the stable-unit target, supply follows population proportionally and price remains at the opening buying-power index of 100:

| Population scenario | People in 2126 | Target supply in 2126, MINI | People in 3026 | Target supply in 3026, MINI |
|---|---:|---:|---:|---:|
| Current population held fixed | 8.301 billion | 1.000 billion | 8.301 billion | 1.000 billion |
| UN then stable | 10.180 billion | 1.226 billion | 10.180 billion | 1.226 billion |
| UN then −0.1%/year | 9.919 billion | 1.195 billion | 4.031 billion | 0.486 billion |
| UN then +0.2%/year | 10.723 billion | 1.292 billion | 64.754 billion | 7.801 billion |

These are rounded counterfactual target supplies. Declining supply implies contraction for which this proposal offers no activated mechanism. For the stable-after-2100 case, target net issuance becomes zero after 2100 at flat real wealth per person. For +0.2% population growth and flat per-capita wealth, compatible issuance is +0.2%, not automatically +3%.

The first UN step, 2026→2027, permits about 0.824% total supply growth under these assumptions. At 1 billion opening MINI, that is about 8.24 million net MINI. The protected Human Share alone is approximately 20 million MINI at the opening annual rate before monthly compounding and equal-allocation rounding. The tension exists immediately; it is not merely a year-3026 issue.

At constant population, a 2% real wealth-per-person growth assumption permits 2% net supply growth. To permit 3% supply growth with 0.2% population growth, real wealth per person must grow `1.03/1.002 − 1 ≈ 2.7944%` annually. This is a required condition, not a promised productivity rate.

## 5. Compare the existing issuance envelope honestly

The exact-integer simulation preserves the recorded 2% annualized Human Share, up to 0.75% services and up to 0.25% qualifying contributions, calculated on epoch-opening circulating supply. Human grants vest over 365 policy days; contribution grants over 90. Monthly-only caps are compared against a **proposed**, separate annual-opening 3% total guard that reduces optional issuance first. The current envelope layer is not claimed to enforce that cumulative annual guard.

The default maximum-utilization, guarded, stable-population scenario produces approximately:

| Year | Total issued MINI | Buying-power index, opening = 100 | Target-compatible supply |
|---|---:|---:|---:|
| 2026 | 1.000 billion | 100 | 1.000 billion |
| 2126 | 18.624 billion | 6.585 | 1.226 billion |
| 3026 | 5.005 × 10^21 | 2.450 × 10^-11 | 1.226 billion |

This path **fails the stable-per-MINI objective** at flat real wealth per person. Its roughly stable Human Share value per person does not rescue that failure. CSV/JSON contain exact micro-MINI balances and approximate valuation figures, including annual issuance by channel, locked supply, target-compatible issuance, buying-power index and the real wealth gap required to keep the existing supply path's unit purchasing power flat.

At 3026, holding the initial unit value while retaining that supply would require roughly USD 2.502 × 10^27 of attributed external wealth, approximately 5.005 trillion times the assumed starting wealth. Do not replace the wealth series with that required series and call it a forecast.

## 6. Proposed decision, not a silent rule change

**Recommend stable purchasing power as a design objective, with issuance treated as a budget constrained by real demand/support rather than as an entitlement to maximum minting.** Retain equal treatment and the voice/value wall. Do not claim a guaranteed peg.

The protected 2% Human Share creates a specific decision boundary. A 3% ceiling need not be used fully, but reducing optional 1% issuance cannot solve every low-growth case. A persistent 2% mint floor and an exactly flat unit purchasing power cannot both be guaranteed under flat or shrinking supporting real value at constant attribution. A material amendment would need explicit classification and the applicable governance process; this research PR changes no adopted rule.

| Option for independent review | Consequence |
|---|---|
| Preserve current protected mint floor | Accept that unit buying power can drift materially; do not advertise stability as a guarantee. |
| Amend issuance to respond to sustainable real demand/value growth | May preserve the target condition, but can require Human Share minting below 2%; needs explicit rule review and robust measurement. |
| Finance equal benefits partly from actual revenues or voluntary transfers of existing MINI | Can support people without equivalent new minting; requires real, reliable funding and does not itself peg market price. |

No payment or governance privilege follows from a founder's title. The companion finite historical-work award and comparable-work compensation remain funded voluntary-sponsor proposals. They cannot justify extra minting to defend a quoted fiat salary, vote, office or permanent founder percentage.

## 7. What a real purchasing-power mechanism still needs

- Define a durable goods/services basket: useful storage, computation, bandwidth, energy and broader human necessities; specify regions, weights, quality adjustment and replacement of obsolete items. USD here is only a calculation reference, not the permanent basket.
- Specify what “fairly flat” means over what horizon, with independently reviewed tolerance bands and asymmetric shocks. No band is silently adopted by this model.
- Measure real demand and prices without a single government, exchange, custodian or oracle becoming protocol authority. Price manipulation, thin markets, subsidized fake demand, data denial and stale cross-planet prices require adversarial tests.
- Separate observations from actuation: an economic index cannot secretly authorize minting, levy a balance charge, spend a treasury or change political voice.
- Model two-sided shocks, sustained contraction, loss of confidence, transaction velocity, exchange liquidity, net asset revaluation and feedback lag. A wealth proxy cannot substitute for that market mechanism.
- Measure real operator costs and whether service rewards purchase sufficient honest capacity under low adoption. Do not equate minted reward units with earned real income.

Rebasing a displayed denomination can hold a screen number steady while diluting balances. It does not meet stable buying power per actual MINI and is not proposed as a workaround. At the illustrative opening price, one micro-MINI is USD 0.50, which is coarse for small payments. A different opening denomination or precision may be desirable, but would require separate review; it creates no real wealth.

## 8. Numerical method and limits

The long-horizon engine uses BigInt micro-MINI, floors each equal per-eligible allocation and leaves the remainder unminted. It tracks grant positions, released and locked amounts, and checks total = circulating + locked and opening + cumulative issuance exactly. Values exceeding u128 micro capacity fail. JavaScript can use larger intermediates, so this does not prove every Rust intermediate operation would succeed. USD and target-supply comparisons use floating-point arithmetic and are approximate planning quantities.

Twelve equal epochs make a 365-day policy year; year-end population is held constant during that year's epochs. This is not a civil-calendar replay. Service compensation is liquid at this envelope layer. Individual births, deaths, presence-grace cohorts, inherited vested balances, private claims, treasury gating/organic-volume limits, per-contributor epoch caps, service quality and fees/burns are not simulated. Full channel utilization is a conditional ceiling case, not production entitlement. Identity-root uniqueness does not establish unique humans.

The target-compatible supply path is algebraic, includes negative net issuance, and does not run a legal monetary controller. Exact target compatibility does not prove economic stability, causality, redeemability or market clearing. Long-run population expansion does not establish multiplanetary carrying capacity. Disconnected settlements do not gain alternative global MINI finality; local resources and pending-claim procedures remain separate engineering requirements.

## 9. Validation, authority and review handoff

Executed checks are documented in the tool README: eight 1,000-year combinations; integer conservation and guarded annual caps; current/declining/expanding population; zero attribution; participation and Sybil leakage; vesting; micro-allocation dust; overflow; invalid inputs; finite growth duration; target-path identity; flat-value zero target issuance; contraction; and 60 earlier 200-year compensation/issuance scenarios. A DOM-contract smoke check exercises initial rendering, changed assumptions and invalid-input handling with no network dependency. Browser visual QA and external economic review have not been performed. Rust tests were not run; Rust and consensus code are unchanged.

Traceability: FD-01/02/03/06/13/16/17/18; P1/P2 voice/value and personhood limitations; D-0073/0074; `docs/design/inflation-and-whale-resistance.md`; `docs/design/treasury-economic-model.md`; `crates/mini-economy/src/issuance.rs` and `ledger.rs`; `docs/FAILURE_BOOK.md`; `docs/gates/economic-simulation-spec.md`; `docs/gates/dtn-design-constraints.md`; and the founder/council/compensation governance documents cited in the companion proposal.

This work is an explicitly requested review proposal, not an exercise of charter-derived approval authority. Repository main was read through the connected GitHub service at the pinned checkpoint; a trusted-launcher runtime activation attestation was not established, and hardened charter conformance is not claimed. No activation, canonicalization, secret, custody, release or owner-adoption action is performed. Human/maintainer review and the repository's applicable approval floor remain outstanding. No AI review counts as human approval.

AI assistance prepared the research arithmetic, offline interface, tests and documentation on 12 September 2026. The initiating user requested a review PR; responsibility acceptance for an approved exact state remains with the applicable human/governance process. Rollback is removal of this isolated research folder and the two proposal documents; no live state migration is involved.

The next bottleneck is an independently reviewed demand/basket/issuance mechanism that resolves the protected mint-floor conflict. A Windows Mininet client is a separate product task; this economics artifact is not that client.

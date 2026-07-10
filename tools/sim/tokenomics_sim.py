#!/usr/bin/env python3
"""
tokenomics_sim.py - deterministic adversarial sweep harness for the D-0073/
D-0074 economic parameters (roadmap #47/#50), per docs/gates/
economic-simulation-spec.md's own "what engineering should build before the
specialist arrives" section.

Known simplification, stated honestly rather than glossed over: this models
reward buckets as proportional-to-existing-holdings flows into actor classes
(whale/sybil/honest/insider), not D-0074's actual Human Share mechanic
(equal MINI per active verified human, independent of prior balance). That
makes this harness conservative for Human Share concentration specifically
(it can only ever show holdings-proportional flows getting *more* unequal,
never D-0074's flat-per-human counterweight reducing concentration) while
still being a real, useful adversarial stress test for the whale/sybil/
treasury dynamics it does model. An external mechanism-design reviewer
should treat this as a starting harness to extend, not a finished model of
D-0074 itself.

Usage:
  python3 tools/sim/tokenomics_sim.py
  python3 tools/sim/tokenomics_sim.py --out /tmp/results.csv
"""

from __future__ import annotations

import argparse
import csv
import itertools
import math
from dataclasses import dataclass, fields
from typing import List


@dataclass
class Scenario:
    name: str
    years: int
    initial_supply: float
    inflation_ceiling: float
    split_a: float  # Human-Share-like bucket
    split_b: float  # service/security-reward-like bucket
    split_c: float  # treasury-contribution-like bucket
    treasury_initial: float
    treasury_spend_rate: float
    whale_buy_rate: float
    sybil_count: int
    sybil_cost_per_epoch: float
    honest_growth_rate: float
    vesting_months: int
    governance_threshold: float


@dataclass
class Result:
    scenario: str
    final_supply: float
    annualized_inflation: float
    treasury_final: float
    treasury_runway_years: float
    whale_share: float
    sybil_share: float
    honest_share: float
    top_actor_share: float
    governance_captured: bool
    sybil_profitable: bool
    failed: bool
    failure_reason: str


def simulate(s: Scenario) -> Result:
    months = s.years * 12
    supply = s.initial_supply
    treasury = s.treasury_initial

    whale = 0.0
    sybils = 0.0
    honest = supply * 0.20
    insiders = supply * 0.20
    public_float = supply - honest - insiders

    locked_rewards: List[float] = []
    sybil_revenue = 0.0
    sybil_cost = 0.0

    for _ in range(months):
        monthly_issuance = supply * s.inflation_ceiling / 12.0
        supply += monthly_issuance

        bucket_a = monthly_issuance * s.split_a
        bucket_b = monthly_issuance * s.split_b
        bucket_c = monthly_issuance * s.split_c

        sybil_capture_rate = min(0.80, s.sybil_count / max(1.0, s.sybil_count + 1000.0))
        sybil_reward = bucket_a * sybil_capture_rate
        honest_reward = bucket_a - sybil_reward + bucket_b * 0.70
        treasury_reward = bucket_c + bucket_b * 0.30

        sybil_revenue += sybil_reward
        sybil_cost += s.sybil_count * s.sybil_cost_per_epoch

        if s.vesting_months > 0:
            locked_rewards.append(honest_reward)
            if len(locked_rewards) > s.vesting_months:
                honest += locked_rewards.pop(0)
        else:
            honest += honest_reward

        sybils += sybil_reward
        treasury += treasury_reward
        treasury -= treasury * s.treasury_spend_rate / 12.0

        whale_purchase = min(public_float, s.whale_buy_rate * supply / 12.0)
        whale += whale_purchase
        public_float -= whale_purchase

        honest_migration = public_float * s.honest_growth_rate / 12.0
        honest += honest_migration
        public_float -= honest_migration

    holders = [whale, sybils, honest, insiders, max(0.0, public_float), max(0.0, treasury)]
    top_actor_share = max(holders) / supply if supply else 0.0
    whale_share = whale / supply if supply else 0.0
    sybil_share = sybils / supply if supply else 0.0
    honest_share = honest / supply if supply else 0.0

    governance_captured = (
        whale_share >= s.governance_threshold
        or (whale_share + sybil_share) >= s.governance_threshold
    )
    sybil_profitable = sybil_revenue > sybil_cost
    treasury_runway_years = math.inf if s.treasury_spend_rate <= 0 else 1.0 / s.treasury_spend_rate

    reasons = []
    if governance_captured:
        reasons.append("governance_capture")
    if sybil_profitable and sybil_share > 0.05:
        reasons.append("sybil_profitable_material_share")
    if treasury <= 0:
        reasons.append("treasury_depleted")
    if top_actor_share > 0.50:
        reasons.append("excessive_concentration")

    return Result(
        scenario=s.name,
        final_supply=supply,
        annualized_inflation=(supply / s.initial_supply) ** (1.0 / s.years) - 1.0,
        treasury_final=treasury,
        treasury_runway_years=treasury_runway_years,
        whale_share=whale_share,
        sybil_share=sybil_share,
        honest_share=honest_share,
        top_actor_share=top_actor_share,
        governance_captured=governance_captured,
        sybil_profitable=sybil_profitable,
        failed=bool(reasons),
        failure_reason=";".join(reasons) if reasons else "pass",
    )


def build_scenarios() -> List[Scenario]:
    # D-0074's actual adopted split is 2%/0.75%/0.25% of a 3% ceiling
    # (i.e. a=0.667, b=0.25, c=0.083 of total issuance); the sweep below
    # brackets that point alongside deliberately worse splits, rather than
    # only ever testing the adopted parameters.
    splits = [(0.667, 0.25, 0.083), (0.50, 0.40, 0.10), (0.45, 0.45, 0.10), (0.60, 0.30, 0.10)]
    ceilings = [0.01, 0.02, 0.03, 0.05]
    sybil_counts = [0, 100, 1000, 10000]
    whale_rates = [0.0, 0.005, 0.02]
    vesting = [0, 12, 36]

    scenarios = []
    for (a, b, c), ceiling, sybils, whale, vest in itertools.product(
        splits, ceilings, sybil_counts, whale_rates, vesting
    ):
        scenarios.append(
            Scenario(
                name=f"split_{a:.3f}_{b:.3f}_{c:.3f}_ceil_{ceiling:.2f}_sybil_{sybils}_whale_{whale}_vest_{vest}",
                years=20,
                initial_supply=1_000_000_000.0,
                inflation_ceiling=ceiling,
                split_a=a,
                split_b=b,
                split_c=c,
                treasury_initial=100_000_000.0,
                treasury_spend_rate=0.10,
                whale_buy_rate=whale,
                sybil_count=sybils,
                sybil_cost_per_epoch=1.0,
                honest_growth_rate=0.02,
                vesting_months=vest,
                governance_threshold=0.33,
            )
        )
    return scenarios


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="tokenomics_results.csv")
    args = parser.parse_args()

    rows = [simulate(s) for s in build_scenarios()]
    field_names = [f.name for f in fields(Result)]
    with open(args.out, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=field_names)
        writer.writeheader()
        for row in rows:
            writer.writerow(row.__dict__)

    failed = sum(1 for r in rows if r.failed)
    print(f"scenarios={len(rows)} failed={failed} pass={len(rows) - failed}")
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()

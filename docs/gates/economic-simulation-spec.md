# Treasury economics & long-term attack modeling — simulation spec

Gates [roadmap #47](https://github.com/britak420/Mininet/issues/47)
(treasury economic model audit) and
[#50](https://github.com/britak420/Mininet/issues/50) (long-term inflation
& whale-attack modeling). **Founder action required: engage
mechanism-design/tokenomics specialists** — people who model adversarial
systems (the Gauntlet/Chaos-Labs style of work other protocols commission),
not token-launch marketing consultants.

## Why philosophy alone doesn't close this

The specs already preserve the right shape: voice/value separation (P1),
concave/diminishing reward curves so doubling storage doesn't double
reward, slow presence-conditioned vesting instead of lump sums (P4). None
of that guarantees the *calibration* is safe. Bad emission curves can
create whale concentration, dead liquidity, fee starvation, or perverse
storage incentives without violating a single frozen invariant — because
invariants constrain the *rules*, not the *numbers* plugged into them.

## What engineering should build before the specialist arrives

A deterministic simulation harness (`crates/mini-econ-sim`, not yet
built) so a mechanism-design reviewer has something to actually run
adversarial scenarios against, rather than reasoning from spec prose
alone:

```rust
pub enum Actor {
    HonestUser,
    Whale,
    StorageFarmer,
    SybilCluster,
    DormantHuman,
    Contributor,
    RelayOperator,
    SearchIndexer,
    EarlyAdopter,
    LateAdopter,
}
```

### Metrics the harness should report

- Gini concentration of holdings over simulated time
- storage-reward concentration (top-N farmers' share of total reward)
- whale accumulation rate under various strategies
- fee affordability for an ordinary user at projected usage levels
- treasury runway under various spend/inflow assumptions
- inactive/dormant supply over decades (Directive 13 timescale)
- Sybil extraction value vs. Sybil operating cost (the same question
  `docs/audits/issue-18-sybil-social-graph-review.md` left open at the
  identity layer — this is its economic-layer counterpart)
- attack ROI for each named attack pattern below
- validator-role concentration over time

### Invariant checks the simulation should assert, not just report

- Money never buys governance weight (P1) — true by construction in the
  code, but worth confirming no simulated *emergent* behavior (e.g.
  wealth buying influence through non-protocol channels like hiring the
  most developers) undermines the *intent* even where the letter holds.
- Doubling committed storage earns strictly less than double the reward
  (concavity holds under the actual emission curve, not just in the
  abstract policy description).
- Per-identity caps hold under a simulated Sybil farm at realistic scale.
- Sybil extraction cost exceeds expected extraction value across a range
  of assumed farming costs — this is the number the whitepaper's "no
  longer cheap" claim (flagged unproven in
  `docs/audits/issue-18-sybil-social-graph-review.md`) actually needs to
  become checkable.
- Late adopters still receive a meaningful share under realistic adoption
  curves (not just early adopters capturing everything).
- Treasury spending cannot, even indirectly, purchase governance
  influence.

## Questions the specialist must answer

- What emission curve and vesting schedule actually produce acceptable
  Gini concentration over a 10/50/100-year simulated horizon?
- At what farming cost does the Sybil-resistance economic argument
  (`SS11`'s whitepaper claim) actually hold, and how does that compare to
  realistic real-world attacker costs (compute, device costs, human
  labor for social-graph infiltration)?
- Is the current reward-curve shape (`mini-reward`) resistant to
  whale-driven storage centralization at realistic hardware-cost
  assumptions?
- What treasury runway does the current contribution-mechanism design
  (SPEC-07) actually project under conservative adoption assumptions, and
  what happens to protocol operation if that runway is shorter than
  expected?

## What closes this gate

A written report from the engaged specialist(s) covering the questions
above, run against the simulation harness once built, with concrete
parameter recommendations — recorded as a new D-number (or several, if
findings affect multiple frozen-but-uncalibrated invariants like P4).

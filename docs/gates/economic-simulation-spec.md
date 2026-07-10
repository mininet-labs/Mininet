# Treasury economics & long-term attack modeling — simulation spec

Gates [roadmap #47](../../issues/47)
(treasury economic model audit) and
[#50](../../issues/50) (long-term inflation
& whale-attack modeling). **Founder action required: engage
mechanism-design/tokenomics specialists** — people who model adversarial
systems (the Gauntlet/Chaos-Labs style of work other protocols commission),
not token-launch marketing consultants.

**Update (D-0073/D-0074, 2026-07-10):** the founder has since fixed the
design parameters this gate was originally scoped to help *invent* —
`docs/design/treasury-economic-model.md` (#47: the XRPL/XMR bridge split,
epoch/oracle/vesting/issuance-ceiling mechanism) and `docs/design/
inflation-and-whale-resistance.md` (#50: the 3%/2%/0.75%/0.25% issuance
envelope, 365-day vesting, and the enumerated anti-whale governance-input
wall) are now founder-set starting parameters, not open questions. This
gate's remaining job is **calibration and adversarial validation** of
those specific numbers — the simulation harness and stress-test/shock
matrices below are unchanged and still required before the parameters are
treated as safe, not superseded by the two decisions above.

## Why philosophy alone doesn't close this

The specs already preserve the right shape: voice/value separation (P1),
concave/diminishing reward curves so doubling storage doesn't double
reward, slow presence-conditioned vesting instead of lump sums (P4). None
of that guarantees the *calibration* is safe. Bad emission curves can
create whale concentration, dead liquidity, fee starvation, or perverse
storage incentives without violating a single frozen invariant — because
invariants constrain the *rules*, not the *numbers* plugged into them.

## What engineering should build before the specialist arrives

**Update (2026-07-10):** a first-pass harness now exists —
`tools/sim/tokenomics_sim.py`, a dependency-free Python sweep, run and
verified working in this session (576 scenarios, 130 pass / 446 fail
against the failure thresholds below). It is deliberately a starting
point for the eventual specialist to extend, not the `crates/
mini-econ-sim` Rust harness described below, and it has one honest,
load-bearing limitation stated in its own docstring: it models reward
flow as proportional to existing holdings across actor classes (whale/
sybil/honest/insider), not D-0074's actual Human Share mechanic (equal
MINI per active verified human, independent of prior balance). That
means it is *structurally incapable* of showing D-0074's real
flattening effect — running it at D-0074's actual adopted split
(0.667/0.25/0.083 of the 3% ceiling, i.e. the 2%/0.75%/0.25% channels)
still flags `excessive_concentration` even with zero sybils and zero
whale buying, purely because the harness can't represent Human Share's
per-human equality. **Do not read that as a finding that D-0074 fails
its own baseline** — it's a known harness gap, not a result. A real
`mini-econ-sim` (Rust, matching the `Actor` enum below) that actually
models per-human equal issuance is still the eventual target; the
Python harness closes the "nothing runnable exists yet" gap in the
meantime.

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

### Adversary classes the harness (Python or eventual Rust) should model

- **A1 — passive whale:** accumulates but doesn't attack governance;
  measures concentration/voting-power drift.
- **A2 — active governance attacker:** buys/earns/borrows enough MINI
  to pass monetary/governance changes.
- **A3 — sybil farmer:** many pseudo-humans/devices/households farming
  issuance and rewards.
- **A4 — treasury drainer:** optimizes proposals/grants/rewards to
  extract treasury funds without durable network value.
- **A5 — liquidity attacker:** thin-market or temporary price
  manipulation distorting governance or economic assumptions.
- **A6 — cartel attacker:** multiple semi-independent accounts
  coordinating while avoiding obvious common ownership.
- **A7 — honest long-term participant:** the control group — a
  builder/host/human node contributing real work, holding long-term.

### Parameter sweep (already run by `tools/sim/tokenomics_sim.py`)

| Parameter | Values swept |
|---|---|
| Split | D-0074's actual 0.667/0.25/0.083, plus 50/40/10, 45/45/10, 60/30/10 |
| Inflation ceiling | 1%, 2%, 3% (D-0074's adopted ceiling), 5% |
| Vesting period | 0, 12, 36 months |
| Whale purchase rate | none, low, medium |
| Sybil count | 0, 100, 1,000, 10,000 |

### Failure thresholds the harness checks per scenario

1. A single whale reaches decisive governance power within a plausible
   budget window.
2. Sybil rewards exceed sybil operating cost while sybils hold a
   material (>5%) share.
3. Treasury balance is depleted under the modeled spend rate.
4. Any single actor class exceeds 50% proportional share
   (`top_actor_share` — see the honest limitation above before reading
   too much into this one specifically).

A parameter set "passes" only if none of the above hold — 130/576 did
in this first run. The full CSV is reproducible (`python3 tools/sim/
tokenomics_sim.py`) but not itself committed, since it's a large
regenerable artifact, not source.

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

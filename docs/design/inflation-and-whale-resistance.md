# Long-term inflation and whale-attack resistance — founder decision (D-0074)

Resolves the design question `docs/gates/economic-simulation-spec.md` was
gating for [roadmap #50](../../issues/50): whether a large early holder's
position, combined with the emission schedule, could ever translate into
disproportionate influence — directly or indirectly (Directive 16). This
document records the founder's answer as a design spec; it is not a claim
that the model is simulated or implemented yet — see "What remains open."

## Position

Mininet does not attempt permanent economic equality by confiscating
wealth or preventing voluntary accumulation. It guarantees: equal
original economic citizenship, a perpetual equal Human Share stream,
predictable bounded inflation, continuing economic entry for future
generations, absolute separation between wealth and political authority,
and diminishing protocol-created advantage from hardware/capital. A
wealthy person may own more MINI. They may never receive more political
voice because of it.

## 1. Human Share as a perpetual right

The Human Share is not a salary, payment for voting, payment for work, an
investment return, charity, a founder grant, or a signup bonus — it is
the person's recurring share of the Mininet economy, established by
personhood and maintained by continued low-burden human presence.
Productive participation may earn additional network rewards (channel B
below) but cannot enlarge or replace the underlying human entitlement.

**No finite first-generation allocation.** "Genesis tranche" must not be
implemented as a finite bag of tokens controlled by founders or divided
mainly among early participants — the Human Share is an issuance *rule*,
not a treasury account, and cannot be exhausted by the first generation.
A person joining in 2126 enters the same current Human Share stream as
every other active verified human in 2126, with no multiplier, superior
rate, seniority, or ownership over future people's allocation for early
participants.

## 2. Annual issuance envelope — 3% ceiling, three channels

| Channel | Ceiling (annual, % of circulating supply) | Gate |
|---|---|---|
| Human Share | **2% floor**, protected first | personhood only, equal per active verified human |
| Service & security rewards | **0.75% max** | objectively verifiable network service (storage, routing, proofs, dev, audit, translation, moderation) |
| Treasury-contribution issuance | **0.25% max** | the epoch mechanism in D-0073 |
| **Total gross annual issuance** | **3% ceiling** | — |

Unused service/treasury capacity expires each period rather than
accumulating. No emergency, treasury deficit, founder request, validator
vote, bridge crisis, or budget decision may exceed the 3% ceiling without
a constitutional amendment. Transaction fees are transfers of existing
MINI, not new issuance, and do not count against the ceiling.

## 3. Human Share formula

For epoch *e*:

```
HumanPool_e = CirculatingSupply_e × 0.02 × (EpochDuration / Year)
HumanShare_{e,h} = HumanPool_e / EligibleHumans_e
```

Every eligible human receives the same amount for the same epoch. The
formula has no input for balance, wealth, age, nationality, employment,
hardware, storage, popularity, reputation, contribution history, voting
behavior, or time since Mininet's launch.

## 4. Presence, inactivity, accessibility

"Active" for Human Share purposes means continued evidence a distinct
human remains present — not casting votes, producing valuable work,
owning powerful hardware, staying constantly online, revealing raw
personal data, maintaining a public profile, or paying a fee. Presence
must be provable through multiple accessible pathways so no single
device, sensor, social class, document, or institution becomes
mandatory (Directive 11; see D-0075 for the evidence model itself).

A person gets a **12-month grace period** after their last accepted
presence event. After that: new accrual pauses, already-vested MINI
remains theirs, nothing is confiscated, reputation is undamaged,
governance rights restore when presence resumes. Reactivation resumes
future accrual — it does not create an unlimited retroactive claim for
missed years. Disability, illness, childhood, displacement, imprisonment,
censorship, device loss, and intermittent connectivity are accessibility
problems, not moral failures, and must not be penalized as such.

## 5. Slow vesting

Each epoch's Human Share accrual **vests linearly over 365 days**. Before
vesting it is non-transferable, non-saleable, non-collateralizable,
non-delegable, unavailable to creditors, and carries no governance power.
Vesting cannot be accelerated by wealth, payment, work, hardware, or
political participation. Once vested, MINI is ordinary private property.
After death or permanent liveness cessation: no new accrual, already
vested property remains transferable/inheritable, the unearned *future*
stream is not inherited or sold — a human right cannot become a tradable
claim over another person's future existence.

## 6. Long-term concentration effect (why this is deliberate)

At the full 3% ceiling, an early holder who does not keep acquiring
newly issued value retains approximately 47.8% of their original
proportional share after 25 years, 22.8% after 50 years, 5.2% after 100
years. At the protected 2% Human Share floor alone, ~13.8% remains after
100 years. Nominal MINI is never lost — the holder's *share of the
expanding economy* declines as future humans and active contributors
enter it. This is the deliberate mechanism that prevents a founding
generation from owning the same fraction of humanity's economy forever,
without confiscation, forced redistribution, or arbitrary balance
seizure.

## 7. No founder/investor/institution multiplier

No founder, investor, developer, exchange, bank, treasury signer, early
adopter, foundation, corporation, or government receives a privileged
emission schedule. Launch funding must use one of the ordinary channels
(voluntary purchase of circulating MINI, transparent service
compensation, bounded treasury contribution per D-0073, publicly
approved bounty) — no permanent founder percentage, investor reserve,
institutional governance class, or hidden allocation.

## 8. Formal anti-whale wall

MINI balance (direct, historical, delegated, locked, liquidity supplied,
treasury contributions, fees paid, economic rank, a balance-purchased
NFT/credential, or any wealth-derived proxy) must never be read by any
function determining: vote weight, proposal eligibility/ordering,
governance quorum, finality vote weight, finality-committee selection,
treasury-signer eligibility, constitutional-review authority, identity
confidence, personhood confidence, dispute/appeal rights, moderation
authority, governance-feed visibility, protocol-update approval, or
access to governance information. Renaming economic power does not make
it non-economic — this is P1/Directive 16 made explicit and enumerated at
the mechanism level, not just the crate-dependency level.

## 9. Vote-buying resistance

One-human-one-vote alone doesn't stop a wealthy actor from buying or
coercing other humans' votes. Long-term governance should therefore use
**secret, receipt-free ballots** where technically feasible — a voter
must be able to cast a valid vote without producing transferable proof
of how they voted. Votes should not be permanently delegable or
saleable; temporary accessibility assistance may exist but must not
create a market in persistent political delegation. The protocol cannot
prevent every off-network bribe, but it should prevent a briber from
reliably verifying compliance.

## 10. Paid attention vs. governance attention

Money may purchase clearly labeled commercial/social amplification. It
may not purchase placement inside constitutional or governance
processes. All verified humans get an equal minimum opportunity to
discover active proposals, voting deadlines, constitutional changes,
treasury spending proposals, security warnings, and protocol release
decisions — governance feeds may be chronological, randomly rotated, or
user-filtered, but never ordered by payment, holdings, contribution
size, or sponsorship.

## 11. Storage/infrastructure concentration

Owning hardware may earn value but must not create unbounded consensus
control: proof of real independent replication before storage gets
consensus relevance (mini-porep, D-0064), concave reward curves,
per-human eligible-capacity ceilings, diminishing returns for additional
hardware under one human root, geographic/network-diversity bonuses, no
balance-weighted or storage-weighted validators, human-sampled equal
finality participation. A warehouse may earn more total service value
than one phone by performing more service — it gains no proportionate
political or finality authority for it.

## 12. Contribution/reputation rank

Economic contribution may produce a factual service record or skill
reputation. It may not unlock exclusive rights to propose, vote, review
constitutional changes, appeal, become a treasury signer or finality
voter, access hidden governance discussions, bypass ordinary review, or
receive guaranteed reach. Skill-based roles may require demonstrated
competence, but a new qualified human must always have an open path to
demonstrate it without buying a credential or needing sponsorship from
an existing elite. No reputation score may become hereditary,
purchasable, or convertible into extra votes.

## 13. What Mininet deliberately does not attempt

No confiscation of large balances, no maximum personal fortune, no
attempt to make everyone's eventual wealth identical, no prevention of
voluntary trade, no punishing saving, no reversing lawful transfers
because concentration increased, no exposing private balances to run
political wealth tests, no promise that markets stay equal. The
guarantee is equal political standing and an equal continuing
human-economic foundation — limiting privilege the *protocol itself*
creates, not total control of human economic behavior.

## What remains open

`docs/gates/economic-simulation-spec.md` still gates: building the
simulation harness and running the 200-year suite below, and external
mechanism-design review of these founder-set starting parameters (the
2%/0.75%/0.25%/3% split and the 365-day vesting window are decisions to
be calibrated against simulation, not values already validated by one).
#50 stays open, retitled, tracking exactly that remaining work.

### Required 200-year simulation suite (not yet built or run)

- **Population paths:** 1B / 10B / 20B participants; declining
  population; rapid/stalled adoption; intermittent participation.
- **Initial whale positions:** 1% / 10% / 30% / 60% / 90% of circulating
  supply.
- **Behavior:** dormant whale; active service-providing whale; whale
  buying all newly sold Human Share; coordinated treasury contributions;
  mass liquidity withdrawal; storage consolidation; prolonged low/high
  transaction volume.
- **Shocks:** 80% XRP/XMR drawdown; bridge shutdown; exchange
  delistings; long low-fee periods; rapid population growth/decline;
  oracle manipulation; theft of one treasury vault cell.

**Pass conditions:** total gross annual issuance never exceeds 3%; Human
Share receives its protected 2% before optional channels; every eligible
human in an epoch receives the identical Human Share amount; a later
human has no inferior emission formula for their birth date; token
balance never changes formal governance authority; storage ownership
never changes governance vote weight; a whale cannot control finality by
holding MINI; treasury contributions cannot dominate total issuance;
dormant early wealth loses proportional dominance over century-scale
periods; security rewards stay sufficient under plausible low-fee
scenarios; no mechanism requires revealing raw personal wealth; no
emergency action can confiscate balances or rewrite ownership; no
institution becomes indispensable to the Human Share; the weakest honest
participant remains able to establish presence and receive their share.

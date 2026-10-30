# Founding Parliament proposal scope

**PR purpose:** define and pre-code Mininet's proposed transition from founder-guarded bootstrap to a rolling, active-duty Parliament that can expand into public human governance without replacing the institutional machinery.

## In scope

This PR adds:

- the proposed Founding Parliament constitution;
- bounded H0 Guardian Stay rules;
- active-duty compensation rules;
- evidence-gated seat growth and public transition rules;
- a non-authorizing Rust reference policy kernel compiled by tests;
- Forge-native schemas for invitations, seat terms, motions, votes, emergency orders, duty receipts, H0 stays, and authority transitions.

The policy kernel intentionally encodes:

- one active seat = one vote;
- H0 ordinary vote = one vote;
- H0 invitations: 100 per rolling 30 days;
- qualified Steward invitations: 10 per rolling 60 days after 90 days service and one completed duty period;
- invitation -> candidacy, never invitation -> vote;
- founding seat series `7 -> 15 -> 31 -> 63 -> ...` using `2n + 1`;
- 90-day rolling seat terms as the proposed default;
- immediate P0/P1 defensive actions capped at 72 hours without plenary ratification;
- ordinary, major, emergency-fix, constitutional, public-transition, and Guardian-Stay-override thresholds;
- duty payment based on activity evidence rather than vote direction;
- H0 Guardian Stay capped at 14 days, one use per exact target, and unable to block public transition, its own sunset, an override, owner adoption, or lawful H0 seat removal;
- monotonic public eligibility;
- one-way destruction of exceptional H0 authority at public maturity.

## Explicitly not activated by this PR

This PR MUST NOT be read as activating the Parliament.

It does not:

- supersede `docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md` merely by merge;
- create persistent Pre-Go-Live identities from GitHub accounts or Beta contribution history;
- make a GitHub username a Steward identity;
- make Forge canonical;
- pay real MINI;
- claim current `did:mini` roots prove unique humans;
- enable H0 Guardian Stay in production;
- give an emergency committee power to modify balances, force updates, censor owners, or rewrite settlement;
- choose the final public election/sortition formula before personhood and governance simulation mature.

Activation requires a later exact-state bootstrap decision plus production wiring that truthfully updates the canonical transition state.

## Why code now

Governance prose without executable invariants is vulnerable to drift. The Rust reference kernel is deliberately compiled only through `mini-forge` integration tests in this PR. That gives engineers deterministic threshold/transition tests while avoiding an accidental authorization path before the proposal is adopted.

The next implementation step after acceptance is to integrate these policy types into canonical Forge objects and state resolution, replacing boolean transition evidence with signed evidence references and independently reproducible checks.

## Primary red flags

This proposal should be rejected or revised if implementation creates any of these paths:

1. invitation directly creates a parliamentary vote;
2. inviter ancestry gives permanent access or vote weight;
3. H0 gets more than one ordinary vote;
4. H0 can indefinitely veto public transition or restore exceptional authority after sunset;
5. duty pay depends on voting YES/NO;
6. inactive officeholders can collect duty pay by title alone;
7. emergency committee action can seize balances or force owner adoption;
8. seat growth occurs by time/popularity without capture/recovery/Forge evidence;
9. public access can later be rolled back;
10. early contributor rewards become political weight.

## Merge/readiness gates

Before this PR is merge-ready:

- all Rust workspace checks must pass on the exact head;
- the parliament policy integration tests must pass;
- Forge schema JSON must parse and satisfy repository schema checks if present;
- governance review must confirm the proposal is non-activating and does not silently contradict the currently active Pre-Go-Live state;
- any review finding about a new central control path must be resolved or explicitly rejected with evidence.

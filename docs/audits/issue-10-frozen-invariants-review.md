# Frozen invariants review

Tracks [roadmap issue #10](https://github.com/britak420/Mininet/issues/10)
(Phase 0.3). Scope, per the issue: apply four adversarial questions to
every Tier-F row in `docs/INVARIANTS.md` — not "is it currently
enforced" (that's [issue #8](https://github.com/britak420/Mininet/issues/8)'s
PASS/PARTIAL/FAIL matrix) but "even if today's code is correct, is there
any path — direct, indirect, or contingent on something not yet built —
by which this invariant erodes." A "maybe" answer gets a concrete attack
scenario, not just a flag.

## The four questions

1. Can this ever produce institutional control?
2. Can money indirectly buy governance?
3. Can humans become second-class (relative to another human, an
   institution, or an AI)?
4. Can updates remove freedom?

## Method

Rather than repeat "No — [reason]" 26×4 times for rows that share the
same answer pattern, invariants are grouped by what would actually have
to break for a "maybe" to become real. Every group states its answer to
all four questions; every **maybe** gets a named attack scenario. This is
deliberately the more skeptical companion to #8 — where #8 asked "is it
enforced," this asks "assume it's enforced today, what could still go
wrong."

## Group 1: Identity & devices (`did-mini`, self-certifying identifiers, delegation, pre-rotation)

1. Institutional control? **No.** No registry, no issuing authority — an
   identity is self-certifying by construction (SCID re-derivation).
   Nothing here requires trusting an institution, today or in any
   foreseeable extension.
2. Money buys governance? **No, directly.** Creating a `did:mini` costs
   nothing but local compute. See Group 3 for the indirect path.
3. Second-class humans? **No** — capability scoping is symmetric per
   identity root; nothing differentiates roots by wealth, geography, or
   device count.
4. Updates remove freedom? **No** — identity continuity (rotation,
   recovery) is entirely local; no update can retroactively invalidate an
   existing identity's self-certifying history.

## Group 2: Personhood & Sybil resistance (`mini-uniqueness`, presence, vouching graph) — **the sharpest "maybe" in this review**

1. Institutional control? **No**, by design (Directive 8) — the trust
   list is the user's own attestation graph, not a third party's.
2. Money buys governance? **Maybe — this is the real finding.**
   **Attack scenario:** an attacker with capital hires labor (or
   automates via bots operating real or coerced devices) to create many
   verified identity roots, each independently satisfying whatever
   personhood bar exists (physical presence, social vouching, time-aged
   promotion). Each resulting root gets one full, legitimate vote under
   P1/P2 — the attacker never touches the ValidatorSet's weight field or
   any balance-to-vote mapping directly, but has *indirectly* bought N
   votes by buying N verified-looking humans. **This is not a bug in any
   code that exists today** — it's the fundamental Sybil-cost question
   the whitepaper itself frames as the central defense ("by the time a
   fake operation is profitable it is nearly indistinguishable from
   genuine adoption," §11). Whether this "maybe" is actually closed
   depends entirely on whether D-0038's multi-signal accumulator makes
   farming costlier than the value of the votes/rewards it produces at
   real-world scale — precisely the open question [issue #18](https://github.com/britak420/Mininet/issues/18)
   exists to answer, not something this review can resolve by inspection
   alone.
3. Second-class humans? **No** structurally, but the same Sybil vector
   above, if unresolved, makes *genuine* humans relatively second-class
   to whoever can afford to mass-produce fake ones — a second framing of
   the same core risk.
4. Updates remove freedom? **No** direct path found.

## Group 3: Value & reward (`mini-value`, `mini-treasury`, `mini-reward`)

1. Institutional control? **No** for the cryptography itself (D-0036/
   D-0037/D-0040/D-0041 are all founder-reviewed, non-custodial designs);
   treasury custody specifically is the whitepaper's own named
   "permanent honeypot" risk class, which is why FROST threshold signing
   (not a single key) is the chosen design.
2. Money buys governance? **No, directly** — P1 holds structurally
   (no weight field). **Indirectly:** the same Sybil-farming path in
   Group 2 also lets capital indirectly amplify *reward accrual*
   (parallelizing across many farmed identities), which isn't governance
   capture but is an adjacent form of the same underlying vulnerability —
   worth tracking under the same umbrella as [#18](https://github.com/britak420/Mininet/issues/18)
   rather than as a separate issue.
3. Second-class humans? **No** structural differentiation; see the Sybil
   caveat above for the same indirect framing.
4. Updates remove freedom? **No** direct path — vesting/accrual state is
   local and rate-capped, not remotely revocable.

## Group 4: Consensus & chain (`mini-chain`, validator sets, finality)

1. Institutional control? **No** in the code that exists (equal weight
   per identity root, by construction). **Contingent maybe:** the
   networked consensus protocol itself (proposer rotation, gossip, view
   change) is not built yet ([#36](https://github.com/britak420/Mininet/issues/36)-[#45](https://github.com/britak420/Mininet/issues/45)).
   A poorly-designed proposer-selection mechanism (e.g. one subtly
   favoring high-uptime, well-resourced nodes even without an explicit
   weight field) could reintroduce institutional-style influence through
   *availability* rather than *balance* — worth an explicit design
   constraint when Phase 5 lands, not just a P1 balance check.
2. Money buys governance? **No** directly; see #1's availability-bias
   caveat as the indirect path worth watching.
3. Second-class humans? **No** today; same availability-bias caveat.
4. Updates remove freedom? **No** direct path in finality-verification
   code; the release-registry chain (Group 5) is the actual mechanism by
   which an update reaches a device at all.

## Group 5: Release, update & bootstrap (`mini-forge`, `mini-update`, `mini-bootstrap`)

1. Institutional control? **No** in `mini-update::AdoptionState` itself
   (local, always-re-verifies, refusal is first-class). **Contingent
   maybe:** the release registry (on-chain, still `pending`) is exactly
   where institutional control could be smuggled in if its quorum/
   timelock rules were ever weakened "temporarily" for an emergency —
   see [#53](https://github.com/britak420/Mininet/issues/53)'s explicit
   scope to review this before it's built, not after.
2. Money buys governance? **No** direct path found in this group.
3. Second-class humans? **No** direct path found.
4. Updates remove freedom? **Maybe, contingent on unbuilt code.**
   **Attack scenario:** once the release registry exists, a captured or
   rushed release ships a "security update" that quietly narrows what
   actions are permitted (an early, subtle version of an off-switch).
   Nothing in the *currently shipped* code enables this — `AdoptionState`
   already refuses to trust a stale decision — but nothing in the
   currently shipped code *prevents* a badly-designed registry from
   enabling it either, since the registry doesn't exist yet. This is the
   single clearest example in this review of "PARTIAL today, becomes
   either PASS or a real violation depending entirely on how Phase 9
   ([#65](https://github.com/britak420/Mininet/issues/65)-[#70](https://github.com/britak420/Mininet/issues/70))
   is built."

## Group 6: Storage & seeding (`mini-store`, `mini-storage`, cache tiers)

1. Institutional control? **No** — seed-on-view is user-controlled and
   policy-bound; no party can compel replication.
2. Money buys governance? **No** direct path; storage reward is value,
   not voice, by explicit design (P1 + D-0033).
3. Second-class humans? **No** direct path — the egalitarian "thousand
   cheap machines" thesis is explicitly what Phase 4's replication-
   uniqueness work ([#31](https://github.com/britak420/Mininet/issues/31))
   exists to keep true rather than letting well-resourced warehouses
   quietly dominate storage reward.
4. Updates remove freedom? **No** direct path found.

## Group 7: AI's role (governance recommendations, code authorship, moderation)

1. Institutional control? **Maybe, if boundaries erode gradually.**
   **Attack scenario:** not a single dramatic violation, but a slow drift
   — an AI recommendation system that starts as "suggestions a human
   reviews" gradually becomes "the thing most humans just accept without
   reading," which is institutional control in practice even though
   Directive 12 is never technically violated on paper. This is exactly
   why [#56](https://github.com/britak420/Mininet/issues/56) and
   [#83](https://github.com/britak420/Mininet/issues/83) frame this as an
   ongoing enforcement review, not a one-time check.
2. Money buys governance? **No** direct path specific to AI.
3. Second-class humans? **Maybe**, same gradual-drift framing as #1 —
   humans who don't have access to (or trust in) AI-assisted tooling
   could become relatively disadvantaged participants even without any
   rule change.
4. Updates remove freedom? **No** direct path specific to AI beyond
   Group 5's general concern.

## Top findings, ranked

1. **Sybil-cost economics (Group 2/3) is the single highest-leverage
   "maybe" in this entire review** — it's the indirect path by which
   money could buy both governance and value, and it's already tracked as
   the roadmap's own top priority ([#18](https://github.com/britak420/Mininet/issues/18),
   [#20](https://github.com/britak420/Mininet/issues/20)). This review's
   contribution is naming it explicitly as a P1/P2-adjacent risk, not
   just a personhood-design question.
2. **The release registry (Group 5) is the clearest "freedom-removing
   update" risk**, entirely because it doesn't exist yet — get its
   quorum/timelock design right *before* building it, not after.
3. **Availability bias in consensus (Group 4)** and **gradual AI-authority
   drift (Group 7)** are both "no rule is technically broken, but the
   practical effect could still centralize" risks — the kind Directive 10
   warns compound silently. Worth a standing review cadence, not a
   one-time check, once the relevant subsystems ship.

No invariant in this review was found to already permit institutional
control, a money-to-governance path, second-class humans, or freedom-
removing updates **today**. Every "maybe" identified is contingent on
either a Sybil-cost assumption not yet proven at scale, or code that
doesn't exist yet — which is the expected, honest state of a project at
this stage, not a hidden defect.

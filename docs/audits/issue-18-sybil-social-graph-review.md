# Issue #18 — Sybil / social-graph attack review

**Scope:** the vouching-graph signal (`mini-uniqueness::graph`) and the
multi-signal promotion policy (`mini-uniqueness::status`) against the issue's
farming patterns: dense fake communities, purchased friendships, long-term
Sybils, sleeping Sybils, nation-state Sybils. This is **the whitepaper's
central Sybil-resistance claim** (§11) and the roadmap's most-flagged open
question (#10 audit, `INVARIANTS.md` hard limitation, `THREAT_MODEL.md` §2).

**Reviewer:** AI-drafted under D-0037. One real hardening bug was found and
fixed in this batch (see F1); the rest is an honest assessment of what the
current design does and does not achieve. Human review required.

**Verdict: the SybilRank-style trust propagation is sound and correctly
discounts inbred clusters, and a real design bug that let a farm bypass the
honest graph entirely has been fixed. But the core claim — that farming
becomes "no longer cheap" — remains UNPROVEN at production parameters and
still rests on the unresolved identity-root-≠-human limitation. This closes
the *review*; it does not close the *threat*.**

## What is genuinely sound

- **Mutual-only edges.** An edge exists only when *both* devices signed one
  mutual vouch transcript (`graph.rs` + `vouch.rs`); there is no wire shape
  for a one-sided "I vouch for you" claim. A farm cannot fabricate edges
  *from* real humans who never participated. ✅
- **Trust propagation, not edge counting.** `trust_scores` propagates mass
  outward from the seed cohort for a bounded number of rounds; a node's
  score reflects proximity *to the trusted region*, not total edge count.
  The test `a_sybil_cluster_with_one_bridge_edge_scores_far_below_the_honest_region`
  confirms a dense 10-node Sybil cluster with one bridge edge scores <¼ of
  the honest region. This correctly defeats **dense fake communities**: an
  arbitrarily large inbred cluster gains almost nothing from internal edges. ✅
- **Integer-only, reproducible.** Truncating integer division only ever
  under-counts trust, never fabricates it — deterministic across devices. ✅
- **Multi-signal promotion.** `FullHuman` requires fused score AND a minimum
  age AND diversity of live sources — no single strong signal promotes, and
  a fresh identity cannot buy in quickly regardless of stacked signals
  (tests cover age gate and single-source ceiling). ✅

## Finding F1 (fixed this batch) — Farm-saturation bypass of the honest graph

**The bug:** `HumanRecord::status` required N *distinct live sources* and a
fused-score threshold, but did **not** require any *specific* source. The
fused score's denominator only sums sources that have evidence
(`score()` divides by `total_weight` of present sources). So a farm could
reach `FullHuman` with:

- `PhysicalPresence` strength 100 — **self-attestable**: issue #17 shows a
  presence attestation is forgeable end-to-end between two devices the
  attacker controls, and
- one `External(_)` method it runs or pays for,

hitting score 100, two live sources, and any age — **every check the policy
made — with zero edges into the honest vouching graph.** The one signal a
farm structurally *cannot* fake (vouching-graph trust only propagates from
the seed cohort) was optional.

**The fix:** `PromotionPolicy::full_required_sources` — sources that must be
*live* for `FullHuman`, defaulting to `[VouchingGraph]`. The seed-anchored
signal is now mandatory, not substitutable. Tests:
`a_farm_cannot_reach_full_human_without_the_seed_anchored_vouch_signal`
(asserts the attack saturates score/age/diversity yet is still denied, and
that genuine vouch evidence then promotes) and
`a_fully_decayed_vouch_signal_does_not_satisfy_the_required_gate` (the
anchor must be live, not merely historical). This is a real tightening of
the Sybil boundary, not a cosmetic change.

## What remains UNRESOLVED (the honest core)

### Purchased friendships — partially, inherently limited

A real, seed-connected human selling genuine vouches transfers real trust to
a Sybil — propagation cannot distinguish a sold-but-real edge from an honest
one. SybilRank bounds the *blast radius* (one seller's mass is finite and
splits across everyone they vouch for, diluting as they sell more), but does
not prevent it. **This is a fundamental limit of any trust-graph scheme** and
is why the whitepaper never relies on the graph alone. Recorded, not solved.

### Long-term & sleeping Sybils — the age gate is a speed bump, not a wall

The minimum-age gate raises the *time* cost of a `FullHuman` farm but not the
ultimate *feasibility*: a nation-state adversary who ages identities and
slowly earns genuine seed-anchored vouches (via a few co-opted real humans)
defeats age + diversity + the F1 gate too, given enough patience and real
social infiltration. The defense degrades exactly against the best-resourced
attacker — as `THREAT_MODEL.md` §2 (Sybil) already marks "explicitly
unresolved."

### Nation-state Sybils — not defended, and honestly cannot be by this layer alone

Effectively unlimited resources + patience + real humans defeats a pure
social-graph + behavioral-signal scheme. This is why the whitepaper's own
framing is "no longer *cheap*," not "impossible," and why the
identity-root-≠-verified-human limitation at the top of `INVARIANTS.md`
remains the governing caveat: **nothing in this crate may be read as
enforcing one-human-one-vote (P2).**

## The unproven claim, stated plainly

The whitepaper (§11) claims farming becomes uneconomic — "by the time a fake
operation is profitable it is nearly indistinguishable from genuine
adoption." This review finds the *mechanisms* consistent with that claim and
now free of the F1 bypass, but the claim itself depends on **production
parameters that do not exist yet**: the seed set's composition and dilution
policy (whitepaper SS12), a calibrated acceptance threshold, iteration count
for real network size, and trust/decay weights are all deliberately
caller-supplied, not fixed. **Whether the cost is actually high enough is an
empirical question answerable only with those parameters and simulation at
scale — roadmap #11 (governance/Sybil simulation) and #21 (uniqueness proof
research).** Until then this is a sound skeleton with the sharpest gap in the
project still open.

## Recommendations filed

1. Seed-set governance + dilution policy — the single biggest missing piece
   (owner: #11 / whitepaper SS12).
2. Large-scale simulation to calibrate threshold/iterations/weights and
   actually test the "no longer cheap" claim (owner: #11).
3. Weight self-attestable signals (presence without hardware ranging, per
   #17) below seed-anchored ones in `TrustWeights` defaults.
4. Keep `full_required_sources` = `[VouchingGraph]` as the default; document
   that emptying it reopens F1.

## Traceability

Directive 8/15 → invariant P2 (target: one human one vote) + its hard
limitation (`docs/INVARIANTS.md` §2 top) → SPEC-02 / whitepaper §11 →
`mini-uniqueness::status` + `::graph` → `crates/mini-uniqueness/src/status.rs`
tests + `docs/THREAT_MODEL.md` §2 (Sybil, "explicitly unresolved").

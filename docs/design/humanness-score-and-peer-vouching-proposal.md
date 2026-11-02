# Humanness score, peer vouching, and audited thresholds — founder proposal mapped to the tree

**Status:** option 1 chosen by the founder in chat on 2026-11-02 (published-policy threshold; physical-presence signals combined with reciprocal peer vouching); first primitives implemented in `mini-uniqueness::policy`. Still not a Decision-Log entry. Not a Decision-Log entry, not
implemented, no frozen row weakened. Written 2026-11-02 in response to a
founder chat proposal; chat direction is task authorization, not a canonical
Decision. It builds on, and does not replace,
`frontier-personhood-governance-and-consensus-proposals.md` (§1, §3) and
`credential-taxonomy.md`.

## The proposal (founder, paraphrased)

1. Fresh-claim privacy: per-claim blinded handles (option 2(b)).
2. Trusted verified humans vouch for each other, raising a combined
   uniqueness + interpersonal-verification "humanness" score.
3. Connecting banks / other apps / services to be controlled from Mininet,
   with their unique use also raising the score.
4. Nobody is excluded automatically; voting and monthly payments are
   audited, gated on a minimum score (0–100) voted by the top 50% of
   accounts by score.
5. The score comes from a complex, adaptive formula that can use anonymous
   signals without revealing their source data.

## What already exists (do not rebuild)

| Proposal item | Already in tree |
|---|---|
| Peer vouching | `mini-uniqueness::{vouch, graph, verify}`: mutual signed vouches, seed-anchored trust propagation (D-0035). |
| Multi-signal 0–100 score | `mini-uniqueness::status`: `HumanRecord::score` (trust-weighted, decay-adjusted, 0..=100), `PromotionPolicy` with required sources, minimum age, distinct-source count (D-0038). |
| Nobody auto-excluded | Statuses only *name evidence*; `Unverified` is a state, not a ban. |
| Anonymous signals, only a derived score recorded | `SignalEvidence` stores strength + time only (P5). Aggregate-proof architecture: frontier doc §1.3 C, §2. |
| Unlinkable claims | Frontier doc §3 (`mini-attest` tiers 0–2); nullifier design §1.3 E. |

## Where the proposal collides with frozen rules (needs a decision, not code)

**A. Linking banks / apps as a score signal — blocked.** INV-18-02 (no edge
crate reaches governance or humanness signals), INV-18-03 (humanness signals
are a closed set; no provider-derived variant) and frontier doc §2.4 ("no
provider-derived or paid engagement predicate may feed humanness") forbid
it. Reasons are also practical: a bank/app link is purchasable, delegable,
and ties a pseudonymous root to a legal identity (P5, Directive 9). Weakening
INV-18-03 needs the constitutional amendment/unfreezing process; ambiguity
denies. *Allowed instead:* controlling external apps from Mininet as an
**edge convenience** (Directive 18) that never touches the score.

**B. "Top 50% by score vote the threshold" — entrenchment risk.** It makes
voting power over the gate depend on the score the gate defines (a
self-reinforcing ruling class), and P2 requires one equal vote per verified
human. Also, the score must stay a human-evidence axis, never a balance
(P1 voice/value wall): a shared score may gate both voting and payments only
if neither value nor governance crates gain a dependency edge on each other.

**C. "Complex adaptive formula" vs. simplicity (Directive 14) and the
no-novel-crypto rule.** Complexity is attack surface and unauditable by
members. Combining anonymous signals into a score without revealing inputs
is an aggregate-proof problem (frontier §1.3 C): a proof of a *published*
policy, not a secret adaptive one. Adaptivity should be limited to
governance-voted parameter changes with public history.

**D. Blinded handles vs. uniqueness.** Per-claim unlinkability (2(b)) is
compatible with one-claim-per-human only via a nullifier/credential scheme
(frontier §1.3 E, §3). Choosing 2(b) therefore *commits* us to that route;
it is not a standalone change and needs an external cryptographic review
before any real value (D-0037/D-0047).

## Options for the gate (decision requested)

1. **Published-policy threshold, equal-vote adjustment (recommended).**
   Score is computed from a public policy over a closed signal set. The
   minimum is a Tier-T parameter changed by one-human-one-vote with timelock
   and bounds; audited actions (vote, monthly payment) require score ≥ the
   minimum *or* a human appeal path. Nobody is excluded from the network.
2. **Founder-set floor + elected audit panel.** Threshold fixed in the
   charter for the first epoch; a randomly selected panel (sortition among
   `EvidenceQualifiedHuman`) audits borderline cases.
3. **Top-50% voting as proposed.** Only with safeguards: bounded movement
   per epoch, a floor/ceiling frozen in the charter, and recorded as an
   explicit entrenchment risk in `THREAT_MODEL.md`.

## What can be built now without touching a frozen row

- `mini-uniqueness`: a pure `AuditGate { minimum: u8 }` evaluating a
  `HumanRecord` score → `Pass | NeedsAudit` (never `Excluded`), with tests
  that no path yields exclusion from the network.
- Vouch-for-vouch reciprocity limits and per-epoch vouch quotas (collusion
  cost), in the existing graph module.
- A docs-only decision record once the owner chooses an option above.

Nothing above links `mini-value`, `mini-bounty`, `mini-treasury`,
`mini-forge`, or chain voting to each other.

## Honest limits

Sybil resistance remains unsolved (INVARIANTS hard limitation). No
mechanism here proves one human; everything counts identity roots until
SPEC-02 lands. Say "identity root", not "one human, one vote".

## Implemented (first primitives, `mini-uniqueness`)

- `PersonhoodPolicy`: versioned, content-addressed (BLAKE3 over canonical bytes) published policy; closed signal set (physical presence + seed-anchored vouching; `External`/behavioral weigh zero); `score(record, now)` 0..=100; validation refuses zero weights (a farm could otherwise saturate the score).
- `AuditGate` / `GateOutcome::{Pass, NeedsAudit, Insufficient}`: **audit only for scores just below the minimum** (inside `audit_band`, founder 2026-11-02: the score should be confident enough that clearly-low accounts need no peer time). Below the band an identity is simply not yet eligible for gated actions (no audit, re-evaluated each epoch); nobody is excluded from the network.
- `bounded_threshold` + `ThresholdBounds`: floor, ceiling and per-change step for the minimum; the one-human-one-vote and timelock live in governance.
- `VouchGraph::add_verdict_capped`: degree cap (collusion cost). Vouches are mutual by construction, i.e. reciprocal.

- `audit` module (founder decision 2026-11-02: peer review of anonymous activity + panel vote, stricter after epoch 1000): `PolicySchedule` (early/mature stages; `new` refuses any mature stage that is looser), `select_panel` (public reproducible sortition excluding the subject and the subject's vouchers), `ActivitySummary` (bucketed aggregates only), `AuditCase` (opaque handle, one vote per panelist, strict-majority clear, denial/expiry), `EligibilityLedger` (a settled audit counts only in its own epoch; a passing score is never blocked; no outcome removes anyone from the network).
- `governor` module: `ThresholdGovernor` — one equal vote per gate-eligible identity root, timelock, participation floor, strict majority, frozen `ThresholdBounds`.
- `VouchQuota`: per-root per-epoch vouch cap (stage-dependent; stricter after epoch 1000).

Starting schedule: early panel 7 / 4 confirmations, minimum 30, 10 vouches per epoch, degree cap 150; mature (epoch >= 1000) panel 11 / 8, minimum 45, 5 vouches per epoch, degree cap 100. These are governance parameters, not frozen law.

## Not built (and why)

- **Wiring into voting and payments.** No "monthly payment" feature exists in the tree yet (`mini-airdrop` only snapshots eligibility). Putting the gate into `mini-forge` quorum counting changes frozen-invariant behavior (P1, P2) and needs its own design and decision; the pure types above are what that wiring will call.
- Reviewer vote authentication/transport, and sourcing the sortition seed from finalized chain state.
- The blinded-handle/nullifier route (2(b)) and its external cryptographic review; bank/app linking stays blocked (INV-18-02/03).
- Honest limits unchanged: identity roots, not proven humans.

## Audit band (founder refinement, 2026-11-02)

Peer audit triggers only for accounts whose score falls in `[minimum - audit_band, minimum)` that epoch. Early stage: minimum 30, band 10 (audit 20-29). Mature (epoch >= 1000): minimum 45, band 5 (audit 40-44) — a narrower window is stricter, and the schedule refuses a mature band wider than the early one. `AuditCase::open` rejects any score outside the band, so reviewers cannot be spammed by cheap low-score accounts. The consequence is that the score itself must be hard to inflate (closed signal set, seed-anchored vouching required, degree and per-epoch vouch caps, presence evidence); that is the real Sybil defence and is still unsolved, so the band must not be widened to compensate.

## Adversarial simulation findings (graph signal only)

`crates/mini-uniqueness/tests/adversarial_sim.rs` (deterministic; 300 honest roots, 10 seeds, 8 vouches each; 400 densely connected sybils; each colluding mature credential spends the early-stage quota of 10 vouches on the cluster; "pass" = trust at least half the honest median):

| colluding mature credentials | attack edges | sybils passing | best sybil vs honest median |
|---|---|---|---|
| 0 | 0 | 0 / 400 | 0% |
| 1 | 10 | 0 / 400 | 3% |
| 6 | 60 | 0 / 400 | 10% |
| 24 | 240 | 0 / 400 | 29% |
| 60 | 600 | 383 / 400 | 77% |
| 100 | 1000 | 400 / 400 | 147% |
| 200 | 2000 | 400 / 400 | 297% |

Reading: collected trust scales with attack edges (colluders x quota), so the quota and degree cap are the real levers, and one, six or even 24 colluders do not lift a cluster — but there is a cliff once attack edges approach the cluster's size (here 600 edges for 400 sybils, with colluders at 20% of a small honest network). The graph signal alone therefore does not stop a large collusion; presence evidence, minimum age, distinct live sources, the audit band, and the economics must carry that case. Not modelled yet: paid real-human witnesses, forged devices, active proximity relays, stolen credentials, larger honest networks, and the second and third signals.

### Price of a cluster (extended simulation)

Same model, now asking how many colluding mature credentials an attacker must buy, bribe or steal before at least a tenth of a 400-identity cluster passes the graph signal (paid real-human witnesses and stolen credentials are the same thing to this signal; they differ in what they cost the attacker):

| setting | colluders needed |
|---|---|
| 300 honest roots, 10 vouches per credential (early stage) | 48 |
| 300 honest roots, 5 vouches per credential (mature stage, epoch >= 1000) | 76 |
| 600 honest roots, 10 vouches per credential | 56 |

The stricter mature quota raises the price by about 60% and a doubled honest network by about 17%, so the per-epoch quota is a stronger lever than network size alone. The number to price against is the attacker's cost per witness versus the value of the human-exclusive power the cluster would unlock. Still not modelled: active proximity relays and forged devices (they attack the presence signal, not the graph), and the other signals.

### Forged or relayed presence (simulator, policy v2)

Presence evidence can be forged (attacker devices attesting each other) or relayed (an active proximity relay). Model: every sybil carries perfect presence; vouching strength is the cluster's seed-anchored trust relative to the honest median.

- Plain weighted average (policy v1): one colluding credential already let 10 of 400 sybils pass the gate, and six let all 400 pass — perfect presence made any trace of graph trust enough.
- Policy v2: no live vouching evidence → score capped below the audit window (a presence-only farm, however large, never reaches audit); presence may lift the score at most 15 points above the vouching strength. Result with forged presence: 0 colluders → 0 pass; 1 → 0; 6 → 0 pass (57 reach audit); 24 → 395 of 400 pass.

Reading: v2 closes the cheap attacks (forged presence alone, or with one to six colluding credentials) but 24 colluding mature credentials still lift a forged-presence cluster over the *current* minimum, because the starting minimum (30) corresponds to only 15% of the honest median vouching strength. The minimum and the vouch quotas are the levers; the starting numbers should be raised (or the pass test should require vouching strength directly) before any gated action carries real value. Note also that the earlier "24 colluders do not lift a cluster" result used the stricter 50%-of-median graph criterion, not the policy gate. Hardware-backed ranging for presence remains the real fix for forged presence.

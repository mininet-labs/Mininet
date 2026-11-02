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
- `AuditGate` / `GateOutcome::{Pass, NeedsAudit}`: no exclusion variant exists.
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

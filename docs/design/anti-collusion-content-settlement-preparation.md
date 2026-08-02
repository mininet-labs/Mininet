# Anti-collusion content/engagement settlement preparation (Track F5, D-0427)

**Status:** Doctrine and research preparation only. No `mini-settlement-
integrity`, `mini-delivery-challenge`, or `mini-settlement-audit` crate
exists. No nullifier/accumulator dependency added. `mini-contribution`
(D-0417) and `mini-attest` Tier 0 (D-0404) are unmodified — they remain
exactly what they are today: a linkable settlement/review baseline, not
yet collusion-resistant.

**Full research:** `docs/design/
cryptographic-architecture-and-flagship-research-protocol.md` §7 (D-0421)
names this gap precisely and is the origin of this document — it is the
"dedicated Phase-0 doctrine document for §7" that D-0421's own Required
follow-up calls for. This document does not re-derive §7's problem
statement or requirements list; it restates them only as needed to define
a phased path, and otherwise defers to D-0421 as the canonical framing.
Adjacent prior art this document builds on rather than re-deriving:
`docs/design/mn602-mn603-anonymous-resource-payment-preparation.md`
(D-0099, whose nine-phase shape this document mirrors), `mini-attest`
Tier 0 (D-0404) and its named Tier 1/Tier 2 follow-ups (roadmap #228/
#229), and `mini-contribution`'s vertical slice (D-0417).

## Decision

Adopt, as canonical framing for Track F5 ("provider payments" in the
MiniSearch research doc's vocabulary, generalized here to any open,
un-gated content/engagement settlement — MiniSearch crawl/serve rewards,
`mini-contribution`'s creator/seeder splits, and any future federated-
search provider payment converge on the same unsolved problem):

**The problem, restated from D-0421 §7.** `mini-contribution` settles a
`PaymentClaim` when `DeliveryEvidence` binds a role, a split, and a
signed completion — an ordinary signed receipt. A single operator
controlling both the requester and provider identity can fabricate
arbitrary self-traffic and extract unlimited settlement, because nothing
today distinguishes "a real distinct human requested this" from "a
signature exists." This is not a bug in `mini-contribution` — its design
never claimed to solve this — it is a genuinely open, un-doctrined gap.

**What today's linkable baseline already provides, and does not
solve.** `mini-contribution` (D-0417) + `mini-attest` Tier 0 (D-0404)
together already are this track's Phase 1 (see phased path below): every
settled claim is bound to a signed, content-addressed, replayable
receipt. That receipt is fully linkable — provider, requester, claim
digest, and timing all correlate in the clear — and linkability is not
itself the problem D-0421 §7 names. The problem is duplicate/collusion
detection and rate-bounding, which linkability alone does not provide: a
colluding pair can sign as many distinct-looking linkable receipts as
they want.

## Role separation

Five roles, kept separable in any future implementation the same way
mn602-mn603 §"Role separation" keeps its own five roles apart — no
future PR may collapse them without a new decision:

1. **Requester** — the identity whose balance funds a claim; already
   `mini-contribution`'s existing role, unchanged.
2. **Provider** — the identity that delivers content/service and is
   named in `DeliveryEvidence`; already `mini-contribution`'s existing
   role, unchanged.
3. **Settlement coordinator** — binds evidence to a `PaymentClaim` and
   finalizes it through `mini_execution::LedgerChain`; already
   `mini-contribution`'s existing role, unchanged.
4. **Collusion-limit issuer** — issues and verifies the scoped
   nullifier-style rate-limit credential a claim must carry (proposed,
   not built); never learns which specific claim a nullifier was later
   attached to, mirroring mn602-mn603's blind-issuer role and
   `mini-attest` Tier 2's issuer-unlinkable design goal.
5. **Auditor** — draws a delayed, randomized sample of already-settled
   claims for after-the-fact collusion review (proposed, not built);
   never gates a claim's original settlement, only flags patterns for
   separate governance/economic response.

## What a resolution needs (D-0421 §7's requirements, unchanged)

Carried forward verbatim as the requirements any future implementation
must satisfy — this document does not weaken, narrow, or reinterpret
any of them:

- requester-funded payment only, never unlimited protocol-issued
  subsidy per claim (the treasury economic model's existing constraint,
  inherited unconditionally);
- unpredictable delivery challenges a fabricated self-traffic loop
  cannot precompute;
- bounded, explicitly-labeled protocol subsidies that are never
  wire-distinguishable from paid settlement (the same non-negotiable
  constraint mn602-mn603 carries for resource-payment subsidies);
- a personhood or scarce-resource constraint on claim frequency, which
  inherits the project-wide Sybil-resistance gap (#18) — this document
  does not solve #18 and no phase below may be represented as doing so;
- duplicate/collusion detection across claims, not just per-claim
  signature validity;
- privacy-preserving rate limiting via a nullifier-style scoped-context
  mechanism, structurally similar to `mini-attest` Tier 2's per-context
  nullifier but domain-separated for the settlement context — reusable
  *research*, not reusable *code*, since the settlement and attestation
  contexts must derive nullifiers under different domain separation;
- delayed, randomized audits over settled claims, not just
  at-claim-time verification.

## Proposed (not yet built) crate boundaries

Named here so a future implementation has a stable target, not created
in this document — mirroring mn602-mn603's own "proposed, not built"
section:

- `mini-settlement-integrity` — nullifier-scoped duplicate/rate-limit
  credential issuance and verification for settlement claims; the
  collusion-limit issuer role above.
- `mini-delivery-challenge` — unpredictable, per-claim challenge
  generation and verification, hooked into `mini_engagement`'s existing
  completion flow so a claim cannot settle against a precomputed or
  replayed delivery.
- `mini-settlement-audit` — delayed, randomized sampling over
  finalized `mini-contribution` claims, surfacing collusion patterns to
  governance without gating original settlement.

None of these three crates exist. `mini-contribution`, `mini-attest`,
`mini-engagement`, `mini-execution`, and `mini-settlement` all stay
exactly what they are today: no dependency edge to any of the three
proposed crates is added by this document.

## Voice/value wall (hard rule, restated for this track)

Every one of the three proposed crates settles or gates *value*, never
*governance*. None may ever be imported by `mini-forge::governance` or
`mini-chain` voting, and none may export a type that participates in
vote weight, review quorum, validator selection, personhood score, or
witness selection — Directive 16's wall, inherited unconditionally, the
same way mn602-mn603 §"Voice/value wall" inherits it. A provider or
auditor role earning or flagging settlement activity must never
translate into governance authority.

## No new cryptography

Directive 14 applies in full. Per-context nullifier/accumulator
construction is genuinely novel engineering surface for a *settlement*
domain this workspace has not yet built (only researched, for the
*review* domain, in `mini-attest` Tier 1/Tier 2, roadmap #228/#229) —
nothing here selects, composes, or invents a primitive. The eventual
implementation is expected to reuse whatever externally-reviewed
accumulator/nullifier construction `mini-attest` Tier 1/Tier 2 research
lands on, domain-separated for the settlement context, rather than
independently researching a second one — sharing review cost the same
way D-0421 §8's flagship synthesis already asks Tracks 1-3 to. External
cryptographic review (D-0047) is required before any construction here
gates real value, and per Sybil resistance remaining unsolved
(INVARIANTS.md's hard limitation), no phase below may claim
one-human-one-claim.

## Phased path (mirrors mn602-mn603's nine phases)

0. This doctrine document.
1. **Already shipped, recognized as Phase 1 by this document — no new
   code.** `mini-contribution` (D-0417) + `mini-attest` Tier 0 (D-0404):
   linkable, signed, replayable settlement receipts. Collusion is
   detectable only by manual/off-chain inspection today; that is this
   phase's known, accepted limitation.
2. A non-monetary prototype of `mini-settlement-integrity`'s nullifier
   issuance and duplicate check, against synthetic claims only —
   mirrors `mini-attest` roadmap #228's Tier-1 accumulator experiment
   rather than duplicating it.
3. `mini-delivery-challenge` integrated with `mini_engagement`
   completion, still valueless — a claim cannot settle without
   answering an unpredictable challenge, but nothing real is at stake
   yet.
4. Both prototypes combined against one low-risk `mini-contribution`
   claim type, still no real settlement value.
5. `mini-settlement-audit`'s delayed randomized sampling, against the
   Phase 4 integration's own claim history.
6. Adversarial simulation: colluding-pair self-traffic, replayed
   challenges, audit-evasion timing — the same discipline mn602-mn603
   Phase 5 and `mini-attest`'s own adversarial suites already apply.
7. External cryptographic review of the chosen nullifier construction
   (D-0047) and external economic review of subsidy bounding (D-0048),
   both required, neither optional.
8. A closed, valueless pilot inside `mini-contribution`'s existing
   vertical-slice scope.
9. A limited MINI-backed pilot, only after all of the above *and* only
   for participants whose claim frequency is bounded by whatever
   personhood/Sybil-resistance state exists at that time — this phase
   cannot be reached by this document alone, since it depends on #18
   resolving independently.

None of Phases 1-9 are started by this document.

## Constitutional impact

None. This document creates no crate, changes no function signature,
and grants no new authority. `mini-contribution`, `mini-attest`,
`mini-engagement`, `mini-execution`, and `mini-settlement` are all
unmodified.

## Implementation status

None. Zero lines of implementation code.

## Failure point

A doctrine-only document can drift from the crates it describes; whoever
first ships Phase 2 code should re-verify this document's role-
separation and requirements sections still match `mini-contribution`'s
and `mini-attest`'s then-current shape before building on them, the same
discipline `docs/STATUS.md` already requires project-wide.

## Required follow-up

Someone ready to own Phase 2's non-monetary nullifier prototype; explicit
coordination with roadmap #228/#229 (`mini-attest` Tier 1/Tier 2) so the
settlement-context and review-context nullifier derivations stay
domain-separated by design rather than by accident; no other follow-up
is proposed by this document.

## Supersedes / superseded by

None. Fulfills D-0421's own named Required follow-up ("a dedicated
Phase-0 doctrine document for §7"); does not supersede D-0421, D-0417,
D-0404, or D-0099.

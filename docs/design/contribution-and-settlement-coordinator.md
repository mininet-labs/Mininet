# Contribution and Settlement Coordinator

**Status:** doctrine design doc, no code shipped yet. Companion code lands as
vertical slice 1 under this same decision (D-0417) once this doc is reviewed.

**Refs:** D-0026 (`mini-media`), D-0352/FD-18, D-0400 (`mini-provider`),
D-0402/D-0403 (`mini-engagement`), D-0413–D-0416 (`mini-economy`/
`mini-execution`/`mini-settlement` monetary stack), D-0407 (Forge-native
contributor coordination — the parallel, out-of-scope loop this doc does not
duplicate); roadmap #18 (Sybil/personhood), #47/#50 (treasury/whale
resistance), Directive 4, Directive 5, Directive 13, Directive 16.

## Why

A founder strategic direction reframed Mininet's core purpose as a
participant-owned economic network rather than centrally operated
infrastructure: participants publish content, other participants seed it,
requesters pay for delivery, and creators/seeders are rewarded — a
**publish → seed → request → deliver → receipt → settle → reward** loop. A
parallel loop exists for *code* contribution (propose a working group, claim
a task, get reviewed, get merged) — that loop already has a home in
`mini-forge::coordination`/`mini-cli::coordination` under proposed D-0407 and
is **not** this doc's scope.

This doc's job is narrower and concrete: identify exactly what already
exists for the *resource/content* loop, name the genuine gap, and scope the
smallest crate that closes it without re-implementing anything that already
works.

## What already exists (verified against current code, not paraphrased)

| Crate | Real capability | Boundary |
|---|---|---|
| `mini-objects` | `Object`/`ObjectBuilder`, content-addressed `ObjectId`, signature + provenance verification (`verify_provenance`) | The universal signed envelope; no payment or delivery concept |
| `mini-media` (D-0026) | `publish_media`, `read_manifest`, `missing_chunks`, `assemble` — chunked, content-addressed manifests (`ObjectType::MEDIA_MANIFEST`) with whole-payload digest reassembly | **This already is the "content manifest" vocabulary.** No new manifest type is needed. It has no creator-vs-seeder distinction and no price/payment concept |
| `mini-store`/`mini-sync` | Real object storage and peer replication (chunks ride ordinary `mini-sync` like any object, D-0080 proved this generically) | No offer/request semantics — sync moves whatever a peer already decided to exchange |
| `mini-provider` (D-0400) | `ProviderDeclaration`, `EngagementGrant`, `LocalProviderPolicy`, `ProviderRanker` — local, structural-only discovery vocabulary | LEAF, no network client, no payment, no canonical registry (INV-18-04) |
| `mini-engagement` (D-0402/D-0403) | `Engagement`/`EngagementState`/`Party` type-state machine; `accept`/`complete`/`dispute`/`release_milestone`/`timeout`; `escrow_claim` is a real `mini_settlement::PaymentClaim`; `canonical_completion_status` reconciles against a real `CanonicalLedgerView` | Generic on purpose (no `CardIssuance`/`Courier` variant — non-negotiable #10); has no defined shape for "delivery evidence," and does not itself submit or split claims |
| `mini-storage` | `ServeReceipt`/`ReceiptFields`, `verify_serve` → `ServeVerdict`; mutually signed host+witness proof one serve happened, replay-checked via `ReplayGuard` | Proves *a* serve happened once; does not run automatically during a real sync exchange (its own docs name this `pending`); no link to an `Engagement` |
| `mini-settlement`/`mini-execution` (D-0413–D-0416) | `PaymentClaim` (`network_id`, `payer`, `payee: Vec<u8>`, `amount_micro: u64`, `sequence`), `sign_claim_for_network`, `PaymentAdmissionPool`, `LedgerState` with real per-account balances, checked debit/credit, supply conservation | One claim moves funds between exactly two opaque accounts; nothing composes *several* claims for one logical payment, and nothing decides who the several payees should be |
| `mini-resource-pricing` | `PriceVector`/`quote` | Quoting only — "no payment execution, no e-cash, no ledger write" per its own module doc |
| `mini-reward` | Existing presence/storage point accrual | A separate, non-MINI-denominated reputation signal; continues to exist unchanged alongside this loop, not replaced by it |

## The actual gap

No crate composes the above into one lifecycle. Concretely missing:

1. A typed offer/acceptance step that binds one `mini-media` `Manifest`, one
   `mini-provider` `ProviderDeclaration`, and one `mini-resource-pricing`
   `Quote` into a proposed `mini-engagement::Engagement`.
2. A **delivery-role** vocabulary distinguishing *creator* (the manifest
   object's signed author — already present, just unread by anything) from
   *seeder* (whoever is serving chunks right now, possibly not the author).
3. A binding from `mini-storage::ServeVerdict` (proof of delivery) into an
   `Engagement`'s completion evidence — today `complete`/`release_milestone`
   take no defined evidence shape for "here is the verified serve."
4. **Multi-party reward split**: given one payer and N payees (creator plus
   one or more seeders) for one completed engagement, deterministically
   build N `PaymentClaim`s from a split policy, with correct per-payee
   sequencing, that sum to no more than the agreed amount.
5. A **reward-evidence-binding** rule: a claim is only ever constructed from
   a *verified* `ServeVerdict`, never from an unverified delivery claim —
   and the funding source is the requester's own existing balance, never new
   issuance (see Doctrine, below).

## Proposed crate: `mini-contribution` (new, LEAF)

Same posture as `mini-provider`/`mini-resource-pricing`/
`mini-publication-policy`: pure vocabulary and deterministic composition,
**no network client, no cryptographic signing of its own, and it must never
depend on `mini-forge` or `mini-chain` voting** (P1, the voice/value wall —
this crate lives entirely on the value side, composing settlement and
delivery evidence, never governance).

Proposed surface (finalized during implementation, not frozen by this doc):

- `DeliveryRole { Creator, Seeder }`
- `RewardSplit { creator_bps: u16, seeder_bps: u16 }` — must sum to `10_000`;
  constructor rejects otherwise.
- `split_amount(total_micro: u64, split: RewardSplit, creator_account: &[u8], seeder_accounts: &[Vec<u8>]) -> Vec<(Vec<u8>, u64)>`
  — deterministic integer division; any division remainder stays with the
  payer (never distributed, never lost) — the same "leave the remainder
  unissued" discipline `mini_economy::plan_human_share` already established
  for a different split.
- `bind_delivery_evidence(engagement: &Engagement, verdict: &ServeVerdict) -> Result<DeliveryEvidence>`
  — checks the verdict's content id matches the engagement's manifest and
  the witness role matches the requester, producing the byte-exact evidence
  `mini_engagement::complete` consumes. No new signature scheme; this only
  checks already-verified fields line up.
- `settle_completed_engagement(engagement: &Engagement, evidence: &DeliveryEvidence, split: RewardSplit, seq: impl FnMut(&[u8]) -> u64) -> Result<Vec<PaymentClaim>>`
  — the coordinator function. Builds but does **not** submit the claims;
  the caller submits each into a `PaymentAdmissionPool` themselves, keeping
  this crate free of network/pool I/O (the same boundary
  `mini-publication-policy` already draws around `mini-transport-policy`).

## Ten-step lifecycle, mapped to real primitives

1. **Publish** — Alice calls `mini_media::publish_media`; gets a `Manifest`
   plus the `Object` chain in her local `Store`. This is the content
   manifest; nothing new is built here.
2. **Announce** — Alice creates a `ProviderDeclaration` (`mini-provider`)
   naming herself `Creator` for that manifest's `ObjectId`, gated by her own
   `LocalProviderPolicy`.
3. **Seed** — Bob replicates Alice's manifest and chunks into his own
   `Store` over `mini-sync` (existing `sync_bidirectional`, D-0080 already
   proved this generically), then publishes his own `ProviderDeclaration` as
   `Seeder` for the same `ObjectId` plus a `mini-resource-pricing` `Quote`.
4. **Discover** — Carol uses `ProviderRanker` (already exists, local-only)
   to pick Bob's declaration.
5. **Request** — Carol proposes a `mini-engagement::Engagement` naming Bob,
   the manifest, and the quoted amount; `escrow_claim` is a real,
   Carol-signed `PaymentClaim`, not yet submitted.
6. **Accept** — Bob calls `mini_engagement::accept`.
7. **Deliver** — Bob serves the manifest's chunks to Carol over `mini-sync`;
   Carol reassembles with `mini_media::assemble`, which independently
   re-verifies the whole-payload digest — Carol never trusts Bob's claim
   that delivery was correct.
8. **Receipt** — Carol (witness) and Bob (host) sign a
   `mini_storage::ServeReceipt`; `verify_serve` produces a `ServeVerdict`.
9. **Settle** — `bind_delivery_evidence` + `settle_completed_engagement`
   build `PaymentClaim`s from Carol to Bob (seeder share) and to Alice
   (creator share, read from the manifest `Object`'s signed author — no
   separate creator registry); `mini_engagement::complete` records
   completion locally; `canonical_completion_status` later confirms against
   the real ledger once the claims finalize.
10. **Reward** — Alice's and Bob's wallets query existing
    `mini_execution::LedgerState` balances. No new "reward" primitive is
    built: **settlement is the reward**, deliberately distinct from
    `mini-reward`'s separate non-monetary point system, which keeps
    covering presence/storage activity that isn't a paid engagement.

## Doctrine: reward-evidence-binding, payment-funded not inflation-funded

- No `PaymentClaim` is ever coordinator-constructed without a verified
  `ServeVerdict` behind it. `verify_serve`'s existing checks (distinct
  identity roots so a host cannot witness itself, `ATTEST` capability,
  fresh non-replayed nonces, freshness policy) are the anti-Sybil floor this
  doc relies on — it does not raise that floor.
- That floor is honestly incomplete: two colluding identity roots can still
  fake a serve/witness pair for each other. This is the same open problem
  `docs/INVARIANTS.md` already names for `mini-storage`/`mini-presence`
  (identity-root ≠ verified human). This doc does not claim to close it.
- Funding is exclusively the requester's own existing balance, debited
  through the real `LedgerState` debit/credit path (D-0415). Never
  treasury, never new issuance, never a `mini-economy` epoch. This is what
  keeps the coordinator entirely on the settlement/value side and off
  `mini-forge`/governance (P1) — a reward here is never inflation, only a
  voluntary transfer two identities already agreed to and a third
  independently verified.

## Vertical slice 1 (Alice/Bob/Carol) — scope for task #219

**In scope:**
- The `mini-contribution` crate: `DeliveryRole`, `RewardSplit` +
  `split_amount`, `bind_delivery_evidence`, `settle_completed_engagement`,
  and a minimal `ContributionOffer` type (single provider, no negotiation
  protocol).
- One integration test with three in-process identities over a shared
  `mini-store::Store` (the same convention `mini-cli`'s own integration
  tests already use): `publish_media` → two `ProviderDeclaration`s →
  `Engagement` propose/accept → real byte transfer via the shared store →
  `ServeReceipt`/`verify_serve` → `settle_completed_engagement` → two
  `PaymentClaim`s admitted into a `PaymentAdmissionPool` → `apply_block`
  finalizes them → `LedgerState` balance queries confirm Alice's and Bob's
  MINI increased and Carol's decreased by exactly the total, with the
  division remainder (if any) left undistributed.

**Explicitly out of scope, named honestly rather than silently skipped:**
- Real network transport for offer/discovery (slice 1 uses local/shared
  `Store` discovery only — the same honest limit `mini-provider`'s own docs
  already state for `ProviderRanker`).
- Adversarial dispute/timeout coverage — `mini_engagement::dispute`/`timeout`
  are reused as-is, unmodified, but slice 1's test only exercises the happy
  path.
- Any CLI, wallet history, or contribution dashboard surface (matches how
  `mini-engagement`/`mini-provider` themselves shipped pure-crate-first,
  CLI wiring later).
- Crawler/search or Forge-market extensions — unchanged, out of scope,
  already covered by D-0312/D-0316/D-0317/D-0407.

## Constitutional impact

No frozen invariant is amended. M1–M3 stay exactly as `mini-settlement`
already enforces them — this crate only *constructs* `PaymentClaim`s and
*reads* verdicts; it never grants itself finality authority. P1 (voice/value
wall) is preserved by construction: `mini-contribution` depends on
`mini-engagement`/`mini-storage`/`mini-settlement`/`mini-execution`/
`mini-media`/`mini-provider`/`mini-objects`, none of which touch
`mini-forge` or `mini-chain` voting, and it must never gain such a
dependency. No new cryptography — this composes `mini-crypto`'s existing
Ed25519 signing via `mini-settlement::sign_claim_for_network` and
`mini-storage`'s existing receipt signing. The typed-domain rule is
respected: `settle_completed_engagement` takes exact `Engagement`/
`ServeVerdict`/`RewardSplit` types, never a generic `sign(&[u8])`.

## Required follow-up

- External economic review of any default `RewardSplit` before real value
  (same D-0047 gate `mini-value`/`mini-treasury` sit behind).
- Real network transport for offer/discovery/delivery (slice 1 is
  local-store only).
- Adversarial dispute/timeout test coverage.
- Wallet/dashboard/CLI surface (a later slice, not this one).
- Reconciling the `Creator` role read from a manifest's author with
  `mini-publication-policy`'s `Attribution` field once a manifest is
  published under a chosen visibility/attribution policy (Track D1) — not
  yet cross-checked by this doc.
- Sybil resistance beyond one co-witnessed serve remains the open #18
  problem; this doc does not solve it.

## Supersedes / superseded by

Builds on, and does not supersede, D-0026, D-0352, D-0400, D-0402, D-0403,
D-0413, D-0414, D-0415, D-0416, or D-0407. It introduces no new
cryptography and amends no frozen invariant.

# Human-evidence taxonomy reconciliation

D-0303. Lane L5 of `docs/design/privacy-cost-doctrine-parallel-execution-plan.md`
(D-0300), scoped `MN-401`, closes tracking issue #137. Reconciles the
founder research's (`docs/research/MININET_RESEARCH_V2_20260713.md` §10)
five confidence classes — `Unassessed → ActiveParticipant →
HumanEvidenceQualified → StrongHumanEvidence → ExternalUniquenessBacked`
— against `mini_uniqueness::HumanStatus`, without introducing a rival
taxonomy (the exact risk D-0094's Required-follow-up field named and
deferred).

## Decision

The source-research confidence classes do not form a single ordinal
state machine — they combine three orthogonal dimensions (participation,
accumulated human-evidence confidence, and external-evidence provenance)
into one list. Mininet keeps `HumanStatus` exactly as it is today
(`Unverified`, `VouchedHuman`, `EvidenceQualifiedHuman`,
`crates/mini-uniqueness/src/status.rs`) — **no new enum variant, no new
type, no code change in this lane.**

## Mapping

| Source class | Mininet representation | Reasoning |
|---|---|---|
| `Unassessed` | `HumanStatus::Unverified` | Direct semantic match — "not enough evidence yet" either way. |
| `ActiveParticipant` | *(not a `HumanStatus`)* | Participation (signing, storage-serving, presence, governance activity) is behavior, not evidence of humanity — an automated root can sustain activity indefinitely. Promoting on activity alone would let bot/farm accounts launder into a human-labelled status. Belongs to separate participation-metric tooling if it's ever built, orthogonal to this enum. |
| `HumanEvidenceQualified` | `HumanStatus::VouchedHuman` | The source class means "crossed the minimum threshold for positive human-related evidence" — `VouchedHuman`'s existing fast-path (reachable from one modest trusted signal) is the honest match, not the strongest state. |
| `StrongHumanEvidence` | `HumanStatus::EvidenceQualifiedHuman` | `EvidenceQualifiedHuman` already requires a high fused score, multiple distinct *live* sources, a minimum evidence age, and (by default) the seed-anchored vouching graph specifically (`PromotionPolicy::full_required_sources`, closing the #18 Sybil review's farm-saturation bypass) — this is already the natural, already-shipped destination for "sustained, diverse, aged evidence." |
| `ExternalUniquenessBacked` | *(not a `HumanStatus` — evidence provenance)* | An external issuer's uniqueness assertion is evidence Mininet can weight, never proof that Mininet itself has established one-human-one-root. `SignalSource::External(u32)` already represents this provenance today. A fourth status would make one external issuer's *scoped* assertion (e.g. "unique within our enrollment database") outrank Mininet's own strongest, multi-source, self-verified evidence — and would risk exactly the "one human, one identity" overclaim `CLAUDE.md`'s hard rules forbid. |

## The hard limitation this reconciliation must not soften

No `HumanStatus` value — including `EvidenceQualifiedHuman` — establishes
global personhood uniqueness, or proves that one human controls only one
`did:mini` identity root. This is `docs/INVARIANTS.md`'s standing hard
limitation (§2, "read the hard limitation above first"), unchanged by
this document. An externally verified credential, however cryptographically
sound, establishes only that a presentation was valid, an issuer was
accepted under some policy, specific claims were asserted, and required
freshness/status/holder-binding checks passed — never that the issuer's
enrollment was free of duplicates, or that the subject holds no other
Mininet root.

## Rejected alternatives

- **Add all five source classes as `HumanStatus` variants** — rejected:
  conflates participation, evidence confidence, and provenance into one
  ordinal axis where none exists; makes an external assertion appear to
  outrank Mininet's own strongest internally-verified state.
- **Add only `ExternalUniquenessBacked` as a fourth status** — rejected:
  different external issuers have incompatible trust/exclusion/privacy
  assumptions that cannot be collapsed into one Mininet-wide status; risks
  external issuers becoming de facto personhood authorities; `External(u32)`
  already covers the provenance-tracking need.
- **Add `ActiveParticipant` as a fourth status** — rejected: bots are
  active participants too; this is a category error between behavior and
  evidence of humanity.

## What this lane does not do

No `ExternalEvidenceAssessment` type, no credential-format adapter (W3C
Verifiable Credentials / OpenID4VCI / OpenID4VP / SD-JWT VC), no issuer
trust-policy registry, no uniqueness-scope field, and no context
nullifier exist yet. These belong to later, separately-scoped lanes —
`MN-402` (`EvidenceStamp` interface + issuer diversity rules), `MN-403`
(private continuity proof phase 1), `MN-404` (context nullifier +
pairwise pseudonym design), `MN-405` (aggregate proof prototype), `MN-406`
(external uniqueness credential adapter), `MN-407` (Sybil-farm/coercion
simulation) — each of which should produce a *scoped* evidence assessment
that becomes one more weighted `SignalEvidence` entry, never a value that
directly assigns a `HumanStatus`. In particular, `MN-406`'s adapter should
enforce that external evidence alone can never independently promote a
record to `EvidenceQualifiedHuman` — at least one live Mininet-native
source (matching today's `full_required_sources` default) should remain
required, so this workspace never silently outsources its personhood
policy to an external issuer.

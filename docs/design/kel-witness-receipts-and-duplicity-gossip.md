# KEL witness receipts and duplicity gossip (audit #12 F4, invariant M3)

**Status:** Phase 0 (design), Phase 1 (receipt types, D-0321), and Phase 2
(in-memory witness state machine, D-0326) shipped. Phase 3 onward not
started.

**Full research:** `docs/research/
KEL_WITNESS_RECEIPTS_DUPLICITY_GOSSIP_RESEARCH_20260715.md`
(founder-supplied, 2026-07-15). This document does not reproduce that
report — it records the adopted direction and the phased plan this repo
commits to, and links back for the full threat model, prior-art survey
(KERI witness receipts, Certificate Transparency gossip, Key Transparency,
CoSi), and protocol design.

## The gap this closes

`did_mini::FreshnessPins` (D-0088) already solves the case where a
verifier has *previously seen* an identity: a newly presented conflicting
event is rejected because it contradicts retained state. It does **not**
solve the harder case audit #12's finding F4 named: a verifier meeting an
identity **for the first time** has no prior head to compare against, and
two internally-valid, controller-signed branches can both pass ordinary
KEL verification in isolation. This is the "never seen a fresher log"
gap — SPEC-01 §7, invariant M3's harder half.

## Decision

Adopt the report's recommended architecture: KERI-inspired asynchronous
witness receipts plus proof-carrying gossip, **not** a global identity
ledger, BFT witness consensus, or interactive signature aggregation.

- Establishment events gain a versioned `WitnessPolicy` (witness set +
  threshold + generation).
- Witnesses issue typed `WitnessReceipt`s over a `WitnessReceiptStatement`
  binding identity, sequence, event digest, prior digest, event kind,
  witness-policy generation, and a coarse observation epoch — never a
  generic `sign(bytes)`, per CLAUDE.md's typed-domain rule.
- Enough receipts against the active policy's threshold form a
  `WitnessedEventCertificate`, independently verifiable offline.
- Witnesses keep first-seen monotonic state per identity; a witness that
  signs conflicting receipts, or a controller that signs conflicting
  same-sequence events, produces a compact, self-contained,
  independently-verifiable duplicity proof.
- Peers gossip compact `KelHeadSummary`s during ordinary relevant
  interactions (not a standalone always-on service); disagreements
  trigger targeted evidence retrieval, not full-log flooding.
- A first-contact verifier's acceptance rule and resulting `KelAssurance`
  (`Direct` / `Pinned` / `Witnessed` / `WitnessedRecent` /
  `WitnessedRecentAndGossiped` / `DuplicityDetected`) replaces any
  boolean "is this fresh" claim with an honest, gradable assurance level.

## Why design-only, this PR

The report's own closing recommendation: "the best next engineering
deliverable is a design-only PR, followed by a small receipt/proof type
PR, then an in-memory witness state-machine PR, and only afterward
network gossip. The dangerous mistake would be starting with a witness
daemon or Merkle log before freezing exactly what a receipt means, what
constitutes duplicity, and what a first-contact verifier is allowed to
claim." This repo's own established discipline this session (MN-207/208,
PQ-15) already scopes founder-supplied research to its own recommended
first phase — here that phase is documentation, not code, so that's
exactly what this PR is.

## Phased plan this repo commits to (see report §29 for full detail)

0. **Design and state audit** — this document.
1. **Receipt types (shipped, D-0321)** — `WitnessPolicy`,
   `WitnessReceiptStatement`, `WitnessReceipt`,
   `WitnessedEventCertificate`; canonical encoding; signature
   verification; no network service. Lives in `did-mini::witness`.
2. **In-memory witness state machine (shipped, D-0326)** — `WitnessJournal`
   in `did-mini::witness_state`: first-seen acceptance, direct-successor
   verification, duplicate idempotence (returns the previously issued
   receipt, never re-signs), stale rejection, same-sequence conflict
   detection (`ControllerDuplicityProof`, built from real controller-
   signed `Event`s), and a standalone `WitnessEquivocationProof::assemble`
   for a third party holding two disagreeing receipts from one witness.
   Trusts the caller that `event` is already chain-valid at its claimed
   position — no signature/pre-rotation/recovery verification, no
   fork-proof construction for the harder "conflicting descendant" case,
   no persistence, no network. Lives in `did-mini::witness_state`.
3. **KEL verification integration** — `KelAssurance` output alongside
   ordinary KEL validity, never replacing it with one boolean.
4. **Receipt collection protocol** — typed request/response messages
   (`SubmitEventForWitnessing`, `FetchWitnessCertificate`, etc.).
5. **Gossip summaries** — piggybacked on existing sync/relay/forge
   traffic, targeted fetch on disagreement only.
6. **Persistent witness service** — durable state, crash recovery,
   bounded retention, quotas.
7. **Witness rotation and recovery** — policy generations, old-policy
   certification of witness-set changes, unavailable-witness recovery
   that can't be triggered casually.
8. **Public-authority transparency** — per-witness append-only receipt
   logs for governance/release/validator/treasury roots only.
9. **Adversarial network simulation** — forks, witness collusion,
   partitions, eclipse attacks, recovery conflicts.
10. **External review** — gated behind D-0047 before any high-value
    authority decision depends on this layer.

## Hard rules carried forward from the report

- **No global freshness claim, ever.** A witnessed-and-gossiped
  certificate proves a threshold of configured witnesses observed one
  branch within the verifier's gossip horizon — it does not, and must
  never be described as, proof that no other branch exists anywhere.
- **Witnesses never gain authority.** They attest observation; they
  cannot create, rotate, or override identity events. A witness that
  goes unavailable triggers an explicit, non-casual recovery path — not
  a silent threshold reduction.
- **No global identity log.** Public transparency logs are opt-in and
  restricted to public-authority roots (governance/release/validator/
  treasury); private and pairwise identities get scoped gossip, never
  global enumeration.
- **Recovery never erases duplicity evidence.** History stays
  append-only even when lawful recovery overrides a compromised branch.
- **Ordinary independent signatures first.** No BLS, CoSi, threshold
  aggregation, or bespoke consensus in the first version — per Directive
  14 and CLAUDE.md's no-new-cryptography rule, composing `did-mini`'s
  existing typed-signature machinery is sufficient for Phase 1-3.

## What this document originally covered, and what D-0321/D-0326 added

This document was originally Phase 0 only: no new type, `FreshnessPins`
(D-0088) unmodified, no witness state machine, no receipt format, no
gossip protocol. D-0321 (Phase 1) shipped `did-mini::witness`'s four
receipt/certificate types, canonical encoding, and signature/threshold
verification — see that decision-log entry for exactly what it does and
does not cover. D-0326 (Phase 2) shipped `did-mini::witness_state`'s
`WitnessJournal` in-memory state machine plus `ControllerDuplicityProof`/
`WitnessEquivocationProof` — see that entry for exactly what it does and
does not cover. No receipt format wired into real establishment events
(`event.rs`'s `Establishment.witnesses: Vec<Vec<u8>>` field remains its
own pre-existing, differently-shaped placeholder, still unused), no
`KelAssurance`/KEL-verification integration, and no gossip protocol exist
yet — those are Phases 3-5, each its own PR, each scoped no larger than
this session's established discipline for founder-research-driven work.

# Cryptographic architecture: composition over invention, and the flagship research protocol (D-0421)

**Status:** Doctrine and research-roadmap synthesis only. No code, no new
crate, no new cryptographic primitive. This document creates no new
capability and modifies no existing crate's behavior; it names, cross-
references, and closes gaps in the *framing* around work this repository
has already scattered across a dozen documents, and names one genuinely
un-doctrined gap for future research.

## Why this document exists

CLAUDE.md's "no inventing cryptography" rule and D-0063 (`docs/DECISION_LOG.md`)
already say what Mininet must never build: proprietary hash functions,
ciphers, signature schemes, RNGs, password hashing, TLS, or general-purpose
ZK curves/proving systems. That rule is a *fence*. It has never been paired,
in one place, with the *field inside the fence* — a positive statement of
where Mininet-specific protocol composition over standard primitives is
not just permitted but required, because no existing off-the-shelf protocol
expresses Mininet's participant-owned, privacy-preserving, no-central-
platform economic model.

That positive statement already exists — just distributed. `docs/design/
mn602-mn603-anonymous-resource-payment-preparation.md` (D-0099),
`docs/design/mn208-pir-research-and-review-preparation.md` (D-0098),
`docs/design/frontier-personhood-governance-and-consensus-proposals.md`,
`docs/design/post-quantum-identity-migration.md` (D-0095/D-0322), and the
shipped `mini-provenance`/`mini-forge::release` code (D-0068, D-0070) each
independently commit to the same discipline: standard primitives,
Mininet-specific *assembly*, staged maturity, external review before real
value. This document is the index that says so explicitly, so a reader —
founder, auditor, or a future agent picking up this codebase — does not
have to independently discover that the pieces already agree. It also
names the one piece that does not yet exist anywhere: an anti-collusion
reward-settlement doctrine for open, un-gated content/engagement
contribution (§7 below).

**Refs:** CLAUDE.md's "No inventing cryptography" hard rule; D-0063
(no-new-crypto clarification and founder override); Directive 14
("simplicity is security"); the six design docs and two shipped crates
named throughout this document.

## Decision

Adopt, as canonical framing (not new policy — this restates and organizes
decisions already made):

> Mininet composes Mininet-specific *protocols* from established,
> independently-analyzed cryptographic *primitives*. A primitive is never
> invented in-house. A protocol composing primitives is Mininet's to
> design, but ships only as `experimental` until it has a written threat
> model, an external-review requirement gating real value, and a named
> shutdown/migration path — the same discipline D-0068's build provenance,
> D-0070's release transparency log, and every `mn6xx`/`mn2xx` research
> doc listed below already apply individually.

"More true to the project" means fewer trusted intermediaries, better
participant privacy, and verifiable useful contribution — never novel
mathematics pursued for its own sake, and never a claim that cryptography
alone can settle a question (genuine human attention, one-person
uniqueness, or social value) that INVARIANTS.md's own frozen "hard,
temporary limitations" section already admits are open.

## The six tracks, as they already exist in this repository

Each track below is a live research or implementation surface already
present in this repository. Nothing here re-derives them; each entry
states current status honestly and links onward rather than duplicating.

### 1. Private proof of useful contribution

**Status: doctrine only, staged, not implemented.**
`docs/design/mn602-mn603-anonymous-resource-payment-preparation.md`
(D-0099) is the primary doctrine: online-spend, issuer-backed, fixed-
denomination blind-signature tokens for relay/mix/storage/bridge/private-
index resource credit, with a nine-phase rollout (Phase 0 doctrine → …
→ Phase 9 limited MINI-backed pilot) and an explicit "No new
cryptography" section naming Privacy Pass, GNU Taler, and Coconut as
prior-art reference points, none embedded. `mini-resource-pricing`
(D-0302) is the only shipped code in this space, and it is pure quoting —
no keys, no issuance, no transfers. `docs/design/
frontier-personhood-governance-and-consensus-proposals.md` §3
(`mini-attest`) independently designs a three-tier assurance ladder for
the adjacent case of *review/attestation* eligibility from a completed
engagement (Tier 0 linkable receipts → Tier 1 Merkle-accumulator
membership hiding → Tier 2 blind/threshold issuance with scoped
nullifiers), naming the same candidate families (Coconut/BBS-style
credentials, Privacy Pass-like issuance, Semaphore-like nullifiers) this
document's flagship protocol (§8) also needs. These are not yet unified
into one crate; §8 below is where that unification is proposed as future
work, not built now.

### 2. Anti-collusion reward settlement

**Status: partially doctrined, one real gap. See §7.**
`docs/design/treasury-economic-model.md` names "receipt-verifier/oracle
collusion" as an explicit threat (its adversarial-scenario list, item 13)
but does not resolve it with a protocol. `docs/design/
contribution-and-settlement-coordinator.md` (D-0417, shipped as
`mini-contribution`) settles real `PaymentClaim`s from role/split/
evidence-bound completions, but its evidence is a signed receipt from the
existing `mini-engagement`/`mini-settlement` machinery — linkable,
un-audited beyond signature validity, and with no delivery-challenge or
duplicate-detection layer. This is the one track named in the source
framework with no existing Mininet doctrine document. §7 opens it.

### 3. Unlinkable personhood membership

**Status: doctrine only (research proposal, not accepted).**
`docs/design/frontier-personhood-governance-and-consensus-proposals.md`
§1 ("Sybil-resistant personhood") and §2 ("privacy-preserving
liveness/personhood proof") lay out an evidence-wallet →
published-policy → aggregate-proof → distributed-issuance →
credential-and-nullifier → recovery architecture, explicitly marked
"Research and design proposal; not accepted, not implemented." Its own
§1.7 names the residual unsolved problem plainly: establishing
one-person eligibility in the first place remains open — the same
admission INVARIANTS.md's frozen hard-limitations section already makes
("identity-root ≠ verified human"). `mini-uniqueness` (shipped) is the
current, much narrower three-signal fusion prototype; it does not
implement anonymous credentials or nullifiers.

### 4. Private federated search

**Status: research and review preparation only.**
`docs/design/mn208-pir-research-and-review-preparation.md` (D-0098)
freezes a first PIR workload and a candidate-technique portfolio
(explicitly "research targets, not selections") before any PIR crate
exists. This directly extends Track E (MiniSearch, D-0312) — `mini-query`
(D-0420, this same PR) implements Track E7/E8 (query parsing, result
provenance) with no privacy-preserving retrieval yet; Track F
(distributed/federated search, roadmap issue #175, not started) is where
MN-208's PIR research would eventually connect, and is out of scope here.

### 5. Recoverable, post-quantum identities

**Status: Phase 0/1 shipped (verify-only), production migration gated.**
`docs/design/post-quantum-identity-migration.md` (D-0095/D-0322) is
further along than the other five: `mini-crypto`'s `SignatureSuite::
MlDsa65` verify-only support is real, shipped code (D-0322), using the
already-standardized ML-DSA-65 (FIPS 204) construction — not a new
primitive. Its own "hard rule: no production migration before external
review" section keeps every recovery-policy, multi-suite-signing, and
key-evolution mechanism the source framework describes (§5 of the pasted
research) as declared future phases, not implemented today.

### 6. Proof-carrying Forge contributions

**Status: shipped and in production use in this repository's own workflow.**
The most mature of the six. `mini-provenance` (D-0068) implements
SLSA/in-toto-style signed build-provenance objects with independent-
builder agreement counting. `mini-forge::release` (D-0070) adds a
release transparency log, rollback protection, and equivocation
detection. `mini-build-runner-wasmtime` (D-0069) is the isolated,
capability-gated executor those provenance claims describe. `mini-cli`'s
`build`/`release`/`provenance`/`installer` subcommands (D-0077) and
`tools/no_github_outage_demo.sh` (D-0081) prove the whole pipeline end to
end. No cryptography was invented for this track: it composes
`mini-crypto`'s existing Ed25519/BLAKE3 primitives exactly as every other
signed object in this codebase does. This is the existence proof that
the "compose, don't invent" discipline this document names is not
aspirational — it already shipped, once, all the way through.

## What Mininet does not invent (restated, not modified)

CLAUDE.md's hard rule already lists this; it is not repeated in full
here to avoid two documents drifting apart. In summary: no proprietary
hash function, symmetric cipher, signature algorithm, RNG, password
hashing, TLS, or general-purpose ZK curve/proving system. Every
protocol named in §§1-8 of this document composes from a fixed set of
already-shipped or already-standardized building blocks: `mini-crypto`'s
Ed25519/X25519/AEAD/BLAKE3 (in production use throughout this
workspace), ML-DSA-65 (FIPS 204, D-0322), and — for the *research-stage*
tracks only, never yet embedded — externally-reviewed constructions
named by reference (Privacy Pass, GNU Taler, Coconut/BBS, Semaphore-style
nullifiers, standard PIR techniques). None of the reference constructions
above has been selected, vendored, or implemented by this repository;
naming them as research targets is not adopting them.

## Required maturity gate for every construction named above

Restating what D-0068, D-0070, D-0095/D-0322, D-0098, and D-0099 already
each independently require, so future work in any of the six tracks does
not have to re-derive it: before real value or real personal data ever
touches a construction from this document, it needs — at minimum — a
written threat model, explicit non-goals, a protocol transcript
specification, a domain-separation registry entry (extending
`mini-crypto`'s existing domain-separation discipline), replay/downgrade
analysis, abuse/collusion analysis, test vectors, at least one reference
implementation, and a named maturity label (`experimental` →
`candidate` → `activated`, mirroring D-0070's `Version`/rollback
machinery's own maturity posture). External cryptographic review is
required — not optional — before any construction backs real MINI or
reveals real personal data, per D-0047's existing external-audit gate.

## 7. The one open gap: anti-collusion reward settlement (research-only, this document)

No prior Mininet document owns this problem directly, though two
adjacent ones (`mn602-mn603`, `mini-attest` Tier 2) solve *pieces* of it.
Naming the gap precisely, per the source framework, without proposing an
implementation:

**The problem.** `mini-contribution` (D-0417) already settles a
`PaymentClaim` when `DeliveryEvidence` binds a role, a split, and a
signed completion. That evidence is an ordinary signed receipt: a single
operator controlling both the requester identity and the provider
identity can fabricate an arbitrarily large volume of self-traffic and
extract unlimited settlement, because nothing in the current design
distinguishes "a real distinct human requested this" from "a signature
exists." Ordinary signed receipts — the source framework's own words —
are insufficient for exactly this reason.

**What a resolution would need**, restated from the source framework as
research requirements, not a design:

- requester-funded payment (never unlimited protocol-issued subsidy per
  claim — the treasury economic model's existing "cannot pay unlimited
  newly issued MINI for views" constraint already rules this out);
- unpredictable delivery challenges a fabricated self-traffic loop cannot
  precompute;
- bounded, explicitly-labeled protocol subsidies only, never
  indistinguishable from paid settlement (the same non-negotiable
  constraint `mn602-mn603` already carries for resource-payment
  subsidies — inherited here, not re-derived);
- a personhood or scarce-resource constraint on claim frequency (which
  inherits Track 3's residual unsolved problem: this cannot be
  "one-human-one-claim" until Sybil resistance itself is solved, per
  INVARIANTS.md's frozen hard limitation);
- duplicate/collusion detection across claims, not just per-claim
  signature validity;
- privacy-preserving rate limiting (a nullifier-style scoped-context
  mechanism, structurally similar to `mini-attest` Tier 2's per-context
  nullifier — reusable research, not reusable code, since the settlement
  context and the attestation context are different domains and must use
  different domain-separated nullifier derivations);
- delayed, randomized audits over settled claims, not just at-claim-time
  verification.

**What this document does not do:** propose a token format, a crate
boundary, a threshold-issuance scheme, or a phased rollout for this gap.
Following `mn602-mn603`'s own precedent, that belongs in its own Phase-0
doctrine document once someone is prepared to own the phased rollout
`mn602-mn603` §"What's required before any code PR" models — a Phase 1
non-monetary prototype, Phase 2 real (but valueless) delivery-challenge
mechanics, external cryptographic review before Phase 9's real-value
pilot. This document only asserts that the gap is real, precisely
scoped, and not accidentally already solved by an adjacent doc.

## 8. The flagship synthesis: Unlinkable Proof of Useful Contribution

Not a new cipher — a name for where Tracks 1, 2 (§7), and 3 (personhood)
converge, because a single implementation eventually needs all three:

> A provider proves entitlement to a bounded reward for serving an
> authentic request, while settlement cannot trivially link the
> requester, provider, content, and their other activity, and fabricated
> self-traffic cannot create unlimited issuance.

This is not proposed as new work to start now. It is named so that when
Track 1 (`mn602-mn603`), Track 2/§7 (anti-collusion settlement), and
Track 3 (`mini-attest`, personhood) each eventually produce real code,
the people building them know in advance that a single coherent protocol
— not three independently-drifting ones — is the actual target, and can
share domain-separated nullifier derivation, blind/threshold-issuance
tooling, and external-review scheduling rather than each re-deriving it.
The staged path below is the same nine-phase shape `mn602-mn603` already
committed to, generalized:

1. Signed, transparent (linkable) receipts — `mini-contribution` (D-0417)
   already provides this for the content/engagement case; `mn602-mn603`
   Phase 1 provides it for the resource-payment case.
2. Scoped pseudonyms and replay-preventing nullifiers — `mini-attest`
   Tier 1 (Merkle-accumulator membership hiding) is the closest existing
   design.
3. Batched settlement with concealed individual requests.
4. Zero-knowledge verification of receipt rules (an established proving
   system used, not designed — per this document's own composition
   rule).
5. Collusion-resistant subsidy limits — directly inherits §7's open
   requirements above.
6. Independent cryptographic review (D-0047's existing external-audit
   gate; not new process).
7. A restricted, valueless economic pilot.
8. Governance-controlled activation (`mini-forge::governance`'s existing
   `propose`/`approve`/`merge` machinery — not a new governance
   mechanism).

**Constitutional impact:** none. This document creates no crate, changes
no function signature, and grants no new authority. It is pure doctrine
synthesis plus one newly-named (not newly-designed) research gap.

**Implementation status:** none. Zero lines of implementation code.

**Failure point:** a synthesis document risks becoming stale faster than
the six tracks it indexes evolve; whoever next ships code in any of
Tracks 1-6 should update this document's status lines in the same PR,
the same discipline `docs/STATUS.md` already requires project-wide.

**Required follow-up:** a dedicated Phase-0 doctrine document for §7 (the
anti-collusion settlement gap) when someone is ready to own its phased
rollout, mirroring `mn602-mn603`'s own structure; no other follow-up is
implied or scheduled by this document.

**Supersedes / superseded by:** supersedes nothing — restates and
cross-references D-0063, D-0068, D-0070, D-0095, D-0098, D-0099, D-0322,
and the (undecided) frontier-personhood proposal without modifying any of
them.

# FROST distributed key generation — audit scope package

Gates [roadmap #93](../../issues/93), **P0**
per D-0048. **Founder action required: same reviewer pool as
`crypto-audit-scope.md`, but treat this as a separate audit scope** — a
DKG bug is categorically worse than a signing bug: it can permanently
expose treasury/bridge keys the moment key generation happens, not just
compromise one transaction.

## Why this is separate from ordinary signing review

`mini-treasury`'s existing FROST signing (D-0041) uses **trusted-dealer
keygen** — one party briefly holds the whole secret before splitting it.
That's acceptable for the live demo; it is not acceptable for real
custody, because that one party is a single point of catastrophic failure
for however long it holds the secret, however careful the code around it
is. Real DKG means no party — not even briefly, not even in memory — ever
holds the full key.

## Scope

Implementing the DKG protocol itself (Pedersen DKG or equivalent) is
ordinary engineering work this repository can do; **trusting an
in-house DKG implementation without external review is not acceptable**
for anything backing real value, which is why this is a named gate rather
than just another roadmap issue.

**Status: implementation done (D-0060), audit not started.** This section
originally sketched the shape a future implementation should take; it now
describes the real one, in `crates/mini-treasury/src/frost_dkg.rs` and
`frost_reshare.rs`, so the auditor can map straight to source rather than
to a proposal.

## The Rust shape actually implemented (for the auditor's context)

Three keygen paths coexist as separate function namespaces rather than one
`KeygenMode` enum (a deliberate simplification — Directive 14 — since each
path takes structurally different arguments and unifying them into one enum
would mostly add match-arm ceremony, not safety):

```rust
// crates/mini-treasury/src/frost_keygen.rs — test/demo only
fn trusted_dealer_keygen(n, threshold, _ack: AcknowledgedPrototypeOnly) -> ...

// crates/mini-treasury/src/frost_dkg.rs — Pedersen DKG, RFC 9591 §4
fn dkg_round1(index, n, threshold, context, _ack: AcknowledgedUnauditedDkg) -> (DkgRound1Secret, DkgRound1Package)
fn verify_round1_package(package, threshold, context) -> Result<()>       // Feldman shape + Schnorr PoK
fn dkg_generate_round2_shares(secret, recipient_indices) -> BTreeMap<u16, Scalar>
fn dkg_verify_received_share(from_package, at_index, share) -> bool
fn dkg_resolve(round1_packages, complaints: &[DkgComplaint], rebuttals: &[DkgRebuttal]) -> Result<DkgResolution>
fn dkg_finalize(my_index, my_secret, round1_packages, received_shares, excluded, threshold) -> Result<(KeyPackage, PublicKeyPackage)>

// crates/mini-treasury/src/frost_reshare.rs — KeygenMode::ReshareFromPreviousEpoch
fn reshare_round1(old_key_package, old_participating_indices, new_n, new_threshold, context, _ack) -> (DkgRound1Secret, DkgRound1Package)
fn verify_reshare_round1_package(package, new_threshold, context, old_public_key_package, old_participating_indices) -> Result<()>
fn reshare_finalize(new_index, round1_packages, received_shares, excluded, new_committee_indices, new_threshold, old_public_key_package) -> Result<(KeyPackage, PublicKeyPackage)>
```

`DkgRound1Secret`/`DkgRound1Package` (Feldman commitments + Schnorr proof
of knowledge) are shared verbatim between DKG and resharing — resharing's
round 1 is literally a DKG round 1 whose constant term is
`lambda_i * old_share_i` instead of fresh randomness, checked against the
old committee's public verifying shares
(`verify_reshare_round1_package`'s one extra comparison).

**No `TestOnlyTrustedDealer`-forbidden-at-mainnet runtime guard exists
yet** — there is no `Network`/deployment-mode concept anywhere in this
workspace yet for such a guard to gate. What exists today is a
compile-time one: every keygen path takes an explicit, distinctly-named
acknowledgment type (`AcknowledgedPrototypeOnly` for the trusted dealer,
`AcknowledgedUnauditedDkg` for DKG/resharing) that must be constructed by
name at every call site — the same typed-authority discipline
`CLAUDE.md` requires generally. A real "forbidden in production" runtime
guard is real follow-up work once a deployment-mode concept exists
(tracked implicitly under #36-#45's chain/deployment work, not yet its
own issue).

## The questions an auditor must answer

- Does the commitment scheme actually bind each participant to their
  share before secrets are revealed (no participant can choose their
  share *after* seeing others')? Claimed yes — Feldman commitments are
  broadcast in round 1, before any round-2 share is sent.
- Is the complaint/dispute mechanism sound — can a malicious participant
  falsely accuse an honest one, or can a genuinely malicious share evade
  detection? Claimed no on both, via the rebuttal step (`frost_dkg`'s
  module docs derive why Feldman's equation has exactly one satisfying
  value, so neither direction of framing works) — this specific claim is
  the single highest-value thing for the auditor to try to break.
- Each of the following has a named test in `frost_dkg.rs`/
  `frost_reshare.rs`'s `#[cfg(test)]` modules — the auditor's job is to
  judge whether each test actually proves what its name claims, and to
  find the adversarial condition that has *no* test yet:
  - a rogue-key participant (`a_forged_round1_proof_of_knowledge_is_rejected`)
  - a missing/non-responsive share (`a_missing_share_from_a_non_excluded_participant_is_rejected_not_silently_dropped`)
  - a malformed commitment (`wrong_commitment_count_for_threshold_is_rejected`)
  - an equivocating participant (`an_equivocating_sender_is_caught_by_whichever_recipient_got_the_inconsistent_share`)
  - a false accusation against an honest, correctly-rebutting sender
    (`an_accuser_cannot_frame_an_honest_sender_who_correctly_rebuts`)
  - an old transcript replayed into a new session
    (`a_round1_package_from_one_session_does_not_verify_under_a_different_context`)
  - a resharing ceremony with a removed/excluded old signer
    (`a_misbehaving_old_sender_can_be_excluded_via_the_shared_dkg_complaint_machinery`)
  - a forged resharing contribution not equal to the claimed weighted share
    (`a_tampered_reshare_contribution_fails_verification`)
  - **not yet tested:** an aborting participant mid-ceremony beyond "their
    share never arrives" (no partial-round-1-but-not-round-2 scenario is
    exercised); nonce reuse across FROST *signing* sessions specifically
    (covered conceptually by D-0059's zeroize-on-drop, not by a dedicated
    reuse-attempt test, since `SigningNonces` being consumed by value
    would need to be worked around deliberately to even attempt reuse).
- Does resharing actually rotate the key material, or does it leave the
  old shares still capable of reconstructing the group key? **Answer:
  it does not revoke old shares** — `frost_reshare`'s own module docs
  state this plainly as an honest limit. This is not a bug to find; it is
  a known, documented gap in what code alone can guarantee (real deletion
  is an operational requirement on the old holders).

## Hard constraints on the review

No admin key, no backdoor, no bypass of the explicit acknowledgment types
(`AcknowledgedPrototypeOnly`, `AcknowledgedUnauditedDkg`) for anything the
founder intends to call "production." A real "forbidden at mainnet"
runtime guard (this doc originally sketched one as `TrustedDealerForbidden`)
does not exist yet — see the implementation-shape section above for why —
and adding one is real follow-up work, not a substitute for this audit.

## Recommended architecture note for whoever wires this into a real deployment

Per the founder's own framing (and consistent with this repo's typed-
domain hard rule in `CLAUDE.md`): treasury, bridge, and any future
emergency-pause signing should share **one** custody engine
(`mini-custody`, not yet built) rather than each growing its own signer
stack — one sealed authority boundary, not several ad hoc ones that could
drift apart under audit.

## What closes this gate

A written audit report on the DKG implementation specifically (separate
document from the general crypto audit, even if the same auditor does
both), triaged findings, and — before any real-value custody ceremony
runs — a signed-off `PedersenDkg` (or equivalent) implementation with
`TestOnlyTrustedDealer` provably unreachable in the production build.

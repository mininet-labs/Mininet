# Storage-fraud detection: replica-commitment collision evidence

**Decisions:** D-0437 (see `docs/DECISION_LOG.md`)
**Status:** Phase 1 (collision evidence) implemented and tested. Timing/latency-based fraud is explicitly out of scope — see "What this is not" below.
**Refs:** roadmap [#42](../../issues/42) (Phase 5.7, storage-fraud detection); [#31](../../issues/31) (Phase 4.3, replication-uniqueness, `mini-porep`, D-0063/D-0064); `mini-consensus::evidence::EquivocationEvidence` (D-0204), whose "detect and prove, assign no penalty" scope this module deliberately mirrors.

## The gap this closes

`mini-porep` proves *replication uniqueness* for one identity in isolation: its registration-time audit proves a given sealed replica required real sequential work to produce, and its ongoing challenge-response proves continued possession of that same sealed replica over time. What it cannot do by itself is answer a *cross-identity* question issue #42 names directly: are two storage providers who each claim to hold their own independently-sealed copy actually colluding — one warehouse serving challenge responses on behalf of several claimed-distinct identities, none of which is genuinely storing anything of its own?

`mini-porep`'s sealing already gives this an answer, provided one condition holds: `SealParams::replica_id` is bound to the sealing party's own identity. `crates/mini-porep/src/seal.rs`'s own `different_replica_ids_seal_to_different_replicas` test already proves the DRG construction: two distinct `replica_id`s sealing the *same* source data produce *different* sealed replicas, and therefore different Merkle roots (`SealedReplica::replica_root()`, the value `mini_spacetime::StorageCommitment::merkle_root` commits to). Two genuinely independent, honest sealers can never end up with the same committed root — so if two distinct identity roots each publish a signed commitment naming the *identical* root, that is direct, cryptographically checkable evidence that at least one of them did not actually do independent sealing work: either they share one physical copy (the "single warehouse, many claimed identities" collusion issue #42 names first), or one copied the other's commitment without ever sealing anything itself.

That is the same kind of finding `mini-consensus`'s equivocation evidence already established for BFT voting: a **self-authenticating byte-level collision** (two conflicting signed votes there; two identical signed commitments naming an identity-bound-but-colliding root here) is portable, independently verifiable proof of misbehavior without requiring any one party to be trusted to detect it.

## What this is (Phase 1: `mini-storage-fraud`)

- **`StorageCommitmentClaim`**: a signed, typed, content-addressed statement — "identity root `provider` holds a sealed replica whose PDP commitment is `commitment`, sealed under a `replica_id` this module itself derives from `provider`'s own DID and a caller-supplied `context` (not a free-form caller-chosen value)." `issue()` composes `Controller::sign_message` over canonical bytes (typed domain, not `sign(bytes)` — CLAUDE.md's hard rule). `verify()` checks the signature against the claimed root's KEL.
- **Binding `replica_id` to identity inside this module, not left to the caller,** is what makes the whole scheme sound: if a caller could pass an arbitrary `replica_id`, two colluding identities could simply agree on the same `replica_id` and legitimately seal to the same root without any misbehavior implied. `derive_replica_id(provider, context)` (`Blake3(domain || provider.scid() || context)`) removes that degree of freedom — an honest party has no way to *not* bind its identity into its own seal.
- **`CollisionEvidence`**: two `StorageCommitmentClaim`s from *different* provider roots naming the same `merkle_root`/`block_count`. `verify_collision(evidence, oracle)` checks both claims verify against their claimed root's KEL, the roots really differ (an identity cannot "collide" with itself — that is just a duplicate claim, not fraud), and the committed `StorageCommitment`s really are identical. Mirrors `verify_equivocation`'s exact shape and the same restrained scope: **this module produces and verifies proof, and stops there.** It assigns no penalty, revokes no storage reward, and excludes nobody. A future consensus/governance/reward layer that wants to act on this evidence is free to, but that layer does not exist yet and is explicitly not built here — the same boundary `mini-consensus::evidence` already drew for equivocation.

## What this is not

- **Not a network-timing/latency fraud detector.** Issue #42's other named scenario — "answering challenges via a fast network fetch rather than genuinely holding data" — needs a live network deployment to establish any honest latency baseline (round-trip time varies by real topology, load, and hardware that does not exist in this repo's test environment) and is a fundamentally different, harder problem: distinguishing "fetched from a well-connected peer in time" from "genuinely held locally" from network timing alone is an active systems-security research question, not a composition of primitives this repo already has reviewed. Attempting it here would mean inventing an unreviewed heuristic and calling it fraud detection — exactly the overclaiming CLAUDE.md's honesty-over-polish rule forbids. Left as explicit, named future work (see below), not attempted.
- **Not a slashing, exclusion, or reward-clawback mechanism.** No crate outside `mini-storage-fraud` depends on it; it creates no consensus authority, no automatic penalty, no governance role. A collision evidence object is exactly as actionable as a human/governance process chooses to make it — the same posture `mini-consensus::evidence::EquivocationEvidence` takes.
- **Not proof that the *other* condition (many claimed-distinct identities are actually one Sybil operator) is false.** Collision evidence proves two *specific* identity roots shared a replica. It says nothing about whether those two roots are themselves controlled by one operator — that is the identity/personhood Sybil question (#18/#21), a different, already-tracked open problem this module does not attempt to solve.

## Constitutional and authority impact

No frozen invariant is touched. No voice/value wall edge (P1, Directive 16): `mini-storage-fraud` depends only on `mini-crypto`, `did-mini`, `mini-porep`, and `mini-spacetime` — no `mini-value`/`mini-bounty`/`mini-treasury` edge in either direction, and nothing here creates or gates a reward, payment, or vote. No generic `sign(bytes)`/authority surface: `StorageCommitmentClaim::issue` takes a specific typed request (`provider`, `device`, `commitment`, `context`, timestamps), not raw bytes — the set of things this signature can mean is fixed at compile time.

## Tests

Adversarial coverage in `crates/mini-storage-fraud/src/collision.rs` and `commitment_claim.rs` (mirroring `mini-consensus::evidence`'s own test discipline):

- a genuine collision (two distinct roots, identical committed root) verifies as real evidence;
- two claims from the *same* root naming the same commitment are not collision (that is just a duplicate claim, not fraud);
- two claims naming *different* commitments are not collision (no conflict at all);
- a forged claim (signed by a device not delegated by the claimed root) is rejected, so a fabricated accusation cannot be laundered into proof;
- `derive_replica_id` is deterministic and differs across distinct `(provider, context)` pairs, and two independent honest seals under two different providers' derived replica ids over the *same* source data produce two different Merkle roots end-to-end (proving the real `mini-porep` sealing path, not just the derivation function in isolation, is what makes collision evidence meaningful).

## Required follow-up

- A real consequence layer (governance review, reward exclusion, or future consensus-level accounting) that consumes `CollisionEvidence` — deliberately not built here, the same boundary equivocation evidence draws.
- Network-timing/latency-based "fast fetch, not genuine possession" detection, once a live network deployment exists to calibrate against — a distinct, harder problem, not attempted in this slice.
- Wiring `StorageCommitmentClaim`/`CollisionEvidence` into whatever storage-provider registration flow eventually exists in `mini-net`/`mini-store`'s still-unstarted network shard distribution (named in the roadmap hub's own Phase 3/4 notes) — this module defines the evidence types and verification logic; it does not yet have a live registration point to observe real claims from.

## Supersedes / superseded by

New ground — no prior decision addressed cross-identity storage-collusion detection. Builds on and does not modify `mini-porep`'s sealing/audit/challenge-response construction (D-0063/D-0064, unmodified, reused exactly as-is) or `mini-spacetime`'s `StorageCommitment`/PDP machinery (unmodified). Mirrors, and is a sibling to, `mini-consensus::evidence::EquivocationEvidence` (D-0204) rather than extending it directly — equivocation is BFT-voting-specific and lives in `mini-consensus`, which `mini-storage-fraud` does not depend on (no voice/value-adjacent coupling between the two).

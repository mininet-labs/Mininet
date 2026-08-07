# Storage-fraud detection: identity-bound replicas, audited registration, and replica-conflict evidence

**Decisions:** D-0437 (see `docs/DECISION_LOG.md`)
**Status:** **Proposed.** Experimental Phase 1 data model and verification logic. Not integrated with any live registration surface, not externally audited, and **not sufficient for fraud attribution**. Nothing here may drive a payment, penalty, exclusion, or consensus outcome.
**Refs:** roadmap [#42](../../issues/42) (Phase 5.7, storage-fraud detection — this closes only part of it); [#31](../../issues/31) (Phase 4.3, replication uniqueness, `mini-porep`, D-0063/D-0064); [#18](../../issues/18)/[#21](../../issues/21) (personhood/Sybil, unsolved); [#72](../../issues/72) (external crypto audit, D-0047); `mini_consensus::evidence::EquivocationEvidence` (D-0204), whose "detect and prove, assign no penalty" scope this mirrors — but see §6 for why the analogy only goes so far.

---

## 1. The gap, stated precisely

`mini-porep` proves that *one* provider performed real sequential sealing work, and its ongoing challenge-response proves that provider still holds what it sealed. Neither answers issue #42's cross-identity question: are two providers who each claim an independent copy actually one warehouse serving many claimed-distinct identities?

`mini-porep`'s sealing gives a handle on this, but only under a condition it does not itself enforce: `SealParams::replica_id` must be bound to the sealing party's identity. `mini-porep` deliberately accepts any 32 bytes there — it is a cryptography crate and knows nothing about identity, which is correct. So the binding has to be established and *enforced* somewhere else, and that is what this design is about.

## 2. What the previous iteration got wrong

The first version of this work (PR #297) shipped a signed statement pairing an identity with a `mini_spacetime::StorageCommitment`, and treated two identities naming the same Merkle root as proof of shared storage. External review found that unsound, and the finding was correct. Recording it here because the failure mode is instructive and easy to repeat:

- **The claim was never connected to a seal.** It carried a bare `StorageCommitment` — a root and a block count with no internal structure — supplied by the caller. Verification checked a signature and nothing else. `mini-porep` appeared in the dependency list and in one test, and was absent from the production verification path entirely.
- **It admitted a trivial framing attack.** Anyone could watch an honest provider publish a root, sign that same root under their own perfectly valid DID, and produce evidence that verified. No replica, no challenge, no forged signature required. The "detects collusion" claim was, in the reviewer's words, a proof that two identities authenticated the same bytes.
- **It over-claimed what a duplicate proves.** Even with the binding fixed, a duplicate root shows at least one claim is unsound — not which one, and not that anybody colluded.

The lesson generalizes beyond this crate: **a signature over a value proves someone signed the value, never that the value describes reality.** Anywhere the protocol wants the second thing, something other than the signer has to have checked.

## 3. The design

### 3.1 A typed context, not opaque bytes

`ReplicaContextV1` names what a replica is *of*: `network_id`, `assignment_id`, `shard_index`, `replica_ordinal`, `sealing_policy_version`. An opaque caller-chosen `[u8; 32]` would let two honest implementations disagree about what a replica id means, and would let a claim made on a test network replay onto the real one.

Canonical encoding — fixed width throughout, no length prefixes, 77 bytes:

```text
u8    version (= 1)
[32]  network_id
[32]  assignment_id
u32be shard_index
u32be replica_ordinal
u32be sealing_policy_version
```

### 3.2 The identity-bound replica id

```text
replica_id = BLAKE3(
    "mininet/mini-storage-fraud/replica-id/v1"      // 40 bytes, verbatim, no length prefix
 || u32be len(root_scid_utf8)   || root_scid_utf8
 || u32be len(device_scid_utf8) || device_scid_utf8
 || ReplicaContextV1 canonical encoding             // 77 bytes
)
```

SCIDs rather than full `did:mini:<scid>` strings (the prefix is constant and carries nothing); both length-prefixed so neither can be slid into the other's field. Fixed test vectors for this derivation, the context encoding, and the seal-commitment digest live in `crates/mini-storage-fraud/tests/vectors.rs`. **If a vector changes, the wire format changed** — that is a version bump and a decision entry, not a test update.

**Binding to root + device is a protocol-policy choice, and it is the founder's to confirm.** Binding to the device means two machines under one root must genuinely seal twice rather than copying one replica between them — the stronger anti-warehouse property. The cost is equally real: replacing a storage device means re-sealing, which is exactly the expensive sequential work the construction is designed around. The alternative (bind to the root alone) lets a root move a replica freely between its own machines and loses the second property. This crate implements root + device + ordinal; §7 records it as an open question rather than treating an implementation detail as a settled protocol rule.

### 3.3 Registration: somebody other than the provider has to look

A provider signing its own commitment establishes only that it owns a key. `AuditAttestation` is one auditor's signed record of having run `mini_porep`'s registration audit against the *full* `SealCommitment`: sampling challenges under a seed the auditor chose after the commitment was published, recomputing the labeling hash itself, and refusing to sign unless every answer verified. `RegistrationReceipt` is a quorum of those.

`RegistrationPolicy` sets the bar: minimum distinct auditor *identity roots*, minimum sampled challenges each. `baseline()` is 2 roots and 64 challenges — 2 being the same floor `mini-forge` applies to code review (D-0033), the smallest quorum where nobody decides alone. **It is a floor, not a reviewed parameter choice.** Verification additionally requires:

- no auditor root equal to the provider root (no self-attestation);
- pairwise-distinct challenge seeds across the quorum, so a quorum cannot all replay one seed the provider supplied;
- distinct roots counted once each, so a root cannot reach quorum by delegating a second device to itself.

The claim carries the seal commitment, and the `mini_spacetime::StorageCommitment` is **derived** from it rather than supplied alongside it. That is what closes the "same root, different block count" bypass: there is only one statement about the replica's size, and it is inside the audited object.

### 3.4 Durable signatures across key rotation

Evidence meant to outlive its signer's next routine key rotation cannot be verified against the signer's *current* key state. `did-mini` gained `Kel::key_state_at(sn)`, `Kel::verify_message_at(sn, …)`, `Kel::event_digest_at(sn)`, and `Kel::head_digest()`; every signed object here cites the KEL sequence it was signed under **and** that event's digest, so the sequence is pinned rather than free.

Two honest limits, stated because they are easy to forget:

- **Historical verification is not a timestamp.** A holder of a compromised historical key can sign something new and present it as old. Establishing *when* needs an independent anchor — witnessed KEL receipts (SPEC-01 §7, M3), a chain height, or countersignatures recording the head each observer saw. `issued_at_ms` and `observed_at_ms` are self-reported and carry no authority.
- **The whole log must verify to its head** before any historical state is returned, so a truncated or corrupted tail cannot be used to resurrect a superseded key state.

### 3.5 A capability for storing

`Capabilities::STORE` is new, and is in **neither** secure default. Unlike signing or posting, a storage commitment exposes the root to durable, publishable conflict evidence about its own conduct: a device with this capability can bind its root to a claim that outlives the device. That liability is granted per storage device on purpose, not inherited from "this is my primary phone".

### 3.6 The registry is the enforcement point

`ReplicaRegistry` refuses a second claim over an already-accepted replica root. That is where the uniqueness invariant should normally be checked — at the moment someone tries to violate it. A claim that fails verification is an error, not a conflict: the registry stores nothing it could not check, so a rejected claim cannot squat on a replica root.

The registry is local and in-memory with no consensus behind it. Two registries run by two operators can each accept one half of a conflicting pair without ever learning of each other. That residual case is what conflict evidence is for.

### 3.7 Conflict evidence, and what it refuses to say

`verify_conflict` requires both claims to verify completely and independently first. Then a shared `replica_root` under two different identity-derived replica ids is reported as `ConflictKind::DuplicateReplicaRoot` (or `…WithDivergentShape` when the two commitments also disagree about node count, layer count, or data root — strictly stranger, because at least one commitment is then internally fabricated rather than merely duplicated).

`ConflictAttribution` has exactly one value, `Unattributed`, and it is not a placeholder. A verified conflict says **at least one of these two registrations is unsound**. It does not say which, and it is consistent with two honest providers plus one corrupt auditor quorum. `VerifiedReplicaConflict::required_follow_up()` states the next step in the object itself: re-audit both replicas independently under fresh verifier-chosen seeds, and review both quorums. **No provider may be penalised on this object alone**, and in particular not because another identity copied a public value.

### 3.8 Ongoing possession, and capacity that has to be proven

Registration proves a replica was genuinely sealed once. It says nothing about whether the provider still holds it next month, and nothing about how much storage that provider may claim to be contributing. `ReplicaLifecycle` carries a verified claim forward through both.

**Windows.** `WindowPolicy` divides time into fixed windows and says how many challenges each demands. `challenges_for(window, beacon, policy)` derives leaf indices from the seal commitment digest, the window index, and a beacon the **verifier** supplies. The provider contributes nothing to the derivation, so it cannot pre-compute which nodes it will be asked for and keep only those. The beacon must come from somewhere the provider does not control and must not be reused across windows — a recent block hash or a fresh verifier nonce both work, and this crate cannot check that it is either. That is a real assumption on the caller, stated here rather than hidden.

**Lapse is reversible; suspension is not.** A replica starts `Degraded { missed_windows: 0 }`, not `Active` — registration is not possession. An answered window makes it `Active`; a missed one degrades it and stops counting its capacity immediately, because capacity follows proof rather than history. Missing beyond `grace_windows` suspends it, and suspension does not self-recover: re-entry means registering again. The grace allowance is not leniency about fraud. From a verifier's position a missed window and an unreachable peer are the same observation, so the response is to stop counting, not to punish. `Retired` is the voluntary terminal exit.

**Capacity is derived, never supplied.** `capacity_units_of(claim, policy)` computes units from the sealed byte count in the audited seal commitment. `ProvenCapacity` has no constructor that accepts a number, so a provider's claimable capacity cannot drift from what a registration quorum actually checked. This closes a real hole one layer down: `mini_spacetime::MerkleStorageProof::new(commitment, capacity_units, policy)` takes `capacity_units` from its caller with nothing tying that figure to the commitment beside it, and `mini_spacetime::proposer_weight` documents that it "trusts its input completely". A provider could seal a single 32-byte node, register it honestly, and then declare a million units. That inverts the thesis the whole storage design rests on — "a thousand cheap, scattered machines outcompete one warehouse" holds only while capacity must be proven, since a warehouse and a Raspberry Pi can type the same number equally cheaply. Division is truncating: a replica smaller than one unit counts as zero rather than rounding up into capacity nobody sealed.

Windows are computed from caller-supplied milliseconds. A caller feeding a dishonest clock gets dishonest windows; anchoring needs the same witnessed-KEL or chain-height evidence §7.3 is still waiting on. Nothing here pays anyone, and no crate consumes `ProvenCapacity`. It is a measurement, not an entitlement.

## 4. A soundness gap this work found in `mini-porep`

Building the adversarial tests surfaced a real defect in `mini_porep::audit`, now fixed.

`verify_audit_response` checks labels against `layer_roots` and data against `data_root` at every challenge, but only touches `SealCommitment::replica_root` when the challenge lands on the **final** layer — that is the single point where the XOR-encoding step is verified. Challenges were sampled uniformly over `num_layers + 1` layers, so an audit could legitimately draw no final-layer challenge at all and return success having never constrained `replica_root`. A prover could publish another provider's replica root, or arbitrary bytes, and pass. For a 2-layer seal audited with 8 challenges that happens with probability `(2/3)^8` — about one audit in twenty-six. Not a corner case.

The fix reserves `max(1, count / (num_layers + 1))` challenges for the final layer — the same number uniform sampling gives on average, made certain instead of likely — exported as `mini_porep::encoding_challenge_budget`. Two regression tests cover it: every seed now draws a final-layer challenge, and a commitment carrying a forged `replica_root` fails under every seed tried.

This is the kind of defect that only appears when someone builds the attack rather than reasoning about the construction, which is an argument for adversarial tests over more documentation.

## 4b. A second gap this work found, in `mini-spacetime`

Building §3.8's proof windows surfaced a worse one. `verify_storage_challenge(commitment, response)` did not take the challenge. It checked that the response's Merkle proof was internally consistent and rooted correctly — and never that the leaf it proved was the leaf that had been asked for. `MerkleStorageProof::submit_response` then credited the window.

The consequence is that per-window possession proved nothing about possession. A prover challenged on leaf 7 could answer leaf 3, every time, and be credited, so satisfying an unbounded number of challenges over an unbounded number of windows required keeping exactly one leaf and its Merkle path — a few hundred bytes standing in for the whole replica. The entire proof-of-spacetime property collapsed to "can produce one authenticated path", which is the failure the layer exists to prevent.

The function now takes the challenge and requires `challenge.leaf_index == response.leaf_index == response.proof.leaf_index`, checking both the response's claim and the proof's own bound index so neither alone can be steered. `MerkleStorageProof::submit_response` and `PorepStorageProof::submit_response` thread the challenge through, and `verify_storage_challenge` is now actually re-exported from `mini_spacetime`'s root — it was `pub` in its module but reachable by no external caller, which is part of why nothing outside the crate had exercised it. Regression tests in both crates answer a leaf other than the one challenged and require refusal.

Both §4 and §4b were found by writing the attack, not by reading the code. Neither had a failing test before this work; both had passing ones.

## 5. What this deliberately does not attempt

- **Timing/latency fraud.** Issue #42's other named scenario — answering challenges by fetching from a fast peer rather than genuinely holding data — needs a live deployment to calibrate any honest baseline, and distinguishing "fetched in time" from "held locally" from network timing is an open systems-security research problem. An unreviewed heuristic wearing the word "fraud" would be worse than nothing.
- **Consequences.** No penalty, exclusion, reward clawback, or consensus authority. No crate in this tree consumes these objects to assign any.
- **Sybil resistance.** Distinct identity roots are not distinct humans (#18/#21). An audit quorum of `n` roots may be one operator with `n` identities. The quorum is a real cost to forge and a real improvement on self-assertion; it is not a trust anchor, and this crate must never be cited as evidence that Sybil resistance exists.
- **Network distribution.** There is no live registration surface to observe real claims from. `mini-net`/`mini-store` shard distribution remains unstarted.

## 6. Why this is not consensus equivocation

`mini-consensus`'s equivocation evidence is two incompatible statements **by one signer** — self-evidently contradictory, and the culprit is named by the object itself. This is two **different signers** whose statements are individually well-formed and jointly impossible. The asymmetry is why attribution is possible there and not here. The scope discipline is shared; the epistemics are not, and conflating them was part of the previous iteration's over-claim.

## 7. Open questions for founder/governance review

1. **What should replica uniqueness bind to** — the identity root, the storage device, or root + device + ordinal (as implemented)? §3.2 states the trade-off. This is a protocol rule, not an implementation detail.
2. **What is a defensible registration quorum** for a deployment carrying real value, given that auditors are self-selected and Sybil is unsolved? `baseline()`'s 2 roots is a floor borrowed from code review, not an analysis of storage economics.
3. **What anchors registration in time?** Until witnessed KEL checkpoints (M3) or a chain height are available, every timestamp in these objects is self-reported.
4. **What is the privacy cost?** A claim publicly links an identity root, a storage device, and an assignment. `did-mini`'s pairwise pseudonyms could unlink them across contexts, but doing so would also break cross-context conflict detection. That trade-off is unexamined and deliberately not resolved here.
5. **Where does the window beacon come from?** §3.8's unpredictability rests entirely on a beacon the provider does not control and that is never reused. A finalized block hash is the obvious source and would bind possession proofs to consensus; a verifier nonce is simpler but makes two verifiers disagree about whether a window was answered. This is a protocol rule, and the crate deliberately cannot enforce either choice.
6. **What is a defensible window length, challenge count, and grace allowance?** `daily()`'s figures are placeholders chosen to be legible, not derived from storage economics or from any measurement of real partition rates. Setting grace too high pays for storage nobody holds; too low, and every partition looks like fraud.

## 8. Required follow-up

- A consequence layer (governance review, reward exclusion, or future consensus accounting) that consumes conflict evidence. Deliberately not built.
- ~~A consumer for `ProvenCapacity`.~~ **Resolved by founder direction 2026-08-07 and implemented in D-0448.** `mini_spacetime::proposer_weight` now takes a typed `ProvenCapacity` and nothing else; the type has no numeric constructor, and `StorageCommitment::block_size_bytes` is re-checked against the served bytes on every challenge, so the derived path is the only path. `mini-storage-fraud`'s duplicate `ProvenCapacity`/`StorageUnitPolicy` were deleted in favour of `mini-spacetime`'s.
- A time anchor for proof windows, so window indices are not self-reported (§7.3).
- A networked, replicated registration surface, so uniqueness is enforced across operators rather than per-registry.
- Timing/latency detection, once a live deployment exists to calibrate against.
- **External cryptographic audit (#72, D-0047)** before anything here gates value. `mini-porep` is unaudited prototype cryptography and everything above inherits that.
- Founder resolution of the §7 questions before this leaves Proposed.

## 9. Supersedes / superseded by

Supersedes the design described in PR #297 (`StorageCommitmentClaim`/`CollisionEvidence`), which was never merged. Builds on `mini-porep` (D-0063/D-0064) and `mini-spacetime`, and modifies `mini-porep` only to fix the §4 sampling defect. Adds historical-verification methods and a `STORE` capability to `did-mini`. No `mini-value`/`mini-bounty`/`mini-treasury` or governance-crate edge in either direction, so no voice/value wall edge exists (P1, Directive 16).

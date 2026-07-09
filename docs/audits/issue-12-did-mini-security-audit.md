# Issue #12 — did-mini security audit

**Scope:** full review of `crates/did-mini` (~2,500 lines: `event.rs`,
`kel.rs`, `controller.rs`, `delegation.rs`, `codec.rs`, `limits.rs`,
`error.rs`, plus the four integration-test files), cross-checked against
SPEC-01 and the KERI model it adapts. Per the issue: key rotation and
pre-rotation correctness, witness mechanics, device delegation, recovery
paths, and device-compromise handling.

**Reviewer:** AI-drafted under D-0037; findings verified by running the
constructions and by new adversarial tests added in this batch
(`crates/did-mini/tests/recovery.rs`). Human review required as with every
audit in this directory.

**Verdict: sound core, two real findings fixed in this batch, one
launch-blocking gap that is honest and already scoped (M3 witnesses), and
several recorded assumptions.** This is an internal audit; it does not
substitute for the external review D-0047 gates production on.

---

## What was checked and found correct

| Property | How it's enforced | Status |
|---|---|---|
| SCID self-certification | `derive_scid` hashes the inception with the identifier field blanked (`Mode::ScidInput`), so the SCID cannot depend on itself; `Kel::verify` recomputes and compares both against the event and the log's claimed id | ✅ correct |
| Pre-rotation | Each establishment commits to multihashes of the *next* keys; a rotation must reveal keys hashing to exactly those commitments, in order, and adopt the pre-committed threshold; the rotation is signed by the **newly revealed** keys, so a leaked current key cannot rotate | ✅ correct (KERI semantics) |
| Chain integrity | `prior` carries the multihash of the previous event's full bytes; `verify` walks sn 0..head checking sequence, prior digest, and per-event signing threshold | ✅ correct |
| Threshold counting | `count_valid_signers` dedupes by **both** index and public-key fingerprint, so a repeated key or repeated signature cannot inflate the count; duplicate keys are additionally rejected at establishment validation | ✅ correct |
| Delegation mutuality | `verify_delegation` requires the device's `dip` to commit to the root **and** the root's KEL to carry an unrevoked `Delegate` seal — neither side alone can fake the link; self-delegation (`dip` naming its own SCID) rejected | ✅ correct |
| Capability conservatism | `Capabilities::from_bits` rejects unknown bits — a capability a verifier doesn't understand is never silently granted; defaults (`primary`/`secondary`) exclude `MANAGE_DEVICES` | ✅ correct |
| Wire hygiene | Hand-rolled deterministic codec; every decoder path is length-limited (`limits.rs`) before allocation; trailing bytes rejected at both the event and KEL level; non-UTF-8 and malformed DIDs rejected on decode | ✅ correct |
| Secret hygiene | `#![forbid(unsafe_code)]`; `Debug` redacts secrets; `SigningKey` drop-zeroization via ed25519-dalek's `zeroize` feature; seeds leave only via the loudly-named `to_seed_bytes` | ✅ correct (see F3 for the one gap found) |

## Findings

### F1 (fixed) — Rotation silently rewrote the M-of-N policy to N-of-N

`Controller::rotate_with_next` hardcoded the new next-set threshold to
`new_next.len()`. Consequence: any multi-key identity (say 2-of-3) had its
policy silently converted to 3-of-3 by its first rotation — from then on,
**losing any single next key bricked all future rotations**, and the
signing threshold itself jumped to N-of-N two rotations in, changing the
availability/security trade the holder chose at inception without any
explicit act.

**Fix (this batch):** `rotate()`/`rotate_with_next` now carry the standing
next-threshold forward unchanged; policy changes require the new, explicit
`rotate_with_next_and_threshold`. Regression test:
`rotation_preserves_threshold_policy`. No wire-format change — this was
purely controller-side event construction; previously emitted KELs remain
valid.

### F2 (fixed) — `verify_delegation` accepted a delegated identity as a root

Nothing required the `root` argument to be a *true* (non-delegated) root, so
a device could delegate sub-devices and pose as a root to any of the six
downstream callers (`mini-presence`, `mini-uniqueness`, `mini-chain`,
`mini-storage`, `mini-keystone`, `mini-objects`) that count "one identity
root." The exposure was bounded — root-counting layers rely on vouching, and
minting plain roots is equally free — but the check was cheap and the
ambiguity unnecessary.

**Fix (this batch):** `verify_delegation` rejects a delegated delegator with
the new `IdentityError::RootIsDelegated`. Device hierarchies, if ever
wanted, are roadmap #14's deliberate design, not an accident of a missing
check. Test: `delegated_identity_cannot_delegate_sub_devices`. Workspace
suite confirms no legitimate caller relied on the loose behavior.

### F3 (fixed) — Unscrubbed seed copies in pairwise-pseudonym derivation

`incept_pairwise_pseudonym` copied the root seed (`to_seed_bytes`), the
64-byte KDF output, and both derived seeds into locals it never zeroized.
`mini-crypto` itself is disciplined about this (drop-zeroization on key
types, scrubbed locals); this call-site was the gap. **Fix (this batch):**
all four locals are now zeroized (best-effort, same standard as
`mini-crypto`), with `zeroize` added as a did-mini dependency.

### F4 (open, launch-blocking for anything that needs revocation or recency) — No KEL freshness/duplicity layer

`Kel::verify` authenticates a log **in isolation**. Two consequences, both
inherent to the current milestone and both now pinned by tests:

- **Stale-KEL revocation bypass:** a revoked device stays "delegated" in any
  copy of the root's KEL predating the revocation. A verifier that accepts
  whatever KEL it is handed accepts the revoked device. Test-documented:
  `stale_root_kel_still_accepts_revoked_device_the_known_freshness_gap`.
- **Duplicity:** a controller (or thief holding current keys) can sign two
  different events at the same sn and show different logs to different
  peers; nothing cross-checks peers' views.

KERI's answer is witnesses/watchers and duplicity detection — exactly
SPEC-01 §7 / milestone M3, already reserved in the wire format (the
`witnesses` field). **Until M3 lands, every caller must (a) fetch the
freshest root KEL available and (b) pin the highest sn ever seen per SCID,
refusing regressions.** This rule is now stated in `verify_delegation`'s
own doc comment. Recommended follow-up, filed as part of this audit: a
small `KelPinner` (SCID → highest verified sn + head digest) callers can
share; `mini-forge::KelDirectory` is the natural home.

### F5 (recorded, no action) — Century-scale KEL growth cap

`Kel::from_bytes` rejects logs over 1,024 events (`MAX_KEL_EVENTS`). Fine
for the beta; **not compatible with Directive 13** for a heavily rotated,
device-churning identity over decades. KERI's own answer (receipts/
checkpointing so verifiers don't replay from genesis) should ride in with
the witness batch. Recorded in the recovery audit (#13) and THREAT_MODEL
"long-term cryptographic decay" is adjacent; owner: identity-continuity
issue #16.

### F6 (recorded, no action) — Assumptions inherited from mini-crypto

- Ed25519 signature **non-malleability** (event digests cover signature
  bytes; a malleable signature would fork a chain digest): satisfied by
  ed25519-dalek 2.x's strict verification; recorded as an explicit
  assumption on the suite.
- The `witnesses` establishment field is decoded, bounded (≤64×256 B), and
  **ignored by verification** — reserved for M3. Higher layers must not
  read it as meaningful. Recorded here so nobody does.
- CPU-bound DoS: a malicious 1,024-event KEL with 64 garbage signatures per
  event costs the verifier ~65k signature checks. Bounded and acceptable
  per-message; rate limiting is the transport's job (roadmap #75).

## Device-compromise summary (what a thief of one device gets)

| Thief holds | Can do | Cannot do |
|---|---|---|
| A delegated device (its keys) | Act within the device's capability bits until the root revokes it — and keep acting **against verifiers with stale root KELs** (F4) | Rotate the root; expand its own capabilities; delegate sub-devices (F2 fix); act after revocation against fresh-KEL verifiers |
| The root device (current keys) | Everything the root can do, until recovery | Rotate — rotation needs the **next** keys; if their seed is escrowed off-device, `recover_from_kel` retakes control and the stolen keys go dead (see issue #13 audit) |
| Current **and** next keys | Full takeover; recovery race decided by whichever rotation each verifier sees first | Win against verifiers that already saw the legitimate holder's recovery rotation (sn pinning); after M3, duplicity becomes detectable evidence |

## Traceability

Directive 2/8 → invariants ID1-ID5 (`docs/INVARIANTS.md` §3) → SPEC-01 →
`did-mini` (this audit) → `tests/identity.rs`, `tests/delegation.rs`,
`tests/pairwise.rs`, `tests/identity_modes.rs`, and the new
`tests/recovery.rs`.

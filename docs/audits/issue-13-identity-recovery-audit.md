# Issue #13 — Identity recovery edge-case audit

**Scope:** every recovery path a real human's `did:mini` identity can go
through, examined adversarially, per the issue's five scenarios: death,
lost devices, guardian collusion, time delay, recovery abuse.
**Deliverable, as the issue demands:** a written threat model per scenario
**plus concrete test cases** — `crates/did-mini/tests/recovery.rs` (8
tests), and a new recovery mechanism where none existed:
`Controller::recover_from_kel`, built in this batch.

The honest starting point: before this batch, **no recovery path existed at
all**. Pre-rotation committed to next keys, but the controller held them on
the same device as the current keys, and there was no API to reconstruct
control from the public KEL plus escrowed key material — meaning a lost
device was a permanently lost identity, despite the cryptography being
designed to prevent exactly that. That gap is now closed for the
escrowed-seed path; everything social (guardians, timelocks) remains
deliberately unbuilt (M5) and is threat-modeled below so it gets built
against these cases, not discovered after.

## The recovery model that now exists

Pre-rotation, used as KERI intends: the *next* keys are committed (as
hashes) but unrevealed, so their seeds can live **off-device** — a paper
backup, a safe, an heir's envelope, a bank box. Recovery is:

```
public KEL (from any peer)  +  escrowed next-key seeds
        │
        ▼
Controller::recover_from_kel(kel, escrowed_keys, new_next, new_next_threshold)
        │  verifies the KEL; checks the escrowed keys hash to the standing
        │  pre-rotation commitments (else RecoveryKeysMismatch); appends an
        ▼  ordinary rotation signed by the revealed keys
same DID, new controller — old device's keys are dead from this event on
```

Recovery is an ordinary rotation — verifiers need no special case, and a
recovery is indistinguishable on the wire from a planned rotation. Tests:
`lost_device_recovers_from_kel_and_escrowed_seed`,
`recovered_identity_is_fully_operational`.

## Scenario threat models

### 1. Lost devices

- **Partial loss (a delegated device):** the root revokes it
  (`revoke_device`); the replacement is delegated fresh. Already worked
  before this batch. Residual risk: **stale-KEL acceptance** — a verifier
  holding a pre-revocation root KEL still accepts the lost device (see
  Abuse, below).
- **Total loss (the root device):** `recover_from_kel` with the escrowed
  seed. Test: happy path + `old_device_keys_are_dead_after_recovery`.
- **Loss of the escrow too:** unrecoverable, **by design** — anything that
  could recover an identity without the committed keys is also a theft
  path. Test: `nothing_recovers_an_identity_without_the_committed_keys`.
  The product consequence is real and must be owned in UX: inception
  should not complete without the user confirming an off-device copy of
  the next-key seed. Filed as a client-app requirement, not a protocol
  change.

### 2. Death

Escrow **is** the estate plan: whoever holds the next-key seeds (heir,
executor, lawyer's envelope) can `recover_from_kel` and take control —
same mechanism, no special "death event." What does *not* exist and is
recorded honestly:

- No dead-man switch, no timelocked auto-transfer, no way for the network
  to know a human died. Anything of that shape belongs to social recovery
  (M5) and personhood (SPEC-02), not to this crate.
- An identity that dies without escrow is permanently orphaned. For
  governance counting this is actually the *safe* failure (an orphaned
  root eventually stops attesting presence and drops out of active-set
  counts); for **money** it means genuinely lost coins — already named in
  `docs/THREAT_MODEL.md` §3 ("dead economy / lost coins") and roadmap #51.

### 3. Guardian collusion

**No guardian mechanism exists yet** (M5), which means today's collusion
surface is zero and the single-holder escrow is the entire model. Recorded
now, so M5 is designed against it rather than audited after:

- A future M-of-N guardian scheme is structurally identical to an M-of-N
  **next** key set — pre-rotation already supports multi-key commitments
  with a threshold, and this batch fixed the bug that made M-of-N next
  sets unusable (threshold silently forced to N-of-N; see audit #12, F1,
  test `rotation_preserves_threshold_policy`). "Guardians" can therefore
  be built as *escrow of next-key shares*, with collusion cost = the
  threshold, and **no new event kind and no new trust primitive**. That is
  the recommended M5 shape: guardians who can *recover* but can never act
  day-to-day, because the current keys were never theirs.
- Collusion threshold must be chosen against both theft (raise M) and
  loss (lower M) — the tension is inherent; defaults are a product
  decision to make explicitly at M5, not here.

### 4. Time delay

No recovery timelock exists: `recover_from_kel` takes effect the moment a
verifier sees the rotation. Assessment, honestly argued both ways:

- **For a timelock:** it gives a theft victim time to contest a malicious
  recovery (thief somehow obtained escrow).
- **Against, today:** with no witness infrastructure (M3) there is nowhere
  neutral for a "contest" to happen and no shared clock to time it against
  — a timelock enforced only by the verifier that happens to see the event
  is theater. **Decision recorded:** timelocked recovery is deferred to M3
  witnesses + M5 social recovery, where a receipt quorum can actually
  enforce a delay. Until then the defense is escrow secrecy plus the race
  dynamics below.

### 5. Recovery abuse

- **False "I lost my device" (hijack attempt):** claiming loss grants
  nothing — recovery requires the committed keys themselves.
  `RecoveryKeysMismatch` otherwise. Tests:
  `recovery_with_wrong_keys_is_rejected`,
  `nothing_recovers_an_identity_without_the_committed_keys`.
- **Laundering a compromised identity:** an attacker who stole *current*
  keys cannot rotate (needs next keys), so they cannot "clean" the
  identity into keys only they hold. The legitimate holder's recovery
  kills the stolen keys. Test: `old_device_keys_are_dead_after_recovery`.
- **The race (attacker got current AND next keys):** both sides can now
  produce a valid rotation at the same sn; each verifier believes
  whichever it saw first. This is KERI duplicity, not a recovery-specific
  flaw; it becomes *detectable evidence* once witnesses land (M3). Pinned
  as a stated limit in `recover_from_kel`'s docs.
- **Stale-KEL replay (the one every caller must respect today):** a
  revoked or superseded state stays acceptable to any verifier holding an
  old KEL. Pinned by test:
  `stale_root_kel_still_accepts_revoked_device_the_known_freshness_gap` —
  written to FAIL the day the gap is closed, forcing the docs to update.
  Interim rule, now in `verify_delegation`'s doc comment: fetch the
  freshest root KEL available; never accept a lower sn than already seen
  for a SCID. Owner of the real fix: M3 witnesses (SPEC-01 §7).

## Bugs found and fixed during this audit

| | Finding | Fix | Test |
|---|---|---|---|
| 1 | Rotation forced next-set policy to N-of-N, bricking future rotations after a single lost next key and silently rewriting the holder's M-of-N choice | Threshold now carried forward; explicit `rotate_with_next_and_threshold` for deliberate changes | `rotation_preserves_threshold_policy` |
| 2 | No recovery API existed — pre-rotation's whole purpose was unreachable after device loss | `Controller::recover_from_kel` + `KeyState` now exposes `next_commitments`/`next_threshold` | 5 recovery tests |
| 3 | A delegated identity could pose as a delegating root | Rejected (`RootIsDelegated`) | `delegated_identity_cannot_delegate_sub_devices` |

## What remains open (owners named)

- **M3 witnesses / KEL freshness + duplicity detection** — the single
  biggest identity-layer launch blocker; everything above that says
  "stale" or "race" resolves there. (SPEC-01 §7; flagged in audit #12 F4.)
- **M5 social recovery** — build as threshold escrow of next-key shares
  per §3 above; guardian collusion = threshold by construction.
- **Client UX mandate** — inception must not complete without confirmed
  off-device escrow of the next-key seed; a recovery drill ("prove your
  paper works") belongs in onboarding. Product requirement, recorded here.
- **KEL growth cap (1,024 events)** vs. century-scale identities — issue
  #16's design work (checkpointing/receipts alongside M3).

## Traceability

Directive 2 (assume central authorities fail — recovery must need no one's
permission), Directive 6 (design for failure — device loss IS the expected
case), Directive 8 (human as root of trust) → invariants ID1/ID3
(`docs/INVARIANTS.md` §3) → SPEC-01 §5 → `did-mini::Controller::
recover_from_kel` → `crates/did-mini/tests/recovery.rs`.

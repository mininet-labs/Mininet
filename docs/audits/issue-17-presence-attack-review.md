# Issue #17 — Presence protocol attack review

**Scope:** `mini-presence`'s range-bound co-presence attestation
(`attestation.rs`, `verify.rs`, `ranging.rs`) against the issue's attack
classes: relay, time spoofing, GPS spoofing, BLE replay, device cloning.

**Reviewer:** AI-drafted under D-0037; claims verified against the code and
the crate's existing tests. Human review required.

**Verdict: the passive/replay/binding attacks are genuinely defended; the
active relay attack is NOT, and cannot be with software timing alone — this
is the crate's central honest limit and it is real. Presence is safe to use
as *one weighted signal* (which is exactly how `mini-uniqueness` consumes
it), and must never be treated as a standalone proof of proximity until
hardware ranging (UWB) with a distance-bounding protocol ships.**

## Attack-by-attack

### Relay attack (attacker relays between two distant real devices) — ⚠ NOT fully defended (known, structural)

The software RTT bound (`RangePolicy::max_rtt_ms`, default 50 ms over ≥4
samples) raises the cost — a relay adds latency, and a tight bound rejects
obvious long-haul relays — but a well-resourced attacker with fast hardware
close to *both* victims can keep added latency under the threshold. **This
is not fixable with application-layer round-trip timing**; it needs a real
distance-bounding protocol on hardware-timed exchanges (UWB), which is
precisely what `ranging.rs` reserves (`RangingSource`/`UwbRanging`) but does
NOT yet implement (`NoUwb` is the only shipped source).

- **What holds today:** the UWB path is *additive* — when a policy sets
  `max_uwb_distance_cm` AND an attestation carries UWB evidence, the tighter
  bound is enforced on top of RTT (`verify.rs` lines ~199-203); the
  `PresenceVerdict.hardware_ranged` flag lets consumers weight
  hardware-ranged presence higher. So the architecture is ready; the
  measurement source is the gap.
- **Recommendation (filed):** presence must be documented everywhere as
  relay-resistant only to the strength of its ranging source, and
  `mini-uniqueness` should weight `hardware_ranged: false` presence
  strictly below hardware-ranged. Distance-bounding over UWB is the real
  fix — roadmap #17 follow-on / platform-shell work.

### Time spoofing (forging `started_at_ms`/`finished_at_ms`) — ✅ defended for freshness, ⚠ inherent for absolute time

Device clocks are attacker-controlled, so absolute timestamps are not
trusted as truth. What the verifier enforces instead:

- **session duration** (`finished - started ≤ max_session_ms`) — a bound on
  a *difference*, robust to clock offset;
- **future-skew rejection** against the verifier's own `now_ms`
  (`finished_at_ms > now + max_clock_skew_ms` → reject);
- **max age** (`finished < now - max_age_ms` → reject), which bounds replay
  windows even across verifier restarts.

The residual — two colluding devices agreeing on a plausible fake absolute
timestamp inside the skew window — buys nothing without *also* defeating the
range and delegation checks, and the `at_ms` in the verdict is only ever
used as recency input to scoring, never as a security boundary.

### GPS spoofing (fake location commitments) — ✅ not trusted (by design)

`location_commitment` is an **optional hash**, never raw coordinates
(privacy, P5), and — verified in `verify.rs` — it is carried in the signed
transcript but **not used in any accept/reject decision**. There is no code
path where a location claim grants anything, so spoofing it accomplishes
nothing. Recorded so no future change starts trusting it without a
distance-bounding basis.

### BLE replay (replaying a captured legitimate exchange) — ✅ defended (multiple independent layers)

1. **Fresh mutual nonces**, each 32 bytes, both in the signed transcript;
   the two nonces must differ (`f.initiator.nonce == f.responder.nonce` →
   reject), so a party can't echo the other's.
2. **`ReplayGuard`** on `(device, nonce)` — with a two-phase check-then-record
   so a partially-invalid attestation never mutates replay state, and an
   atomic reject if *either* nonce was seen.
3. **`max_age_ms`** bounds how long any guard must remember a nonce, so
   replay resistance doesn't depend on guard memory surviving forever.
4. **Channel binding** — when the verifier participated, `expected_binding`
   must equal the transcript's `channel_binding`, so an attestation captured
   on one channel can't be re-presented on another.

The one operational requirement, already documented loudly on `Party.nonce`:
production MUST use `mini_crypto::random_32`; the fixed test nonces are a
test-only convention. And `ReplayGuard` MUST be backed by durable storage in
production (the trait doc says so). Both correct; both a deployment
responsibility, not a code gap.

### Device cloning (same device identity on two physical devices) — ✅ bounded to the same limit as all key custody

A cloned device = a copied signing key, which is the general key-compromise
case, not presence-specific. Presence adds no new exposure: an attestation
still requires a *distinct counterparty root* (`SelfPresence` rejected when
`initiator_root.scid() == responder_root.scid()`), a currently-delegated
`ATTEST`-capable device (`verify_delegation` + capability check, which also
rejects revoked devices), and the range/replay checks above. Two clones of
one identity still cannot manufacture presence with each other (self-presence),
and cloning a *counterparty's* key to fake a meeting is exactly the
key-custody boundary audited in issue #12. Revocation of a known-cloned
device is subject to the same KEL-freshness caveat flagged in #12 (F4) —
cross-referenced, not re-litigated here.

## Assumptions recorded

- Presence is a **weighted signal, not a proof** — its whole security posture
  assumes a consumer (`mini-uniqueness`) that fuses it with independent
  signals and weights hardware-ranged evidence above software-RTT evidence.
- The `InProcess` transport is proximity-accepted **for CI only** and must
  never be reachable in a production build's transport set.
- No change may start trusting `location_commitment` or absolute timestamps
  for accept/reject without a distance-bounding basis.

## Traceability

Directive 8/15 → invariant PH1 (`docs/INVARIANTS.md` §2, "co-presence is
range-bound and mutually signed; relay can't fake it" — **note: PH1's
"relay can't fake it" is aspirational at software-RTT strength; this review
is the evidence it needs UWB distance-bounding to fully hold**) → SPEC-02/03
→ `mini-presence::verify_presence` → `crates/mini-presence/tests/` +
`docs/THREAT_MODEL.md` §2 (routing/relay). Follow-up: weight `hardware_ranged`
in `mini-uniqueness`; implement a distance-bounding `RangingSource`.

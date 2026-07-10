# Presence/ranging hardware validation — test protocol

Gates [roadmap #97](../../issues/97)
(split from #22). **Founder action required: find a mobile engineer with
real devices** — this cannot be built or tested from a sandboxed
development environment with no phone hardware.

## What's already built (so the tester isn't starting from zero)

- `mini-presence::ranging::RangingSource` — the trait seam a platform
  shell implements. `NoUwb` is the reference/fallback (software RTT only)
  and is the **permanent correct behavior** for devices without a UWB
  chip, not a stub to delete.
- `mini-presence::verify::verify_presence` — the full verification path,
  reviewed adversarially in `docs/audits/issue-17-presence-attack-review.md`:
  replay, binding, and clone attacks are defended; the one honestly
  unresolved gap is that software RTT alone cannot defeat an active
  relay attack.
- `PresenceVerdict::hardware_ranged` — already threaded through so a
  consumer (`mini-uniqueness`) can weight hardware-ranged presence above
  software-RTT-only presence once a real `RangingSource` exists.

## What needs building

A real platform-shell `RangingSource` implementation:

- **Android:** the platform's UWB ranging APIs (API 33+) or BLE RSSI/RTT
  where UWB hardware isn't present, bridged to Rust via UniFFI (D-0020's
  architecture — the Rust core defines the trait, the native shell
  supplies the measurement).
- **iOS:** Nearby Interaction framework (UWB-backed on supported
  hardware), same UniFFI bridge pattern.

## Required hardware

- 2+ Android phones with BLE, ideally with UWB chips (check
  manufacturer specs — not all Android UWB support is uniform)
- 2+ iPhones with Nearby Interaction support, if the iOS path matters for
  the launch target (U1-chip-or-later iPhones for UWB; earlier models
  fall back to `NoUwb` behavior, which is fine)
- A physical space to actually run distance/relay tests — this can't be
  simulated meaningfully

## Test protocol — what must be demonstrated before this gate closes

1. **Baseline correctness:** two devices at a known distance produce a
   `UwbRanging.distance_cm` within the platform API's stated accuracy
   tolerance, across at least 20 trials at 3+ different real distances.
2. **`RangePolicy::max_uwb_distance_cm` enforcement:** a policy set below
   the actual distance correctly rejects the attestation
   (`PresenceError::UwbRangeExceeded`); a policy set above it correctly
   accepts.
3. **The relay-attack drill named in the #17 audit:** attempt an actual
   relay (two attacker devices physically far apart, relaying BLE/UWB
   signals between two victim devices) and confirm whether the real UWB
   distance-bounding defeats it where software RTT alone did not. This is
   the actual point of building real hardware ranging — document the
   result even if it's "still defeatable under conditions X," since that
   honesty matters more than a clean pass.
4. **Fallback correctness:** a device with no UWB chip (or UWB disabled)
   still completes presence verification via software RTT alone, with
   `hardware_ranged: false` on the resulting verdict, and no crash or
   silent failure.
5. **Battery/performance sanity:** ranging sessions don't materially drain
   battery or block the UI thread in normal use — not a hard pass/fail
   gate, but should be reported.

## Hard rule this implementation must respect

Per the #17 audit: hardware ranging is **additive tightening only**. The
software RTT bound must always be enforced regardless of whether UWB
evidence is present (`verify_presence`'s existing behavior) — a real
`RangingSource` implementation must never be wired in a way that lets UWB
evidence *replace* the RTT check rather than sit on top of it.

## What closes this gate

A merged platform-shell implementation (Android and/or iOS, scoped to
whichever the launch target actually needs) passing the test protocol
above, with the relay-attack drill result documented honestly in a new
dated entry under `docs/audits/`, cross-referenced from
`docs/audits/issue-17-presence-attack-review.md`.

## Detailed test matrix (2026-07-10) — for whoever runs the hardware

The five numbered requirements above are the pass/fail bar; this section
is the actual step-by-step protocol to run against them, so two
independent testers can execute it and get comparable logs.

**Minimum hardware:** 2 Android phones (ideally API 33+), 2 iPhones, a
laptop for logs, a tape measure, two indoor rooms plus one outdoor space.
**Better:** 3+ of each, a Raspberry Pi or ESP32 BLE board (relay-drill
proxy), a second router/hotspot, hallway/stairwell/cafe/outdoor access.

**Per-device data to log:** model, OS version, UWB support, BLE version,
battery-optimization state, foreground/background app state, distance
estimate, RSSI, latency, packet loss, timestamp, environment notes,
device position (hand/pocket/bag/behind wall).

**Test environments:** same room at 1m/3m/5m; adjacent room through a
wall; hallway line-of-sight; stairwell/multi-floor; outdoor open area;
crowded BLE/Wi-Fi area; phone in pocket/bag; moving user.

**Test classes:**

- **T1 — honest same-room baseline.** Place devices at measured
  distances, record 60s per distance, repeat 3×. Pass: signal
  distinguishes near vs. far better than random, variance low enough to
  be useful as a weak signal.
- **T2 — wall/floor false-positive.** One phone stays in the test room,
  the other moves behind a wall / upstairs / downstairs / into the
  hallway; log RSSI/ranging/latency. Pass: if wall/floor presence looks
  like same-room presence, the signal's weight must be set low.
- **T3 — pocket/bag/body blocking.** Repeat the baseline with the phone
  in hand, pocket, backpack, under a table. Pass: if normal carrying
  behavior breaks the signal, require retries or a lower weight.
- **T4 — BLE relay drill** (the specific drill named in requirement 3
  above). Use a laptop/ESP32/Pi as a BLE proxy if available, otherwise a
  manual delayed-relay experiment with two phones and network messaging.
  Measure added latency, success rate, and whether the protocol detects
  impossible timing.
- **T5 — internet relay drill.** Device A near the verifier, device B
  remote (another room/network); relay the challenge/response over the
  internet or local network; measure whether the verifier accepts. If it
  reliably accepts, BLE/UWB cannot be treated as a strong signal.
- **T6 — multi-sybil phone farm.** Place multiple devices in one area,
  attempt repeated presence proofs, measure duplicate/farm detectability.

**Log schema** — `docs/gates/hardware-test-log-template.csv` (shared with
the #98 Wi-Fi bearer protocol): `issue, test_id, run_id, timestamp_utc,
verifier_device, prover_device, os_pair, environment, distance_m,
wall_or_obstacle, device_position, signal_type, rssi_dbm,
range_estimate_m, latency_ms, packet_loss_pct, success,
suspected_false_positive, notes`.

**Acceptance criteria specifically for this detailed matrix:** at least 2
Android + 2 iPhone devices tested; same-room, wall, outdoor, and
pocket/bag cases all logged; at least one relay drill (T4 or T5)
attempted; results classify BLE/UWB as strong/medium/weak/unusable; the
final protocol recommendation caps its trust contribution accordingly —
this feeds directly into `docs/design/human-continuity-proof.md`'s §3
"Repeated physical co-presence" weight (currently capped at 20/100) and
should adjust that cap if the hardware results warrant it.

**Expected outcome, stated in advance so a surprising result gets
noticed rather than rationalized:** BLE alone is likely weak-to-medium
local evidence; UWB, where available, is stronger for ranging but not
universal; relay resistance depends on timing, secure hardware, and
protocol design; the system should end up treating BLE/UWB as local
freshness evidence, never identity.

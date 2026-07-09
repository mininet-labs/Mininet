# Presence/ranging hardware validation — test protocol

Gates [roadmap #97](https://github.com/britak420/Mininet/issues/97)
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

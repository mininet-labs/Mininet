# Device hierarchy design (issue #14, D-0530)

Status: policy layer shipped in `did-mini` (`DeviceTier`,
`Capabilities::for_tier`, `Controller::delegate_device_tier`,
`Controller::revoke_devices_except`/`revoke_all_devices`). This note records
the *why* behind that shape; the *what* is the crate's own rustdoc.

## Problem

`did-mini` already gives an identity root a real capability-scoped
delegation primitive (`BaseDeviceRole` is orthogonal — operational metadata,
never authority; see `crates/did-mini/src/base_device.rs`). What it did not
have was a *policy*: a named, bounded answer to "what kind of device gets
what capabilities," so every application built on top of `did-mini` is not
left re-inventing its own ad hoc device classes with its own (possibly
inconsistent, possibly over-broad) capability grants.

Issue #14 asks for that policy across four device shapes: a cold root
(rarely-used, highest-authority key), daily devices (phone-class, constant
use), hardware tokens (dedicated signing hardware), and future
implant/wearable devices — explicitly speculative, because Founder
Directive 13 ("think in centuries, not releases") forbids assuming today's
device shapes are permanent.

## Why four tiers, not more or fewer

Each tier is a distinct **risk profile**, not a distinct product category:

- **`ColdRoot`** — the device most likely to sit offline/air-gapped and be
  invoked only to re-key the device set after something else is lost or
  compromised. It is the *only* tier that gets
  `Capabilities::MANAGE_DEVICES` (and thus the full `Capabilities::ALL`
  bound) because its whole reason to exist is disaster recovery: if it
  cannot re-key everything else, it has no purpose.
- **`HardwareToken`** — dedicated signing hardware (a FIDO2-class key, a
  smart card). It is bound to `Capabilities::SIGN` alone. A hardware
  token's threat model is "this specific physical object is stolen or
  cloned"; giving it pay/post/vote/attest/manage-devices/store authority
  would turn a signing-nuisance theft into a funds, governance, or
  storage-liability incident for no operational benefit — the whole point
  of a dedicated signer is that it does one thing.
- **`DailyDevice`** — the phone-class device in constant use, and
  therefore the device most likely to be lost, left unlocked, or
  malware-infected. It gets `Capabilities::primary()`'s everyday bound
  (sign/pay/post/attest/vote) but never `MANAGE_DEVICES` (it cannot expand
  or contract the device set on its own) and never `STORE` (per the
  existing `STORE` rationale in `delegation.rs`: a storage commitment is a
  durable, publishable liability that must be granted on purpose, per
  device, never inherited from "this is my everyday phone").
- **`Emerging`** — the deliberately unfinished tier. A device shape this
  crate has no experience with yet (an implant, a wearable, or something
  not yet invented) does not get silently matched into `DailyDevice` just
  because it is also "phone-class" in some sense, and it does not get a
  bespoke capability set assembled on the spot either — both of those
  would defeat the whole point of a *fixed, typed* tier-to-capability
  mapping. Instead it starts at `Capabilities::secondary()` (sign/pay/post,
  no vote, no device management) — the network's existing conservative
  default for an unproven device — until enough real-world experience with
  the shape exists to warrant its own named tier and bound, decided the
  same way any other capability policy is decided: a new
  `docs/DECISION_LOG.md` entry, not a code change nobody reviewed.

Four tiers, not a numeric scale (`Tier(0..=255)`) or an open string label,
because the whole property this issue asks for is that capability sets are
*fixed and reviewed per tier*, not chosen ad hoc per device. A numeric or
string tier would let a caller "add a new tier" by picking an unused number
or string and assigning it whatever bits looked convenient at the call
site — exactly the un-reviewed capability growth the typed-domain rule in
`CLAUDE.md` exists to prevent. A closed (but `#[non_exhaustive]`) enum means
a genuinely new device shape requires a source change reviewed under the
same process as everything else, while the crate can still add a fifth,
sixth, ... variant later without an API-breaking rewrite.

## Why the mapping is a constructor, not caller-supplied bits

`Capabilities::for_tier(DeviceTier) -> Capabilities` and
`Controller::delegate_device_tier(&Did, DeviceTier)` are the only
tier-to-capability paths. There is deliberately no
`Controller::delegate_device_for_tier_with_extra_caps(...)` escape hatch:
letting a caller widen a tier's bound at the call site would make the tier
a suggestion, not a bound, and would reopen exactly the "undocumented
capability grows later" risk the typed-domain rule targets. An application
that genuinely needs a capability set no existing tier provides needs a new
tier (a decision-log entry), not a parameter.

The crate's original, untiered `Controller::delegate_device(&Did,
Capabilities)` is kept as-is (existing callers, existing tests, and any
future genuinely bespoke capability set still need it) — the tier layer is
additive, not a replacement.

## Revocation ergonomics

Two new `Controller` methods build on the existing `Seal::Revoke` +
`Kel::delegated_devices()` machinery, adding no new event kind or wire
format:

- `revoke_devices_except(&[Did])` — the common "I lost my daily device, cut
  everything except my cold root and hardware token" operation, in one
  seal event. It reads the root's own KEL for the currently-delegated
  device set (so a caller never has to track that list itself) and revokes
  everything not named in `keep`; a call where every current device is
  already in `keep` appends no event at all (checked by
  `revoke_devices_except_is_a_noop_when_nothing_needs_cutting`).
- `revoke_all_devices()` — the full-wipe form, for "assume every device I
  have ever authorized is compromised."

Both are pure policy composed from primitives that already existed
(`Seal::Revoke`, `Controller::seal`, `Kel::delegated_devices`); no new
cryptography, no new event kind.

## What this is not

- **Not a change to who holds the root's own signing key.** `ColdRoot`
  here is a *delegated device* under the existing `Seal::Delegate`
  mechanism, granted the full capability bound because of what it is used
  for (device-set recovery), not a claim that this crate's multi-key/
  threshold root keys (see `Controller::incept`'s `current_threshold`) are
  now modeled as "devices." A deployment that wants literal multi-key root
  custody (e.g. an actual air-gapped root key participating in a
  threshold) already has that via `current`/`next` threshold keys — this
  issue's tiers sit one layer up, at the capability-scoped delegation a
  root grants to *other* identifiers.
- **Not enforcement that only a device holding `MANAGE_DEVICES` may call
  `delegate_device`/`revoke_device`.** Those calls are made by the root's
  own `Controller` (signed with the root's current keys) regardless of any
  device's granted capabilities; `MANAGE_DEVICES` is a capability bit an
  application-layer authorization check can consult (e.g. "does the
  session claiming to act as this device carry `MANAGE_DEVICES`?"), the
  same as every other capability bit in this crate. Wiring that
  application-layer check is out of scope here, same as `VOTE`/`PAY`/etc.
  already are.
- **Not Sybil/personhood policy.** Exactly as the existing module doc
  states, capability scoping narrows a device, never inflates a root's
  standing; nothing here changes vote-counting (roadmap #18 remains open).

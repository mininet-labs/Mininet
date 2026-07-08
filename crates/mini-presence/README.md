# mini-presence

Range-bound, mutually-signed **co-presence attestations**: proof that two delegated
identity-root devices were physically together at a moment in time, established offline.
This is the honest core of the keystone demo — not "two internet peers signed
something," but "two delegated devices, near each other, over an encrypted link,
each proving control of an identity-rooted identity."

## What both devices sign

One deterministic transcript (`AttestationFields::transcript`) binding: the
session's **channel binding** (from `mini-bearer`), each device's `did:mini` and
**KEL digest**, fresh **nonces**, the **time window**, the **round-trip range
samples**, the **transport**, an optional fuzzed location commitment, and
optional hardware (UWB) ranging evidence (`UwbRanging`) where a platform
shell supplies it.

## What verification requires (both sides)

- the device KEL verifies and is a **delegated device of an identity root, unrevoked**,
  holding the **`ATTEST`** capability (SPEC-01 §6 → SPEC-02 presence);
- the signature verifies against the device's **current keys** (distinct-key
  threshold, shared with `did-mini`);
- the attestation is bound to **this channel** and to **fresh, non-replayed**
  nonces;
- the transport is a **proximity** bearer and the **round-trip range** is under
  policy.

The verdict names the two **identity roots** (the delegators), so the scoring layer counts
a co-presence once per identity-root pair (P2) and can discount repeats via
`PresenceVerdict::pair_key`.

## Honest limit

The RTT check is a **thresholding hook**, not a full distance-bounding protocol.
Real relay/wormhole resistance needs a tight physical-layer challenge-response
round-trip timing bound over the BLE / Wi-Fi link. With no dedicated ranging
radio (a deliberate no-radio tradeoff) this is a *software* bound, weaker than
hardware ranging, and plain RSSI is only a weak hint. This crate provides the
signed, bound, replay-checked envelope those measurements slot into.

Where a device has a UWB chip, `UwbRanging` layers a hardware-timed distance
bound on top — **additive, never a replacement**: the RTT check above always
runs regardless. `ranging::RangingSource` is the seam a platform shell fills
in to supply a real measurement (D-0034 point 1); `NoUwb` is the reference
fallback every device without a chip correctly uses today.

## Build & test

```sh
cargo test -p mini-presence
```

License: CC0-1.0 (public domain).

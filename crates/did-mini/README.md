# did-mini

Self-sovereign identity for Mininet (SPEC-01): a stable identifier you own, with
keys you can rotate, **verifiable peer-to-peer — no central registry, no required
blockchain** (SPEC-01 G8).

Built on the KERI model of autonomic identifiers (Founder Decision A2). This is
the foundation every higher layer signs against: presence attestations, the
personhood graph, forge contributions, and chain accounts are all `did:mini`
identities.

## What it provides (this batch — SPEC-01 M1 + M2)

- **Self-certifying identifiers.** `did:mini:<scid>` where `<scid>` is derived
  from the inception event — anyone recomputes it to confirm authenticity, with
  no lookup authority (SPEC-01 §3).
- **Key Event Log.** A hash-chained, append-only log of signed inception /
  rotation / interaction / seal events (SPEC-01 §4). `Kel::verify` walks it
  offline and returns the current authoritative key state, or the first
  inconsistency.
- **Pre-rotation.** Each event commits to the *hash* of the next keys; a rotation
  must reveal keys matching that commitment. A leaked current key therefore
  cannot seize control (SPEC-01 §5).
- **Device delegation (M2).** Each device is its own delegated identifier (own
  KEL + pre-rotation) whose id commits to its human-root; the root authorizes it
  with a capability set and can revoke it. `verify_delegation` proves a device
  belongs to a human — the link is *mutual*, so neither side can fake it
  (SPEC-01 §6).
- **Peer-to-peer wire format.** `Kel::to_bytes` / `from_bytes` exchange a
  verifiable identity blob between two devices with no shared state. The decoder
  caps all untrusted counts/lengths and rejects malformed SCIDs, thresholds,
  duplicate keys, invalid next commitments, unknown capability bits, and
  signature-suite ambiguity before higher layers rely on the KEL.

### Identity modes and the base device (founder decision, 2026-07-07)

`IdentityMode` names the six ways a human may show up: `HumanRoot`,
`BaseDevice`, `DeviceDid`, `PublicWall`, `PseudonymProfile`, and
`AnonymousAction`. All but the ZK-dependent `AnonymousAction` are implemented
today. `BaseDeviceRole` lets a device declare itself the user's recommended
main device for storage/seeding/participation — it is operational metadata
only, deliberately outside `Capabilities`, and cannot grant or imply a vote
(see `base_device_role_never_requires_or_implies_capabilities`).

### Pairwise pseudonyms (SPEC-01 §10)

`Controller::incept_pairwise_pseudonym(context)` deterministically derives an
independent, ordinary `did:mini` root from a human's existing root plus an
arbitrary context string (HKDF-SHA256 over the root's current key material,
never leaving the device). Same root + same context always recovers the same
pseudonym, with no extra seed storage; different contexts are unlinkable by
default. This is what makes "one human, many public/pseudonymous surfaces"
(see `mini-social::PublicWall`) a one-line call instead of hand-managing N
random seeds. See `crates/did-mini/tests/pairwise.rs`.

### Capabilities never multiply a human (P2)

Capability scoping decides *which* device may act (sign, pay, post, attest, vote);
it can only narrow a device, never inflate the human. All devices chain to one
human-root, counted once — `VOTE` designates which device casts the human's one
equal vote, it does not add a vote. This is constitution P2 encoded in the type.

## The boundary that must not be blurred

This crate makes **no claim about humanness**. A `did:mini` can be a bot; one
person can make many. did-mini solves *undercounting* (proving many devices are
one human — the delegation batch); *overcounting* (Sybils) is personhood's job
(SPEC-02). See SPEC-01 §0.

## Secret hygiene (SPEC-01 G1)

Secret keys live only in `Controller`. They never appear in any wire format here,
and `Controller`'s `Debug` redacts them. The only secret export path is
`mini-crypto`'s explicit, loudly-named on-device function.

## Not yet (later batches, per SPEC-01 roadmap)

Witnesses + duplicity detection (M3), revocation hardening + on-chain anchoring of
security-critical events (M4), social/threshold recovery (M5), pairwise
identifiers + ZK personhood linkage and selective-disclosure VCs (M6), W3C DID
Document resolution and the post-quantum rotation path (M7). Optional on-chain
anchoring lands with the chain.

## Build & test

```sh
cargo test -p did-mini
```

Tests are deterministic (fixed seeds) and run fully offline.

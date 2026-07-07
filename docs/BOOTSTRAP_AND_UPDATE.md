# Mininet self-contained bootstrap and update path

Status: accepted design target for PR-0001. Code crates still pending except the
did:mini hardening that makes peer-exchanged identity logs safe to parse.

## Goal

A person should be able to join, sync, verify releases, and receive future node
software without trusting or contacting any external service. GitHub, websites,
DNS, app stores, CDNs, package registries, and cloud buckets may mirror Mininet,
but the system must not depend on them.

The minimum physical path is **Bluetooth only**. Local Wi-Fi/hotspot/mDNS is a
speed upgrade. Internet relay is optional.

## Honest boundary

No protocol can make a phone or laptop execute code from nothing. A user needs one
of these first artifacts:

- a Mininet binary transferred by Bluetooth, USB, QR sequence, local Wi-Fi, or any
  other local means;
- a source bootstrapper plus a local compiler/interpreter; or
- an already-installed Mininet-compatible app/runtime.

After that first artifact, every future trust decision is local and cryptographic:
verify genesis, verify release registry finality, verify bundle hashes, verify
reproducible-build attestations, and verify the constitution-guard verdict.

## Genesis bootstrap capsule

A full genesis file MUST contain or directly commit to:

- `chain_id` and genesis block hash;
- constitution hash and invariant register hash;
- canonical schema descriptors for genesis and block formats;
- initial peer-card format and bearer protocol versions;
- initial release manifest CID and hash;
- minimal source/binary bootstrap bundle CID and hash;
- reproducible build recipe hash;
- initial release-verifier public keys or did:mini KEL roots;
- emergency rescue bundle hashes for offline reconstruction.

A tiny `GenesisSeed` may be transmitted first over BLE advertisements. It contains
only the chain id, genesis hash, release-manifest hash, and a peer card. The full
capsule is then requested in chunks.

## Release object

A governed release object is valid only if all checks pass:

```text
Release {
  chain_id,
  release_id,
  parent_release_id,
  source_bundle_cid,
  artifact_bundle_cid,
  build_recipe_cid,
  artifact_hashes_by_platform,
  schema_migrations,
  activation_height,
  timelock_not_before,
  constitution_guard_verdict,
  reproducible_build_attestations[],
  signer_did_or_nullifier_proofs[],
}
```

External URLs are forbidden in consensus-critical fields. A release may include a
non-authoritative `mirrors[]` hint list for convenience, but a client must treat
those as untrusted byte sources and verify hashes before use.

## Bluetooth bootstrap protocol (`MINI/BT0`)

The BLE path has four phases.

1. **Advertise:** A node broadcasts a compact peer card: protocol tag, chain id,
   genesis hash prefix, device key hash, and optional local service UUID.
2. **Handshake:** Peers open a GATT/L2CAP channel and run the anonymous
   `MINI/CH1` handshake using X25519, HKDF-SHA256, and ChaCha20-Poly1305 from
   `mini-crypto`. The handshake carries no stable identity. If a flow needs
   endpoint authentication, it happens after encryption through signed KEL/device
   payloads or a later pairwise-pseudonym channel upgrade.
3. **Identity exchange:** Peers exchange KELs, validate `did:mini` SCIDs, check
   pre-rotation chains, and optionally verify device delegation under a human-root.
4. **Chunk sync:** A receiver requests chunks by multihash: genesis capsule,
   release manifests, KELs, presence attestations, block headers, and update
   bundles. Chunks are Merkle-addressed, resumable, and store-and-forward.

Bluetooth has low throughput, so every object is chunked. A phone can gather a
release gradually across repeated short encounters. Each chunk is independently
verified against the object Merkle root before storage.

## Update adoption rule

A conforming client may present an update as adoptable only when:

1. the chain finalizes a release-registry entry for it;
2. the timelock has elapsed;
3. the constitution guard accepted the release class;
4. the source bundle hash and artifact bundle hash match the registry;
5. at least the required independent reproducible-build attestations match;
6. the local platform artifact hash matches the registry;
7. any schema migration is deterministic and locally replayable.

The client must never install code merely because a server, repository, package
registry, app store, or peer says it is current.

## No forced updates

Mininet has no off switch. An update can become the canonical protocol after its
activation height, but software must not contain a remote kill switch or a forced
silent auto-update path. A user can refuse an update, freeze their client, or join
a fork. The cost of refusal is normal protocol compatibility, not remote control.

## First implementation tasks

- `mini-crypto`: X25519, HKDF-SHA256, and ChaCha20-Poly1305 primitives for
  the session layer.
- `mini-bearer`: bearer trait, BLE adapter, local Wi-Fi/mDNS adapter, CH1 anonymous encrypted session.
- `mini-bootstrap`: `GenesisSeed`, peer cards, Merkle chunk exchange, resumable store.
- `mini-update`: release-object verifier and local adoption state machine.
- `mini-chain`: release registry and constitution-guard hook.
- `mini-forge`: content-addressed release/source storage inside Mininet itself.

## Acceptance tests

- Two phones in airplane mode exchange KELs and verify each other over BLE.
- A receiver with no internet reconstructs a genesis capsule from BLE chunks.
- A receiver rejects a release whose artifact hash differs from the chain registry.
- A receiver rejects a release with too few independent build attestations.
- A receiver continues operating on the old version if the user declines adoption.
- No test depends on GitHub, DNS, HTTP, app stores, package registries, or cloud storage.

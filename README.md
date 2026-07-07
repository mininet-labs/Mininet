# Mininet

> A population, not an organization. Fork it, build on it, run it — own it,
> together.

Mininet is a peer-to-peer network whose rules sit above its protocol: money buys
reach and storage but never a vote; governance is one verified human, one equal
vote; there is no owner, no institution, no foundation, no admin key, no off
switch, no law-enforcement backdoor, and no party that can unmask a user. The
software is public domain, built Rust-first and in-house — proven designs are
adapted into our own tree, never taken as a live external dependency. Privacy,
data sovereignty, and the right to run locally are structural.

This repository is the **self-contained Rust core**. It starts with identity
because identity works offline before any chain, server, app store, DNS record, or
website exists. The immediate demo remains simple and falsifiable: two ordinary
phones in airplane mode form an encrypted Mininet link over Bluetooth, exchange
verifiable did:mini identities, and produce a signed presence attestation.

## Current alpha honesty note

This tree is an architecture alpha/protocol-logic preview. Forge quorums, release
attestations, reward accrual, and keystone reports currently count **verified
identity roots** and delegated devices. They do not yet prove one-human-one-vote;
that requires SPEC-02 personhood and the future `PersonhoodOracle`.

Release adoption must use `verify_governed_release`; `verify_release_artifact_only`
checks only artifact/timelock/attestation facts and is not sufficient to install
software. The physical two-phone beta still needs a real BLE/local-Wi-Fi bearer,
active software RTT challenge-response, and persistent replay storage. A real Rust
toolchain pass (`cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D
warnings`, `cargo test --all --all-features`) is clean and `Cargo.lock` is
committed as of this tree.

## What makes this repo different

Mininet must be able to survive the loss of every normal internet convenience.
GitHub, DNS, app stores, package registries, websites, cloud object stores, and
CDNs are mirrors only. They are not trust roots and are not required for core
operation.

The trust root is the chain plus the content-addressed fabric:

1. **Genesis carries the bootstrap capsule.** A full genesis file contains the
   constitution hash, chain schema descriptors, the first release manifest, and
   the minimal source/binary bundle required to verify and sync the network.
2. **Updates are governed releases.** A release is valid only when the chain's
   release registry points to a content-addressed bundle, the reproducible-build
   attestations match, the timelock has elapsed, and the constitution guard has
   not rejected it.
3. **Peers distribute updates.** A node fetches release bundles from any Mininet
   peer over Bluetooth, local Wi-Fi/hotspot, optional relay, or later the storage
   fabric. There is no privileged update server.
4. **No forced updates.** A client may refuse an update and fork/exit. Protocol
   compatibility may end at an activation height, but no remote party can push
   code onto a device.

A phone still needs *some executable or source interpreter* to run code at all;
no protocol can make an operating system execute bytes from nothing. The Mininet
promise is narrower and stronger: once any person has one verified copy, they can
seed the next person using only local transport, including Bluetooth.

## Repository map

```
mininet/
├── Cargo.toml              workspace for the Rust core
├── rust-toolchain.toml     pinned toolchain for reproducible-build hygiene
├── crates/
│   ├── mini-crypto/        signatures, X25519, AEAD, HKDF, strong multihash
│   ├── did-mini/           KERI-style KEL, pre-rotation, device delegation
│   └── mini-bearer/        bearer trait + anonymous encrypted in-process channel
├── docs/
│   ├── BOOTSTRAP_AND_UPDATE.md  self-contained update + Bluetooth bootstrap spec
│   ├── ROADMAP.md               pack order from two-phone demo to full network
│   ├── DECISION_LOG.md          every stack and freeze choice with rationale
│   └── INVARIANTS.md            frozen/tunable register mapped to code
└── .github/workflows/ci.yml     temporary mirror CI until the internal forge lands
```

## Critical path

1. `mini-crypto` — cryptographic primitives, strong content addressing, and now
   Pack 1 X25519/HKDF/ChaCha20-Poly1305 session primitives.
2. `did-mini` — self-certifying identity, KEL verification, pre-rotation, M2
   device delegation.
3. `mini-bearer` — bearer trait plus anonymous encrypted sessions over an
   in-process transport, then BLE/local-Wi-Fi adapters and optional pairwise
   pseudonym authentication.
4. `mini-presence` — range-bound, co-signed presence attestation. *(shipped)*
5. `mini-reward` — deterministic, non-spendable, slowly-maturing value signal
   before the chain. *(shipped)*
6. `mini-keystone` — the composed end-to-end demo flow, one code path for CI
   (in-process) and phones (BLE / local Wi-Fi). *(shipped)*
7. `mini-bootstrap` — the self-certifying genesis/update capsule header,
   tiny broadcastable `GenesisSeed`, and chunk-exchange want-lists over
   `mini-media`. *(structural piece shipped; real BLE/local-Wi-Fi transport
   is `mini-bearer`'s job and remains pending.)*
8. `mini-update` — local adoption-state machine wrapping `mini-forge`'s
   release verification: evaluate, adopt, or explicitly refuse a candidate
   release. No forced update, no kill path. *(shipped)*
9. `mini-chain` — custom Rust chain adapting a proven Tendermint/CometBFT-style
   BFT, with equal validator power per verified identity root, never stake.
   *(finality-verification core shipped — `ValidatorSet`, `BlockHeader`,
   `Vote`/quorum-certificate verification, `Capabilities::VOTE`'s first real
   consumer; the networked consensus protocol and state machine remain
   pending.)*

See `docs/ROADMAP.md` for the full pack sequence and acceptance tests.

**Honesty note (identity root, not personhood):** the forge counts quorums in
*distinct verified `did:mini` identity roots*, not unique humans. `did:mini`
(SPEC-01) proves cryptographic identity and device delegation; the Sybil / one-
real-human problem is SPEC-02 personhood, which is not yet implemented. Until it
is, no code path here should be read as "one human, one vote" — only "one
verified identity root, one vote". The `IdentityOracle` seam is where a future
`PersonhoodOracle` will slot in.

## Constitution summary

The latest public-facing constitution has eleven principles. The first six are
from the original whitepaper; the later amendments make explicit the no-unmask,
open-participation, bot/agent, pure-humanness, and speech/reach separation rules.
In short:

1. Money never buys voice.
2. One verified human has one equal vote.
3. There is no owner, legal entity, admin key, or off switch.
4. The human share vests slowly and requires continuing human presence.
5. Privacy is structural; no component can unmask a user.
6. Users are sovereign over their own data and replication choices.
7. Nobody can be forced to participate or rejected from basic network use.
8. Bots and agents may use the network except where human proof is required.
9. Humanness proves only humanity, not conduct or reputation.
10. Content rules live in user/community filters, indexes, and blocklists.
11. Constitutional invariants are enforced as validity rules, not promises.

The canonical enforcement map is `docs/INVARIANTS.md`.

## Identity, public walls, and base devices (founder decisions, 2026-07-07)

A human's status is private, cold, and never public by default — but everyone
is free to be public, pseudonymous, or anonymous with whatever *they* choose
to publish:

- **Public profiles are voluntary "public walls,"** first-class from the start
  (`mini-social::PublicWall`). A wall is published under whatever DID the user
  picks and never carries a human-root field; publishing one requires only
  ordinary post authority and never a vote, and it never creates extra human
  status. One human may run **many** public, pseudonymous, or anonymous
  surfaces — unlinkable by default, linkable only if the user explicitly,
  voluntarily signs a linkage. See `crates/mini-social/src/wall.rs` and
  `did-mini::IdentityMode` for the full taxonomy (`HumanRoot`, `BaseDevice`,
  `DeviceDid`, `PublicWall`, `PseudonymProfile`, `AnonymousAction`).
- **One base/static device is recommended** per human — for hosting, storage,
  seeding, and participation (`did-mini::BaseDeviceRole`). It is operational
  infrastructure, not political power: it is deliberately not a capability
  and cannot buy governance weight.
- **Watching helps seed it.** Opening public content naturally helps seed it
  to the network, unless the user disables that or content policy forbids it
  — see `mini-store::CacheTier` and `Store::note_view`. Encrypted/private
  content is never advertised, no matter the policy.
- **Money, storage, and reach never buy a vote.** Storage/seeding commitment
  earns value (`mini-reward`) and reach, never voice (P1).

## Stack at a glance

- **Language:** one Rust stack for on-device core and chain.
- **Chain:** custom Rust chain adapting proven BFT finality; equal validator vote
  weight per verified human, never stake.
- **Identity:** KERI-style did:mini autonomic identifiers.
- **Networking core:** BLE + local Wi-Fi/hotspot/mDNS + optional relay;
  store-and-forward/delay-tolerant by default. Radio/LoRa is **permanently
  out of scope** — a closed founder decision (2026-07-07, D-0033), not a
  Phase-1 deferral.
- **Forge/update:** internal content-addressed forge and on-chain release registry;
  GitHub/GitLab/etc. are temporary mirrors only.

## Build & test

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
```

All three are clean on this tree and `Cargo.lock` is committed for
reproducible builds (D-0006).

## License

Public domain via [CC0 1.0](./LICENSE).

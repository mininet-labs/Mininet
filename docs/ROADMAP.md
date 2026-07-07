# Mininet roadmap: from two-phone proof to self-governed network

Status: governed planning artifact for PR-0001. This document records the build
order after the did:mini M2 hardening and the self-contained bootstrap/update
spec. It is intentionally practical: every pack must leave the repo in a smaller,
more testable, more censorship-resistant state.

## Sequencing rule

The keystone demo comes before the chain. SPEC-01 identity is ledger-independent,
so two nearby devices can exchange verifiable identities and sign presence without
waiting for consensus, settlement, governance, or the release registry. The chain
becomes necessary when value, governance, release finality, and global settlement
enter the path.

## Product target 1: the keystone demo

**Goal:** two ordinary phones in airplane mode form an encrypted local Mininet
link, exchange did:mini identities, verify device delegation, sign a mutual
range-bound presence attestation, and show local reward accrual. No internet,
DNS, app store, GitHub, server, package registry, or chain is required for the
demo logic.

The demo is not a simulation if all of these pass:

1. Both devices generate or import did:mini human-root/device KELs locally.
2. The bearer session is encrypted with fresh pairwise session keys and carries
   no public human-root identifier. Endpoint authentication, where needed, happens
   through signed payloads or a later pairwise-pseudonym channel upgrade.
3. Each side verifies the other's KEL and device delegation before accepting a
   presence signature.
4. The presence attestation binds both pairwise pseudonyms, both nonces, time,
   bearer transcript hash, and range evidence.
5. The reward-accrual stub is a deterministic pure function over verified
   attestations.
6. The full exchange runs with radios limited to Bluetooth/local connectivity.

## Pack 1 — `mini-crypto`: DH, AEAD, HKDF primitives

**Status in this PR:** started.

**Delivers:** the audited primitive set that `mini-bearer` needs for a Noise-style
channel:

- X25519 key agreement;
- ChaCha20-Poly1305 authenticated encryption;
- HKDF-SHA256 key derivation;
- suite tags for crypto agility;
- secret-redacting wrappers and deterministic negative tests.

**Way:** adapt vetted RustCrypto/dalek crates (`x25519-dalek`,
`chacha20poly1305`, `hkdf`) rather than hand-rolling cryptography. Mininet's own
code only adds suite tags, strict length checks, all-zero X25519 shared-secret
rejection, explicit secret export names, `Debug` redaction, and tests.

**Acceptance tests:**

- X25519 agreement is symmetric and non-zero;
- all-zero/small-order shared-secret result is rejected;
- public key suite/byte roundtrip works;
- HKDF derives matching AEAD keys from matching shared secrets;
- ChaCha20-Poly1305 decrypts only with the correct nonce and associated data;
- unknown DH/AEAD/KDF suite tags are rejected;
- secret debug output redacts key material.

## Pack 2 — `mini-bearer`: transport trait and encrypted sessions  ·  *started*

**Delivers:** the identity-agnostic transport layer used by Bluetooth, local
Wi-Fi, in-process tests, and optional relays.

**Way (shipped):** a small `Bearer` trait + length-prefix framing + an in-process
bearer for deterministic CI, and an **anonymous** encrypted session: an ephemeral
X25519 handshake (Pack 1) carries explicit DH/KDF/AEAD suite tags and derives
directional ChaCha20-Poly1305 traffic keys via HKDF-SHA256 plus a
channel-binding value, with the full hello transcript bound into HKDF. The
handshake carries **no identities** at all — this is the "anonymous connection,
valid payload" model (P5): the channel is not endpoint-authenticated, and
authenticity is a payload property
(self-certifying KELs, content-addressed chunks, presence signed over the channel
binding as transcript context; relay resistance comes from Pack 4
round-trip bounds, not the binding alone).

**Way (next):** an optional endpoint-authenticated variant (SIGMA/Noise-XX) keyed
by a **per-session pairwise pseudonym or delegated device key** — never the public
human-root by default — for flows that want channel-level mutual auth. Same trait,
same channel binding.

**Acceptance tests (met):** two in-process peers complete a handshake, agree on the
channel binding, derive distinct send/receive keys, exchange traffic both ways,
reject ciphertext tampering and wrong associated data, reject malformed/unknown-
suite/small-order handshakes, enforce frame-size caps before crypto, and hide
plaintext from the bearer. **Pending:** reliability/reassembly for lossy physical
bearers, replay-window enforcement for any out-of-order mode, and the
pairwise-pseudonym auth variant.

## Pack 3 — BLE/local adapters

**Delivers:** the first real physical bearer for the keystone demo.

**Way:** keep platform-specific Bluetooth code behind the Pack 2 trait. Start
with a desktop/integration adapter if mobile bindings slow the sprint, but the
protocol must preserve the mobile target: BLE advertisements, GATT/L2CAP-style
chunk channels, MTU-sized frames, resumable transfer, and short-contact recovery.

**Acceptance tests:** one device advertises a compact peer card; another finds it;
peers establish the encrypted session; a dropped connection resumes chunk sync
without accepting duplicate or corrupted chunks.

## Pack 4 — `mini-presence`: co-presence attestation  ·  *started*

**Delivers:** the strongest launch personhood signal: mutual, local,
range-bound presence.

**Way (shipped):** both devices sign one transcript binding the channel binding,
each device's `did:mini` + KEL digest, fresh nonces, the time window, round-trip
range samples, and the transport. Verification requires, for both sides, a
delegated + unrevoked + `ATTEST`-capable device of a human-root, a valid
distinct-key signature, channel binding + nonce freshness, a proximity transport,
and range under policy. The verdict names the two humans (P2), with an
order-independent pair key for discounting repeats. Needed `did-mini` additions:
`Controller::sign_message` + `Kel::verify_message`.

**Way (pending):** a tighter distance bound from BLE / Wi-Fi round-trip timing
(no ranging radio, by design — a software bound); the current RTT ceiling is a
thresholding hook, not relay-proof on its own. Plain RSSI is only a weak hint and
must never be treated as distance proof.

**Acceptance tests (met):** valid co-presence verifies and names both humans;
replayed nonces fail; revoked-device attestations fail; transcript tampering fails;
non-`ATTEST` devices fail; non-proximity transport, too-few/too-far range, and
wrong channel binding all fail. **Pending:** counterparty-diversity discounting
lives in the scoring stub (Pack 5).

## Pack 5 — local reward-accrual stub  ·  *started*

**Delivers:** visible value in the demo before the chain: a deterministic local
counter that accrues provisional reward from verified presence attestations.

**Way (shipped):** `mini-reward` — a pure function over `PresenceVerdict`s, per
human-root (P2), diversity-weighted (same-counterparty repeats halve and cap),
per-window rate-capped, and maturation-delayed before vesting (P4). No I/O, no
owned clock, fully reproducible; `accrue` for one identity root, `ledger` for all identity roots.

**Acceptance tests (met):** base accrual for both parties; diversity beats
repetition; repeat cap; per-window rate cap; maturation gating; uninvolved and
self-pairing humans accrue nothing; ledger is complete, sorted, and
order-independent.

**Deliberately not:** money (no transfer/balance/spend) and not a vote (no
governance weight — P1). The chain reward module replaces it later.

**Acceptance tests:** deterministic replay; duplicate attestations do not double
count; same-cluster/same-counterparty saturation works; revoked/tampered
attestations are ignored.

## Product target 2: Bluetooth bootstrap and self-contained updates

**Goal:** once one person has a verified copy, they can seed the next person with
genesis, identities, block headers, release manifests, and update bundles using
only local transport.

## Pack 6 — `mini-bootstrap`: `MINI/BT0` peer cards and chunk sync

**Delivers:** content-addressed, resumable chunk transfer for genesis/update/KEL
material.

**Way:** BLAKE3/SHA-256 multihash chunks, Merkle roots, request-by-hash, bounded
frame sizes, and store-and-forward queues. BLE is slow, so every large object must
assemble across many short encounters.

**Acceptance tests:** reconstruct a genesis capsule from out-of-order chunks;
reject wrong-hash chunks; resume after interruption; sync with no DNS/HTTP.

## Pack 7 — `mini-update`: local release verifier

**Delivers:** the no-external-update-authority rule in code.

**Way:** verify release-registry finality, timelock, artifact hashes,
reproducible-build attestations, schema migrations, and constitution-guard
verdicts. Peers are byte sources only. URLs are convenience hints only.

**Acceptance tests:** accept a valid release object; reject wrong hashes; reject
insufficient independent builders; reject pre-timelock adoption; continue running
old code when the user declines.

## Product target 3: personhood and governance foundations

## Pack 8 — private personhood graph and nullifier design

**Delivers:** the Sybil layer needed for one-human-one-vote and the human share.

**Way:** use presence and social evidence while keeping raw edges private. The
network should learn only verdicts, nullifiers, and challengeable commitments,
not raw location/social graphs. The target pattern is Semaphore-style nullifiers:
one verified human yields one unlinkable nullifier per context/proposal, without
revealing which human produced it.

**Acceptance tests:** one human cannot produce two valid nullifiers for the same
scope; the same human produces unlinkable nullifiers for different scopes;
revoked/non-human proofs fail; raw graph edges are absent from public objects.

## Pack 9 — `mini-chain`: custom Rust BFT and release registry

**Delivers:** value settlement, governance finality, release-registry finality,
and constitution-guard enforcement.

**Way:** adapt a proven Tendermint/CometBFT-style BFT design in Rust. Validator
vote weight is equal per verified human, never stake. Storage contribution can
affect eligibility/reward/selection probability within caps, not vote weight.
The release registry lands here so updates become governed state transitions.

**Acceptance tests:** finality under normal BFT assumptions; equal validator
power in commits; constitution-guard rejects frozen-invariant violations; release
registry entries light-client verify.

## Pack 10 — `mini-forge`: self-governed code commons

**Delivers:** the GitHub-style surface inside Mininet: issues, PRs, reviews,
bounties, signed refs, reproducible release attestations, and governed updates.

**Way:** use content-addressed Git objects and CRDT collaborative objects on the
storage fabric. Put value and decisions on-chain; keep code/social blobs off
chain. Money funds work but never grants merge authority or vote weight.

**Acceptance tests:** repo objects replicate by CID; signed refs reject forgery;
bounty release follows merge policy; governed release links source, artifact,
recipe, attestations, and timelock.

## Non-goals for the first demo

- Spendable MINI, bridge operations, treasury custody, or XRPL liquidity.
- Global DHT/gossip, onion/mix routing, or internet relay incentives.
- Final privacy-preserving graph/ZK scoring. The demo proves the local primitive;
  the full personhood system remains a research/engineering track.
- Forced update. There is never a forced update path.

## Current critical path

1. Finish Pack 1 locally with `cargo fmt`, `cargo clippy`, `cargo test`, and a real
   committed `Cargo.lock`.
2. Finish Pack 2 locally with the same fmt/clippy/test pass; the in-process
   bearer and anonymous channel are now present, but still need real compilation.
3. Pack 4: presence attestation over the in-process bearer. This is pure logic
   and can land before hardware-specific Bluetooth work.
4. Pack 5: local reward-accrual stub.
5. Pack 3: Bluetooth adapter, then run the already-tested presence flow on two
   phones in airplane mode.

When those five packs pass on two phones in airplane mode, Mininet has its first
visible fact: identity, encrypted local networking, human presence, and value
accrual without any external service.


## UI beta

The client/product layer (feed, communities, media, forge portal, web/desktop)
is planned in full in `docs/UI_BETA_PLAN.md` — epics E1–E12, 12 sprints, task
IDs and acceptance criteria for parallel teams (D-0019).

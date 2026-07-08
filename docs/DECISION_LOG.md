# Decision Log

The Phase 0/1 sprint backlog requires that *"every \[FREEZE\] choice is recorded
with rationale."* This is that log. It also records the foundational stack choices
so nothing is silently decided.

Status legend:
- **Accepted** — settled for this codebase.
- **Provisional** — adopts the documented *recommendation* from
  `Mininet_Founder_Decisions_Required.txt`, pending formal founder-cohort
  ratification. Nothing here forecloses the documented fallback.

---

### D-0001 — On-device core language: Rust; single-language stack  ·  *Accepted*
**Date:** 2026-06-30 (ratified by founder cohort) · **Refs:** SPEC-11 (reproducible
builds \[FREEZE\]), SPEC-01 G1/G6/G8, Founder Decisions A1/A2/A3, Phase 0/1 sprint.

The Phase 1 critical path (did:mini → BLE link → presence → score) is on-device
client code, and SPEC-01 G8 makes identity **ledger-independent**, so this work is
not blocked on the chain framework (A1).

Rust is chosen for the on-device core because the founder guarantees point there:
reproducible builds are a *frozen* SPEC-11 requirement and Rust is best-in-class
for deterministic output; keys-never-leave-device (G1) wants one core that binds
to iOS Secure Enclave / Android Keystore via UniFFI and also compiles to WASM.

The cohort has ratified a **single-language Rust stack** (see D-0008): the chain is
built in Rust too, not Go/Cosmos. This unifies the on-device core and the chain
into one audit surface and lets identity/personhood logic be shared rather than
reimplemented across a language boundary — directly serving the self-reliance and
"own what we must self-govern" principle.

---

### D-0002 — License: CC0 1.0 (public-domain dedication)  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** Constitution P3.

P3 requires *"an irrevocable public-domain dedication."* CC0 1.0 is the standard
instrument for exactly that, with a fallback license for jurisdictions that don't
recognize public-domain dedication. There is no owner to license the work *from*,
so a permissive-with-attribution license would misrepresent the project.

---

### D-0003 — Signature suite: Ed25519 default, behind a versioned tag  ·  *Provisional*
**Date:** 2026-06-30 · **Refs:** SPEC-01 §13 \[FREEZE\] (crypto-agility), §16, A2.

The crypto layer **must remain agile** — frozen. We satisfy this by tagging every
key and signature with a `SignatureSuite` byte; Ed25519 is the *current default*
(a tunable parameter), and a post-quantum suite (ML-DSA-65 / FIPS 204) is reserved
at wire tag `0x02` and can be added with no call-site or wire-format change. See
`crates/mini-crypto/src/suite.rs`.

---

### D-0004 — Content-address hash: BLAKE3 default, SHA-256 interop, SHA-1 forbidden  ·  *Accepted (freeze)*
**Date:** 2026-06-30 · **Refs:** SPEC-11 \[FREEZE\] (hash hardening), SPEC-01 §3.

Canonical addressing must use a strong hash and never SHA-1. Enforced
structurally: `HashAlgorithm` has only `Blake3` and `Sha256`, and
`Multihash::from_bytes` rejects the SHA-1 multicodec `0x11`. BLAKE3 is the default
for new addresses; SHA-256 is retained for the SHA-256 Git-object interop path.

---

### D-0005 — Identity foundation: KERI-style autonomic identifiers  ·  *Accepted*
**Date:** 2026-06-30 (ratified by founder cohort) · **Refs:** SPEC-01, Founder
Decision A2.

did:mini is built on KERI: self-certifying identifier from an inception event, a
hash-chained Key Event Log, pre-rotation, delegated device identifiers, witnesses
for duplicity detection, with our chain as an *optional* anchor (not a dependency).
Ratified because it is the only A2 option giving stable-ID + rotation + recovery +
**off-grid peer-to-peer verification** together, and a small self-built crate is
the purest expression of the project's no-external-dependency principle.

**M1 + M2 implemented** in the `did-mini` crate: inception, the KEL, pre-rotation,
SCID derivation, offline verification, a peer-to-peer wire format (D-0007), and
device delegation with capability scoping + revocation (D-0010). Witnesses (M3),
revocation hardening (M4), recovery (M5), and ZK linkage (M6) follow.

---

### D-0006 — Reproducibility hygiene: commit `Cargo.lock`, pin the toolchain  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-11 (verified-reproducible releases \[FREEZE\]).

`rust-toolchain.toml` pins the channel, and `Cargo.lock` must be committed by the
first maintainer environment that can run `cargo generate-lockfile`. Full hermetic
/ K-independent-builder reproducibility (SPEC-11 §8) is a later batch; this is the
cheap groundwork that doesn't surprise contributors. The exact pinned version is
tunable; pinning *something* and locking dependencies before public release is the
frozen-spirit requirement.

---

### D-0007 — did:mini wire profile: hand-rolled deterministic codec; BLAKE3 SAID-style SCID  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-01 §3/§4/§5, D-0004.

The `did-mini` crate implements a faithful but minimal KERI profile rather than a
full CESR/JSON-LD stack, to keep the security-critical identity path small and
auditable (the same reasoning as `mini-crypto`'s hand-rolled multihash):

- **Serialization:** a tiny length-prefixed binary codec (`codec.rs`), not serde.
  Each event has exactly one canonical byte layout, so digests and signatures
  computed on one device verify byte-for-byte on another — there is no separate
  canonicalisation step to get wrong.
- **SCID derivation:** `multibase(base58btc, multihash(BLAKE3, scid_input(icp)))`,
  where `scid_input` is the inception serialized with the identifier field blank
  and signatures omitted. This is the self-addressing-identifier (SAID) pattern:
  the identifier is the content address of its own inception, so it self-certifies
  with no registry (SPEC-01 §3, G8). BLAKE3 per D-0004.
- **Pre-rotation commitment:** the multihash of each *next* suite-tagged public
  key; a rotation must reveal keys whose commitments equal the prior ones, and
  is signed by those revealed (now-current) keys (SPEC-01 §5).
- **Chaining:** each event's `prior` is the BLAKE3 multihash of the full previous
  event, making the log tamper-evident end to end.

*Migration note:* this profile is wire-versioned by the suite tag and event tags;
moving to canonical CESR later (for cross-implementation interop) is an additive
change, not a breaking one, because the suite/format tags travel with the data.

---

### D-0008 — Chain framework (A1): custom Rust chain on an adapted proven BFT  ·  *Accepted*
**Date:** 2026-06-30 (ratified by founder cohort) · **Refs:** Founder Decision A1,
SPEC-05 (MINI chain), SPEC-06 (XRPL bridge), constitution P1/P2/P3.

The cohort ratified building the MINI chain **ourselves in Rust**, rather than
taking a live dependency on the Cosmos SDK / Go (A1 Option 1) or Substrate as an
external framework. Governing principle: *own what encodes our values and must be
self-governed* (the state machine, the freeze boundary, equal-weight-per-human and
personhood gating), and *adapt proven open-source designs* — vendored into our own
tree and governed through the internal forge (SPEC-11) — for the parts where
novelty is risk without value.

Specifically: adapt a **Tendermint/CometBFT-style BFT** for instant deterministic
finality (target ~1–3 s, meeting the "settlement speed like XRP" goal) rather than
inventing a new consensus algorithm. Validator power is **equal weight per verified
human** (P1/P2), not stake — money never buys voice. XRPL is an **external
settlement bridge only** (SPEC-06), never our consensus.

*Why not from-scratch consensus:* for a money-bearing, ownerless, century-scale
base, correctness and auditability dominate novelty; a proven BFT design adapted
into our tree gives self-governance without re-deriving safety/liveness proofs.

*Sequencing / risk:* identity and presence are ledger-independent (SPEC-01 G8), so
the Phase-1 keystone demo does **not** depend on this; the chain can harden in
parallel without blocking the two-phone demo. The specific Rust BFT engine to
adapt (e.g. an Informal-Systems-style Tendermint core) is selected when the chain
crate begins.

---

### D-0009 — Networking core (A3): mesh + local Wi-Fi + optional relay; drop radio  ·  *Accepted*
**Date:** 2026-06-30 (ratified by founder cohort) · **Refs:** Founder Decision A3,
SPEC-03 (connectivity overlay), Phase 0/1 keystone demo.

The cohort ratified a connectivity core built on **our own bearer abstraction**
over adapted-proven plumbing, and **dropped LoRa/radio** as a core concern. Bearers
are: **BLE**, **local Wi-Fi / hotspot** (mDNS discovery), and an **optional
internet relay**. Routing is **mesh + store-and-forward / delay-tolerant**: nodes
sync opportunistically and may "refresh" and submit payloads to the wider network
**periodically** rather than maintaining constant connectivity.

**Founder decision (2026-07-07, reaffirmed in D-0033): radio/LoRa is
permanently out of scope**, not merely deferred past Phase 1. This is a
closed question, not an open one to revisit as the network scales — the
connectivity core stays BLE + local Wi-Fi/hotspot/mDNS + optional internet
relay + store-and-forward/delay-tolerant sync, indefinitely.

The bearer trait is the load-bearing commitment — it keeps every transport
swappable, so no single bearer (and no single upstream project) is ever
load-bearing. Proven pieces (authenticated-encryption channel design, gossip/epidemic
broadcast, mDNS) are adapted behind that trait, not adopted as a heavyweight
framework lock-in.

*Sequencing:* the keystone two-phone demo needs only the bearer trait + the CH1
anonymous encrypted channel over BLE/local Wi-Fi (no internet). Wider-network
gossip/DHT and the relay + DTN layer build on the same trait afterward.

---

### D-0010 — Device delegation model (M2): delegated inception + mutual seal  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-01 §6 (human-root and device keys), P2.

"Many devices, one human" (SPEC-01 G3) is implemented so that the human↔device link
is **mutual and unforgeable from either side**:

- A device is its **own delegated identifier** (`dip`): a normal `did:mini` with its
  own KEL and pre-rotation whose SCID **commits to its delegator** (the human-root).
  A device therefore cannot silently change which human it claims.
- The human-root authorizes the device by anchoring a **`Delegate` seal** (device id
  + capability set) in its **own** KEL, and revokes via a **`Revoke` seal**. The
  root's history is thus a tamper-evident record of which devices it authorized and
  when.
- `verify_delegation` requires **both**: the device's `dip` names the root *and* the
  root's KEL carries an unrevoked `Delegate` for the device. Neither a root claiming
  someone else's identifier, nor a device claiming an unwilling root, passes.

**Capabilities only narrow, never multiply (P2).** The capability bitset (sign, pay,
post, attest, vote, manage-devices) scopes *what a device may do*; it cannot create
extra votes or extra standing. All devices chain to one human-root, counted once;
`VOTE` merely designates which device casts the human's single equal vote. Secure
defaults: `primary()` excludes device-management; `secondary()` also excludes
voting (SPEC-01 §6).

*Note:* full KERI delegated-rotation/anchoring semantics (a device rotation that the
root co-anchors) and on-chain anchoring of revocations are later hardening (M4); the
mutual `dip` + seal model satisfies the M2 acceptance — prove several devices are
one human, with capability scoping and revocation — offline and today.


### D-0011 — Self-contained genesis and self-updating release registry  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-11 release registry, SPEC-04 content-addressed
fabric, self-contained-ledger amendment.

Core participation must not rely on GitHub, DNS, app stores, websites, package
registries, cloud buckets, or any other external service. Those may mirror the
project, but they are never trust roots and never required for sync or update.

A valid genesis file carries a **bootstrap capsule**: chain id, constitution hash,
state/schema descriptors, the initial release manifest, reproducible-build recipe,
and enough source/binary material to verify and join the network. After genesis,
updates are **governed release objects**: a proposal points to content-addressed
source and artifact CIDs, independent build attestations, the activation height,
and a constitution-guard verdict. A client accepts a release only if chain finality,
timelock, artifact hash, and build attestations all verify locally.

Large blobs belong in the Mininet storage fabric, but release manifests and rescue
bundles must be reconstructable from the chain and peer caches. External forges and
websites are convenience mirrors only.

[FREEZE] No conforming client may treat an external service as an update authority.

---

### D-0012 — Bluetooth-only bootstrap is a mandatory launch path  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-03 keystone demo, SPEC-01 G8, Phase 0/1 sprint.

The network must be able to **start from only a nearby signal**. The minimum launch
path is Bluetooth: one device advertises a `MINI/BT0` peer card, another connects,
exchanges did:mini KELs, verifies the peer, and can request missing genesis or
release chunks by content hash. Local Wi-Fi/hotspot/mDNS is a speed upgrade, not a
requirement.

This is not magic execution: a device must already have a way to run a Mininet
binary, source bootstrapper, or compatible runtime. The guarantee is that no URL,
app store, DNS record, Git host, or central relay is required once one verified
copy exists nearby.

The BLE bootstrap protocol is delay-tolerant: chunks are Merkle-addressed, resumable,
and store-and-forward. A phone may acquire a release across many short encounters.

[FREEZE] The identity exchange and genesis/update chunk protocol must function over
Bluetooth with no internet path.

---

### D-0013 — did:mini wire hardening before first public release  ·  *Accepted*
**Date:** 2026-06-30 · **Refs:** SPEC-01 §3/§4/§5/§6, D-0007, D-0012.

Before any public compatibility promise, the did:mini decoder is hardened as a
security boundary:

- KEL and event decoders cap untrusted counts and length-prefixed fields before
  allocation.
- Establishment events reject empty key sets, zero/oversized thresholds, duplicate
  public keys, empty/invalid next commitments, duplicate next commitments, and
  malformed commitment multihashes.
- Threshold verification counts distinct public keys, not merely distinct signature
  indexes.
- `did:mini` parsing requires a canonical multibase strong multihash SCID.
- Supported multihash algorithms must carry their canonical digest length.
- Delegation capabilities reject unknown future bits.
- Full-form event signatures carry their own suite tag. This is a pre-public wire
  profile correction: compatibility is not yet promised, so the safest wire format
  wins now.

These checks make Bluetooth exchange safe against malformed peer input before the
transport layer starts handing KELs around opportunistically.

---

### D-0014 — Pack 1 crypto primitives for Bluetooth/local encrypted sessions  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-03 transport/session crypto, D-0012,
`docs/ROADMAP.md` Pack 1.

The next build pack after did:mini M2 is the smallest primitive layer needed by
`mini-bearer`: **X25519** for Diffie-Hellman, **HKDF-SHA256** for traffic-key
schedule derivation, and **ChaCha20-Poly1305** for AEAD frame encryption. These
are adapted from vetted RustCrypto/dalek crates rather than implemented from
scratch.

Mininet-owned code adds the rules that express our invariants:

- every DH, AEAD, and KDF primitive has a stable suite tag, matching the existing
  signature-suite agility pattern;
- all wire-facing byte constructors enforce exact suite lengths;
- X25519 all-zero shared-secret results are rejected;
- secret and shared-key types redact `Debug` output and zeroize local buffers on a
  best-effort basis;
- HKDF output length is capped before allocation, per RFC 5869;
- deterministic tests cover the handshake primitives before any BLE adapter exists.

This is not yet a full Noise implementation. It is the auditable primitive base
that Pack 2 composes into the bearer handshake.

---

### D-0015 — Bearer layer and anonymous encrypted channel (`mini-bearer`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-03 keystone demo, D-0009 (bearers), D-0014
(Pack 1 primitives), constitution P5.

The connectivity core is split into a dumb, identity-free **bearer** (moves opaque
frames; BLE / local Wi-Fi / relay all implement one trait) and an encrypted
**channel** over it. The channel does an ephemeral X25519 handshake whose hello messages carry
key-agreement, KDF, and AEAD suite tags, then derives ChaCha20-Poly1305 traffic
keys via HKDF-SHA256 (Pack 1), binding the full hello transcript into HKDF. This
gives confidentiality, forward secrecy, and a channel-binding value.

**The handshake carries no identities.** The connection is anonymous and
unlinkable; a passive observer sees only ephemeral public keys. This encodes
"anonymous connection, valid transaction" (P5): the channel is *not*
endpoint-authenticated by design, and authenticity is a payload property —
self-certifying KELs, content-addressed chunks, and presence attestations whose
signed transcript includes the channel binding. The binding is necessary context,
but it is not a complete anti-relay proof by itself; Pack 4 must add mutual nonces,
a transcript hash, and a round-trip distance bound. Endpoint pseudonym
authentication (a SIGMA/Noise-XX step keyed by a per-session pairwise pseudonym)
can layer on later without changing the crate's shape.

This batch ships the trait, length-prefix framing, an in-process bearer for CI, and
the channel with tests. Frame-size caps are enforced before allocation/crypto,
small-order X25519 handshakes are rejected, and derived traffic-key material is
scrubbed after splitting. Real BLE / local-Wi-Fi adapters are the one component
that must be built and tested on real devices; they sit behind the same trait.

[FREEZE] No bearer may carry a stable identity in the clear, and no channel may
require revealing a human-root identity to open it.

---

### D-0016 — Presence attestations (`mini-presence`): range-bound, mutually signed  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-02 (presence primitive), SPEC-03 (keystone
demo), D-0010 (delegation), D-0015 (channel), constitution P2/P5.

Co-presence is attested by a transcript **both devices sign**, binding the session
channel binding, each device's `did:mini` + KEL digest, fresh nonces, the time
window, round-trip range samples, and the transport. Verification requires, for
both sides: the device is a delegated, unrevoked, `ATTEST`-capable device of a
human-root (D-0010); the signature verifies against the device's current keys
(distinct-key threshold); the attestation is bound to the observed channel and to
fresh (non-replayed) nonces; the transport is a proximity bearer; and the range is
under policy. The verdict names the two **humans**, so the scoring layer counts a
pairing once per human pair (P2) and can discount repeats.

New did-mini surface (small, reused hardened logic): `Controller::sign_message`
(detached signing; secrets never leave the device) and `Kel::verify_message`
(detached verification against current key state, counting distinct public keys via
the shared `count_valid_signers`).

[FREEZE] Presence must be **range-bound**: a non-proximity (relay/internet)
transport can never evidence co-presence. The RTT check is a thresholding hook; a
complete distance-bounding protocol (BLE / Wi-Fi round-trip timing — no ranging
radio, so a software bound) is required before relay resistance is claimed; RSSI
alone is only a weak hint (SPEC-02).

---

### D-0017 — Reward accrual stub (`mini-reward`): slow, diversity-weighted vesting toward MINI  ·  *Accepted, corrected 2026-07-08*
**Date:** 2026-07-01 · **Refs:** SPEC-03 (demo value), constitution P1/P2/P4.

Verified presence becomes visible value through a **pure function** over
[`PresenceVerdict`]s — no I/O, no owned clock, fully reproducible. Accrual is per
**human-root** (P2), **diversity-weighted** (repeats with one counterparty halve
and cap; distinct counterparties pay full), **rate-capped per window**, and
**matures on a delay** before vesting (P4: slow, presence-conditioned).

**Correction (2026-07-08, whitepaper §8.3 confirms — see D-0035):** an earlier
version of this entry called the stub "not money," describing it as a demo
counter the chain reward module would later replace with a separate real
currency. That was wrong. The whitepaper is explicit: there is **one**
currency, MINI, and "a large genesis tranche represents the present value of
the world and is distributed as each verified human's slowly-vesting share,
conditioned on continuous human presence." `RewardAccount`'s accrual and
maturation *is* that vesting mechanism, not a stand-in for it — `vested_points`
are, in a full implementation, literally spendable MINI. Nothing about the
accrual math changes; what changes is what the numbers *mean*.

[FREEZE, unchanged and now more load-bearing, not less] Whatever this value
becomes spendable as, it carries no governance weight, ever. `RewardAccount`
has no vote-weight field today and must never grow one; MINI balance and
voting eligibility are permanently separate axes (P1, whitepaper §3 "the
central separation: voice and value"). This is the wall the whitepaper calls
"the single decision that makes the whole vision hold" — it is not this
crate's business to enforce spend/transfer (that is the future MINI ledger's
job, `mini-value`/`mini-chain`, D-0034/D-0035), but it is this crate's business
to never let accrual imply or carry a vote.

Note: diversity-weighting and caps only *blunt* farming; they are not Sybil
resistance, which remains personhood's job (SPEC-02, now specified in detail
by the whitepaper's three-signal design — see D-0035).

---

### D-0018 — Keystone harness (`mini-keystone`): the beta is the composed flow  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-03 keystone, D-0015/16/17, constitution P1/P2/P4/P5.

The beta deliverable is defined as one composed, testable flow (`run_demo`):
anonymous channel → encrypted KEL exchange → offline mutual identity + delegation
verification (requiring `ATTEST`) → mutually-signed range-bound presence, verified
independently by each side against its own channel binding and replay guard →
per-human, diversity-weighted, maturing accrual. The flow is generic over the
`Bearer` trait, so the CI run (in-process) and the phone run (BLE / local Wi-Fi)
are the *same code path* — the physical adapter is the only difference.

Audit fixes folded in with this batch: presence verification now rejects
**self-presence** (both devices of one human — P2: presence is evidence of two
humans meeting) and is **side-effect-free until fully valid** (two-phase replay
guard: a failed verification can no longer burn nonces).

---

### D-0019 — UI beta stack: one Rust core, Flutter shell, WASM web, SPEC-09 objects  ·  *Proposed (founder veto open)*
**Date:** 2026-07-01 · **Refs:** SPEC-09 (one object model, every surface), SPEC-11
(forge/self-update), D-0008 (own values / adapt proven), docs/UI_BETA_PLAN.md.

All product logic stays in the one Rust core; UI layers are thin renderers.
Bindings: UniFFI (Kotlin/Swift/desktop) + wasm-bindgen (web). UI: Flutter
(BSD, pinned in-tree) via flutter_rust_bridge for Android/iOS/desktop; PWA on the
WASM core for web (relay bearer only — browsers cannot do our BLE). Storage:
BLAKE3 content-addressed blobs + vendored SQLite indexes. Mutable state: our own
minimal signed op-log CRDT (SPEC-09 §3) — owned because it encodes authorship and
one-human semantics. Alternatives (React Native, native shells, Tauri) recorded in
the plan with reasons.

[FREEZE carried into UI] One object model across all surfaces; reach ranking is a
user-chosen client-side filter, never a hidden algorithm (SPEC-09 §5); moderation
is labels/filters, never central deletion (SPEC-10); no forced update or remote
kill path (D-0011/P3); money never buys merge authority (SPEC-11).

---

### D-0020 — Sovereignty-first UI & distribution stack  ·  *Accepted (founder directive)*
**Date:** 2026-07-01 · **Supersedes:** D-0019 (Flutter proposal — rejected: Google-
governed toolchain and non-reproducible builds conflict with the directive and the
frozen SPEC-11 reproducibility requirement). **Refs:** docs/UI_BETA_PLAN.md §2.

**Founder directive: nobody able to forbid, censor, block, or kill it — above
every other consideration.** Consequences:

- **Distribution IS the network.** Binaries are content-addressed release objects
  synced peer to peer (D-0011/D-0012). Android sideload APK is the canonical
  mobile path; app stores are optional mirrors; no canonical domain; no push
  services; no Google Play Services, ever.
- **UI stack has no framework owner.** Desktop reference client in pure-Rust egui
  (MIT, vendored); Android as a thin, logic-free Kotlin/Compose shell over UniFFI;
  web as the egui-WASM mirror over relays; iOS as a best-effort SwiftUI shell,
  documented honestly as the least sovereign platform (Apple's gate, not ours) and
  never on the critical path.
- **Reproducibility is a sprint-1 gate** (E1.S3.T3): two independent builders must
  produce bit-identical artifacts before later sprints proceed.
- **Cost accepted knowingly:** ~2 sprints more UI work and functional-not-native
  polish, traded for a client no company can deprecate and no store can kill.

[FREEZE] No canonical distribution point may ever be required: a fresh device must
be able to obtain, verify, and run the client from a nearby peer alone.

---

### D-0021 — Unified object envelope (`mini-objects`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-09 §2–4, D-0020, UI plan E2.S1.

One envelope for every surface: extensible type, human+device authorship,
timestamp/sequence, typed links, public-or-encrypted payload, device signatures;
content-addressed by BLAKE3 multihash over canonical bytes (multibase, SCID-style;
IPLD-CID byte interop is a later additive mapping). Verification is layered:
integrity (keyless), authenticity (device KEL, distinct-key threshold), provenance
(delegation + capability — `POST` required for content types, so a stolen
SIGN-only device cannot speak as its human). Untrusted decode is bounded before
allocation. [FREEZE] All surfaces use this one model; no per-surface format may
ever be introduced.

---

### D-0022 — Local store & head pointers (`mini-store`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-09 §3, SPEC-04 (content addressing), D-0021,
UI plan E2.S2.

Persistence over a small `Backend` trait (memory for tests; filesystem with
atomic writes and traversal-hardened keys for devices; SQLite later behind the
same trait). Deterministic author/type/link indexes; `want_list`/`missing_links`
seed sync (E3). Mutable state = SPEC-09 signed head pointers implemented as
ordinary `HEAD` objects (added to `mini-objects`, `POST`-capability-gated):
last-write-wins by (sequence, then greatest id) so all replicas converge in any
arrival order, and a head can only move its own author's slot. Trust boundary
documented explicitly: the store verifies integrity by construction; signature +
provenance verification is the ingest pipeline's obligation before insertion.

---

### D-0023 — Op-log CRDT (`mini-crdt`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-09 §3, SPEC-10 (moderation as filters),
D-0021/D-0022, UI plan E2.S3.

Multi-author mutable state = append-only logs of signed `CRDT_OP` objects (Add /
Edit / Tombstone) replayed by a pure, order-independent fold: Adds are set
membership (orphans pend until their parent arrives), Edits are per-node
last-write-wins by `(sequence, op id)`, Tombstones are monotone. Convergence is
by construction and permutation-tested. Hostile or malformed ops are
deterministically excluded and reported — one bad op can never poison a thread.

[FREEZE-aligned] Edit/tombstone authority belongs to the node's **human** (any of
their delegated devices), never to another human's device: moderation operates
through filters/labels (SPEC-10), never by rewriting someone's words. Tombstones
are honest retractions, not claimed erasure (P6: bytes that left a device may
persist elsewhere; the protocol does not pretend otherwise).

---

### D-0024 — Replication & verified ingest (`mini-sync`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-03 (local-first), SPEC-04 (content
addressing), D-0009/D-0012/D-0020/D-0022, UI plan E3.

Pull-based, strictly alternating MINI/SYNC1: bucketed set reconciliation,
byte-budgeted batches, hard message limits, resume by idempotence (no transfer
session state to corrupt — the store-and-forward model). Identities replicate as
ordinary `mini/kel` carrier objects whose embedded KELs self-certify; extensions
of a known log are accepted, conflicting histories refused (duplicity surfaces to
the witness layer, SPEC-01 M3, later).

[FREEZE-aligned] The ingest pipeline is the trust boundary: nothing enters a
store without integrity + signature + full provenance against cached KELs, and
content from unknown authors is rejected outright — a peer must supply the
identity that signed what it offers. Sync runs only inside the encrypted channel;
the transport learns nothing (P5). What replication inherently reveals to the
*chosen peer* is stated, not hidden.

---

### D-0025 — Social graph & explainable feeds (`mini-social`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-09 §3/§5/§6.1, D-0021/D-0022, UI plan E4/E5.

Profiles = `PROFILE` objects resolved via signed heads; follows = `FOLLOW`
objects with per-(follower, target) LWW by `(sequence, id)`; the feed = a pure,
locally computed view over the store: viewer's own posts plus followed authors',
chronological with deterministic tiebreaks, truncation-stable.

[FREEZE] Ranking is a user-chosen, explicitly passed filter — never a hidden
algorithm. Filters are total orderings: they reorder, never silently drop
followed speech (personal blocklists are the user's own explicit choice, safety
layer E9). Every feed item carries a `FeedReason`, so "why am I seeing this" is
answerable by construction (speech-vs-reach, SPEC-09 §5).

Honest note: the public follow graph is public — derivable by anyone from public
objects. Pseudonymous graphs come with pairwise identifiers (SPEC-01 §10, M6);
until then the client must not imply graph privacy.

---

### D-0026 — Chunked media (`mini-media`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-04/SPEC-09 §6.1, D-0020/D-0024, UI plan E7.

≤1 MiB content-addressed chunk objects + one ordered manifest (content type,
total length, whole-payload BLAKE3 digest). Assembly re-verifies the digest, so
manifests cannot lie; chunks ride ordinary sync in any order — progressive,
interruption-proof, nothing restarts. Manifests double as the forge's artifact
carrier. Honest limits recorded: ~256 MiB per manifest (nesting later);
nearby-first + relay, not a CDN.

---

### D-0027 — Forge core & release verification (`mini-forge`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-11, D-0011/D-0020/D-0026, UI plan E8.

Repos = signed content-addressed objects (file blobs, nested trees that *link*
their entries so one commit id pulls a whole repo over sync, commits with
parents); branches = signed heads. Releases name version, source commit,
artifact manifest, artifact + recipe digests. `verify_release_artifact_only` checks only the artifact layer: timelock,
complete digest-checked artifact, and ≥N **independent** attestations.
Adoption requires `verify_governed_release`, which additionally binds the source
commit to the governed canonical branch head and refuses governance forks.

[FREEZE] Current alpha attestations are counted per verified identity root: one root's many devices
count once. SPEC-02 `PersonhoodOracle` upgrades this to verified humans later; the release author's own attestation never counts, and no balance
appears anywhere in the rule — money never buys release or merge authority
(P1/SPEC-11). [FREEZE] The verifier only verifies: no execution, no remote
trigger, no forced-update or kill path exists in this module (P3/D-0011).
Provisional label: until `mini-chain`, the attestation rule stands in for chain
finality; the chain replaces the counting, never the object formats.

---

### D-0028 — Merge governance & self-amending maintainer chain (`mini-forge::governance`)  ·  *Accepted*
**Date:** 2026-07-01 · **Refs:** SPEC-11, D-0027, constitution P1/P2/P3, UI plan
E8.S6–S7.

Projects declare a maintainer set + `min_approvals`; anyone may propose a PR
(open participation); approvals are **bound to the exact head commit reviewed**
(no bait-and-switch); merges and policy **amendments** are entries in one
hash-linked governance chain, each judged against the policy *as of the previous
entry* (self-amending, forward-only power, no owner key — P3). PR discussion
rides `mini-crdt` with the PR object as the doc root.

[FREEZE] Current alpha merge quorums are counted in **distinct verified identity roots from the
maintainer set**: no balance, stake, or payment appears anywhere in the rule
(money never buys merge — P1/SPEC-11); one identity root's many devices count once and
the PR author's own approval never counts (P2 + independent review). Competing
valid entries resolve deterministically (greatest id) **and set
`forks_detected`** — an honest, labeled-provisional tiebreak until `mini-chain`
finality replaces it; the chain replaces the counting, never these objects.

---

### D-0029 — External review integrated: hardening pass (Batches 7A/7B/7C-core)  ·  *Accepted*
**Date:** 2026-07-02 · **Refs:** external static review (architecture-alpha verdict),
D-0027/D-0028, SPEC-11.

Blockers closed, each with regression tests:
**7A (forge validity).** Policies validated (`min_approvals ∈ 1..=maintainers`,
no duplicate maintainer humans; invalid amendment policies never apply). Merge
lineage enforced: a PR must target this project, its `base` must equal the entry
it merges onto (no stale-base or cross-project merges), and its head must exist
and be a real commit. **Provenance re-binding:** every decision-relevant object
(genesis, chain entries, PRs, approvals, releases, attestations) must re-pass
full provenance against a caller-supplied `AuthorOracle` of verified KELs at
decision time — a store polluted outside verified sync cannot influence
governance. **Release↔governance binding closed:** `verify_governed_release`
enforces the full chain — release source commit == the canonical branch head
resolved through valid identity-root-quorum merges — and **refuses adoption on any
governance fork** (`ForkDetected`; display may use the provisional tiebreak,
installing software may not).
**7B (storage/media/sync).** `Store::get` verifies fetched bytes derive the
requested id (backends can never substitute content). Media manifests are
strictly decoded with hard allocation caps; assembly aborts early on
over-declared chunks. Trees are canonical: unique, strictly-ascending valid
names, strict flag bytes, no trailing bytes; checkout is budgeted (files, bytes,
depth, cycle guard). Sync drops **unsolicited** objects (only what was asked for
is ingested) under a per-pull byte budget; KEL carriers must embed *their own
author's* log — wrapping a third party's KEL is refused.
**7C (presence, part).** `max_age_ms` bounds how old an attestation may be, so
replay windows are finite across restarts; `ReplayGuard` is documented as the
durable-persistence interface. Active challenge-response ranging remains open,
honestly labeled (software RTT hook only).

Still open from the review (tracked): active range measurement, per-device
persistent replay store, standalone CLI harness (7D), git SHA-256 interop,
`Cargo.lock` + full toolchain pass (requires a Rust environment), external
crypto review.

---

### D-0030 — Second review integrated: compile-readiness + honest quorum semantics  ·  *Accepted*
**Date:** 2026-07-03 · **Refs:** second external static review (6.1), D-0029,
SPEC-01/SPEC-02, SPEC-11.

- **Compile blocker fixed:** removed the duplicate `AuthorOracle` import in
  `mini-forge` (kept the public re-export; import only `author_verified`).
- **Genesis policy validated on decode:** `resolve_project` now runs
  `valid_policy` on the decoded project object (strict, no trailing bytes) — a
  hand-crafted signed project cannot smuggle a zero-approval or duplicate-
  maintainer set past resolution.
- **One canonical strict parser each** for PR (`parse_pr_payload_strict`) and
  release payloads, used by both validation and application, with exact-EOF and
  valid-name enforcement — canonical branch state never depends on loose parsing.
- **Adoption footgun closed:** `verify_release` → `verify_release_artifact_only`,
  documented as *not sufficient for adoption*; `verify_governed_release` is the
  only adoption-safe gate and now enforces `ReleasePolicy::validate_for_adoption`
  floors (`ADOPTION_MIN_ATTESTATIONS`, `ADOPTION_MIN_TIMELOCK_MS`) — no
  zero-attestation or zero-timelock adoption is possible.
- **Honest quorum semantics [wording FREEZE]:** `AuthorOracle` → `IdentityOracle`.
  The forge counts **distinct verified identity roots**, NOT humans. `did:mini`
  (SPEC-01) proves identity + delegation; personhood (SPEC-02) is unimplemented,
  so nothing is described as "one human, one vote" yet. A future
  `PersonhoodOracle` wraps `IdentityOracle` at the same seam. Docs/README/code
  comments corrected accordingly.
- **`KelDirectory::try_insert_verified`** verifies a KEL (self-certifying) before
  indexing and refuses conflicting forks (extensions only); `insert` kept for
  already-verified inputs and documented as such.
- **KEL carrier provenance completed:** a device carrier's own object signature
  must verify against its embedded device KEL before absorption (not just DID
  match) — no misleading/unsigned envelopes pollute indexes.
- **`FsBackend::put_blob` repairs corrupt existing blobs** (compares bytes;
  atomically rewrites on mismatch) instead of trusting a stale local copy.

Still open (require a real environment or later specs; tracked in BETA_STATUS):
active challenge-response range, persistent replay store, KEL freshness/
revocation anchoring, personhood (SPEC-02), standalone CLI (7D), `Cargo.lock` +
toolchain pass, external crypto/supply-chain review. Value/treasury/bridge
surfaces remain out of scope and gated on counsel.

---

### D-0031 — Third review integrated: truth-sync + root-carrier provenance + repo self-description  ·  *Accepted*
**Date:** 2026-07-03 · **Refs:** third external static review (7.0), D-0029/D-0030,
SPEC-01/SPEC-11, review issues #1/#2/#3/#7.

D-0030 recorded a set of fixes that the tree had only *partially* applied. This
batch makes the code match the log, closes the one genuinely-open provenance
hole, and adds offline navigability. No `cargo` was available, so this remains a
static batch — the toolchain pass below is still required.

- **Additional compile-readiness fixes** (neither catchable without a compiler):
  `crates/mini-forge/tests/forge.rs` imported the renamed-away `verify_release`
  (call sites already used `verify_release_artifact_only`); and
  `mini-sync::KelCache::absorb_carrier` called `obj.verify_signature(kel)` by
  value where the method borrows `&Kel`.
- **Residual identity-root wording scrubbed [wording FREEZE, per D-0030].**
  Current-code descriptions in `mini-forge` (`lib.rs`, `governance.rs`, tests),
  `mini-presence` (`lib.rs`, `verify.rs`, `error.rs`), `mini-keystone` tests,
  `did-mini::delegation` header, the forge README, and `UI_BETA_PLAN` now say
  *distinct verified identity roots*, never *humans*. Constitutional **target**
  references (P2) are kept only where explicitly labeled as the target; the root
  README's honesty note already scoped those correctly and was left as-is. Public
  field/param names (`initiator_human`, `author_human`, `human: &Did`) are left
  unchanged and tracked as a compiler-gated rename — a half-applied cross-crate
  rename without a compiler is worse than consistent names with corrected docs.
- **KEL root-carrier envelope provenance closed (review #3).** D-0030 closed the
  *device*-carrier case. `absorb_carrier` now returns three ways — envelope
  verified / KEL-only / rejected. A root-only carrier's self-certifying KEL is
  still absorbed (identity is useful), but the **object** is indexed only if its
  signing device is known and `verify_provenance` holds now; otherwise it stays
  transport-only and never pollutes authorship indexes. `IngestOutcome` gains
  `AcceptedKelOnly`; `pull` runs a **two-pass** ingest so a root carrier whose
  device arrives in the same batch still indexes on the second pass, while true
  orphans do not. Regression test:
  `orphan_root_carrier_is_absorbed_but_not_indexed`. Existing sync invariants
  (`carriers == 2`, store-id equality) are preserved under both processing orders.
- **Self-describing repo (review #7).** Restored `tools/mininet_nav.py` (stdlib
  only: `map` / `deps` / `crate` / `search`) and generated
  `docs/_generated/{REPO_INDEX.json,REPO_MAP.md}`, plus `docs/NAVIGATION.md` and
  `docs/SELF_DESCRIBING_REPO.md`. The generated map is a lens, not an authority:
  source and Constitution win on any disagreement.

Still open (unchanged, require a real environment or later specs; tracked in
BETA_STATUS): **`Cargo.lock` + `cargo fmt`/`clippy -D warnings`/`test --all`
cannot run here** and gate first publish; active challenge-response range;
persistent replay store; KEL freshness/revocation anchoring; personhood
(SPEC-02); standalone CLI harness (7D); external crypto/supply-chain review.
Value/treasury/bridge surfaces remain out of scope and gated on counsel.

---

### D-0032 — Parser strictness, artifact-only verifier rename, KEL cache caps, and repo-map restoration  ·  *Accepted*
**Date:** 2026-07-03 · **Refs:** SPEC-01/SPEC-04/SPEC-11, D-0030/D-0031, Batch 7C review.

This batch closes the remaining static-review findings before the first real
Rust toolchain pass:

- `verify_release_artifact` is now `verify_release_artifact_only`, making the
  public API name itself say that it is **not** adoption-safe. Adoption remains
  gated only by `verify_governed_release`.
- Governance genesis decode now rejects malformed project names as well as bad
  policies and trailing bytes.
- Approval payloads and governance-chain entries now use strict canonical parsers:
  approvals are exactly `{approve, reviewed_object_id}`, merge entries are exactly
  one byte, and amendment entries are exactly `{amend, policy}`. Trailing bytes are
  rejected instead of ignored.
- `mini-sync` adds KEL cache and per-pull KEL-carrier caps so a hostile peer cannot
  grow identity state without bound merely by sending many valid but irrelevant KELs.
- Current alpha wording is tightened around identity-root semantics in presence,
  reward, keystone, forge, and beta docs. `did:mini` still proves identity and
  device delegation; `PersonhoodOracle` is the future layer that upgrades this to
  one verified human.
- The richer offline navigation tool is restored: `build`, `map`, `search`,
  `symbols`, and `files`, with JSON/JSONL generated indexes.

Still required before publication: real `cargo fmt`, `cargo clippy --all-targets
--all-features -- -D warnings`, `cargo test --all`, `cargo generate-lockfile`,
then commit `Cargo.lock`.

### D-0033 — Founder decisions batch: public walls, base device, seed-on-view, 2-approval floor, radio/Cosmos closed for good  ·  *Accepted*
**Date:** 2026-07-07 (ratified by founder cohort) · **Refs:** SPEC-00 P1/P2/P6,
SPEC-09 §6.1, SPEC-11 §2, D-0009, D-0025, D-0028, D-0030.

Six founder decisions, locked and implemented in this batch:

1. **Public profiles are first-class "public walls."** `mini-social::PublicWall`
   (`ObjectType::WALL`) is a voluntary public identity surface published under
   whatever DID a user chooses — a primary root or an independent pseudonym
   root. It carries no human-root field, requires only `POST` capability
   (never `VOTE`), and is never auto-registered anywhere. The **only** way to
   bind a wall to another identity is an explicit, self-signed
   `publish_wall_linkage` (`ObjectType::WALL_LINKAGE`) — absent by default.
   Tests: `crates/mini-social/tests/social.rs`.
2. **No preservation duty for now-contradictory Cosmos/radio docs.** Superseded
   language is rewritten in place, not kept "for history" — `docs/DECISION_LOG.md`
   itself is the history.
3. **Protocol-repo approvals: 2 for now.** `mini-forge::governance::PROTOCOL_MIN_APPROVALS
   = 2` and `valid_policy_for_protocol_repo` reject any protocol-critical policy
   below that floor — no 1-of-1 canonical merge path. Mirrors the existing
   `ADOPTION_MIN_ATTESTATIONS = 2` release-attestation floor. This upgrades to
   personhood-root quorum once SPEC-02 lands; it is a floor, not a ceiling.
4. **Radio/LoRa is permanently out**, not merely deferred past Phase 1 (amends
   D-0009's framing — see that entry). The connectivity core stays BLE + local
   Wi-Fi/hotspot/mDNS + optional internet relay + store-and-forward/DTN sync.
5. **One base/static device is recommended per human**, for hosting, storage,
   seeding, and participation — `did-mini::BaseDeviceRole` (storage commitment,
   relay, seed-on-view, availability window, bandwidth limit, battery policy,
   privacy mode). This is *advisory only*: it is deliberately not a
   `Capabilities` bit and cannot grant governance weight (P1) — a human may run
   zero or many. Tests: `crates/did-mini/tests/identity_modes.rs`.
6. **Seed-on-view: watching helps seed, unless disabled or policy forbids it.**
   `mini-store::CacheTier` (`EphemeralCache`, `SeedCache`, `CommittedStorage`,
   `PrivateOnly`, `PinnedByOwner`) and `Store::note_view` promote public content
   toward `SeedCache` only when the device's `BaseDeviceRole` policy, battery,
   metered-connection, and storage-budget checks all allow it. Encrypted
   content is never promoted past `PrivateOnly`; `note_view` takes no viewer
   identity (opening content cannot mutate identity state); pinned/committed
   tiers are never downgraded by a view. Tests: `crates/mini-store/tests/cache.rs`.

Also formalized: the identity-mode taxonomy (`did-mini::IdentityMode` —
`HumanRoot`, `BaseDevice`, `DeviceDid`, `PublicWall`, `PseudonymProfile`,
`AnonymousAction`), documenting which are implemented today and which remain
`pending` (only `AnonymousAction`, gated on SPEC-02's `PersonhoodOracle`).

None of the above changes P1/P2: money and infrastructure commitment still
never buy a vote, and human status is still private and exactly one per human.

---

### D-0034 — Ranging (UWB), uniqueness graph, P2P networking (libp2p), and privacy-preserving value transfer: four founder decisions, phased  ·  *Accepted (founder directive, phased implementation)*
**Date:** 2026-07-08 (founder cohort) · **Refs:** SPEC-02 (personhood, unimplemented),
D-0008 (own what must be self-governed, adapt proven pieces), D-0009/D-0016
(bearers, presence honest limits), D-0033, constitution P1/P2/P5.

Same governing principle as D-0008/D-0009: **own the parts that encode our
values and must be self-governed; adapt proven, audited open-source designs —
into our own tree, never as a live framework dependency — where novelty is
risk without value.** Four decisions, each phased rather than landed in one
batch, because each opens a new crate:

1. **Ranging: native UWB where available, software RTT elsewhere.**
   Amends D-0016's honest limit, not D-0009's radio freeze — these are
   different things. D-0009/D-0033 forbid a *long-range communications radio*
   (LoRa/mesh radio) as a network bearer; UWB here is a *short-range ranging
   sensor* already inside commodity phones (iPhone U-series, many Android
   flagships), used only to tighten the existing two-device distance bound
   inside an already-established presence session — it carries no traffic and
   is never a bearer. Devices without UWB keep the current software RTT bound
   as a fallback; this is additive, not a replacement. Platform bridging
   (native UWB APIs are not reachable from pure Rust) fits the existing
   UniFFI-shell architecture (D-0020): the Rust core defines the ranging
   trait/result type, each platform shell supplies the UWB measurement.
   **Implemented 2026-07-08:** `mini_presence::UwbRanging` carries the
   measurement as part of the signed transcript (tamper-evident once
   signed); `RangePolicy::max_uwb_distance_cm` is an optional tighter bound
   `verify_presence` enforces only when both the policy and the evidence are
   present, alongside the RTT check, never instead of it;
   `ranging::RangingSource` is the platform seam, with `NoUwb` as the
   permanent, correct reference implementation for chip-less devices — no
   real UWB adapter exists in this repo, the same honest-limit shape as
   `mini-bearer`'s still-pending real BLE adapter. Tests:
   `crates/mini-presence/tests/presence.rs` (four new cases: absence is a
   no-op, in-policy acceptance, out-of-policy rejection even when RTT alone
   would pass, and tamper detection on the signed UWB field).

2. **Uniqueness/personhood: a custom in-house uniqueness graph — not a raw
   trust list, not an outside oracle, not biometrics.** Founder guidance: the
   co-presence attestations `mini-presence` already produces are each user's
   *own* trusted contact list — real, useful, but not by themselves a
   network-wide uniqueness proof (a user's own list is exactly what a sybil
   would also produce). SPEC-02 personhood is built as our own graph-based
   uniqueness algorithm layered on top of that attestation data (edges =
   mutually-signed, range-bound co-presence pairs, per D-0016), not by
   integrating an existing proof-of-personhood project (rejected: puts a
   third party's graph/servers in the trust path) and not by biometrics
   (rejected outright by P5: no raw personal data). **This is a design task
   before it is a coding task** — the graph algorithm itself (how uniqueness
   is scored/decided from an attestation graph, and how it resists sybil
   clustering) needs its own written design pass, most likely its own SPEC
   section, before a `mini-uniqueness` crate is built. Marked `pending`,
   highest design risk of the four.

3. **Wide-area networking: our own DHT + gossip, borrowing libp2p's proven
   *designs*, not taking libp2p as a dependency.** D-0009 already anticipated
   "wider-network gossip/DHT and the relay + DTN layer" as a later addition
   on top of the bearer trait; this decision picks which *algorithms* that
   layer is built from — a Kademlia-style routing table for peer discovery,
   and an epidemic/gossipsub-style pub/sub broadcast for propagation — and
   reimplements them as Mininet-owned code in a new `mini-net` crate, the
   same "own what must be self-governed" stance as D-0008/D-0009: no
   heavyweight external framework becomes load-bearing plumbing our peers
   depend on, and no upstream project's release cadence, governance, or
   supply chain sits on our critical path. This is a correction from an
   earlier draft of this entry, which had proposed a live `libp2p` crate
   dependency — founder guidance was explicit that "open-source tech that
   works" (the Monero/Ripple/libp2p references) means *study and adapt the
   design*, not depend on the project's code. **Scope boundary, so this
   cannot quietly become an identity leak:** the transport-routing peer id
   this layer generates is ephemeral, session-scoped, and never derived from
   or bound to a `did:mini` root — it must never become a stable
   cross-session identifier, which would undermine the anonymous-channel
   invariant [FREEZE] in D-0015. `mini-net` sits behind the same kind of
   narrow trait `mini-bearer` uses, so the wider-network relay stays
   swappable and never load-bearing for trust.

4. **Value transfer: a real spendable-value layer, Monero-style primitives,
   built in-house.** `mini-reward` (D-0017) stays exactly as frozen — non-
   spendable, no governance weight, never money buying voice (P1). This is a
   *separate* layer for real transfers (e.g. paying for storage service,
   tipping), explicitly not the reward/accrual system. Ring signatures,
   stealth addresses, and RingCT-style confidential amounts are the chosen
   primitives — reimplemented in our own `mini-crypto`-adjacent crate rather
   than vendoring Monero's codebase, matching this project's existing pattern
   of owning the primitive layer while following a proven design. **[FREEZE]
   Same P1 boundary applies here as everywhere else: no balance, stake, or
   payment in this layer may ever appear in a vote/quorum/access rule, and no
   key in this layer may unmask a user.** Highest engineering-risk item of
   the four (real value, real cryptography, real loss-of-funds surface) —
   primitives ship with extensive test coverage first; no real-value
   deployment before an external crypto review, mirroring the caution already
   recorded for signature/hash agility (D-0003/D-0004).

Sequencing (lowest-risk/most-scoped first, since all four are independent of
each other and of the existing keystone critical path): `mini-net` first
(mechanical, well-understood, unlocks real multi-device testing for
everything else) — then ranging, then the uniqueness-graph design pass, then
value transfer last (highest risk, most deliberation needed before code).

---

### D-0035 — Whitepaper reconciliation: MINI unification, three-signal personhood, hybrid consensus, human-only crypto core — and one open contradiction  ·  *Accepted where noted; one item explicitly OPEN*
**Date:** 2026-07-08 · **Refs:** `Mininet_Whitepaper.pdf` v1.0 (founding document,
received this date), D-0008/D-0009/D-0017/D-0033/D-0034.

The founding whitepaper was shared for the first time this session. It is the
senior document — the constitution's own six principles are drawn from it —
so this entry reconciles it against decisions already made in this log.
Most of it *confirms* what was already built or decided; one item
*contradicts* an already-"closed" decision and is called out as genuinely
open, not resolved here.

**1. MINI is one currency; reward accrual is literally its slow-release
mechanism (confirms and corrects D-0017 — see that entry's inline
correction).** Whitepaper §8.3: "a large genesis tranche represents the
present value of the world and is distributed as each verified human's
slowly-vesting share, conditioned on continuous human presence." This *is*
`mini-reward`'s accrual/maturation design, not a demo stand-in for it.
`RewardAccount` was mischaracterized as "not money" — corrected in place.
What does not change, and matters more now that the numbers are confirmed to
be real value: MINI balance and voting weight are permanently different axes
(whitepaper §3, "the central separation: voice and value" — the wall the
whitepaper itself calls load-bearing for the entire project).

**2. Personhood has a specified design; D-0034 point 2's "left to us"
framing is superseded.** Whitepaper §5 specifies three fused signals: (a) a
social-vouching graph (~100 non-clustered genuine connections, graph-
community analysis to discount Sybil-farm clusters — a known technique
family, e.g. SybilRank-style trust propagation); (b) on-device behavioral/
location entropy, proved in zero-knowledge so raw sensor data never leaves
the device — explicitly named the most research-intensive, not-yet-shipped-
anywhere component; (c) physical-presence attestation — **exactly what
`mini-presence` already implements**, named the strongest signal because two
devices cannot be in two places at once. Confidence decays over time and
must be continuously re-earned; value/governance unlock only as confidence
stays high across months, not at a single verification moment. Cold start is
a diverse founding cohort vouching for each other in person, diluting
rapidly as the graph grows, with **no extra vote for being early** (P2 still
holds). `mini-uniqueness` (task pending) now has a real spec to build
toward — signal (c) is done, signal (a) is a graph algorithm with prior art,
signal (b) is genuine unsolved-elsewhere R&D.

**Implemented 2026-07-08 (signals a and c; signal b deliberately stubbed):**
`mini-uniqueness::vouch`/`verify` build mutual, signed vouch attestations
between identity roots (mirroring `mini-presence`'s two-party pattern, minus
the proximity requirement — vouching may ride any transport). `graph::VouchGraph`
records them as an undirected graph; `graph::trust_scores` is a from-scratch,
integer-only reimplementation of SybilRank's bounded power-iteration shape,
propagating trust outward from a seed set so a Sybil cluster's internal
edges don't help it — only edges into the trusted region do (test:
`a_sybil_cluster_with_one_bridge_edge_scores_far_below_the_honest_region`).
`confidence::fuse_confidence` combines the vouch-graph score and a caller-
supplied presence-strength score (signal c, from `mini_presence::PresenceVerdict`)
with per-signal time decay into one 0..=100 confidence value — weights and
the decay curve are an explicitly tunable first cut, not whitepaper-specified.
Signal (b) is `confidence::BehavioralEntropySource`, a seam only:
`NoEntropySource` (always `None`) is the correct, permanent implementation
until the human-authored, externally-audited proof this crate cannot build
exists (D-0035 point 5). 18 tests across `mini-uniqueness`.

**3. Consensus is a hybrid, not flat equal-weight-per-human as D-0008 alone
implied.** Whitepaper §8.1: block-production weight comes from **proof-of-
space-time** (concave reward curve + per-identity caps + geographic/network-
diversity bonuses, so doubling capacity yields less than double reward —
storage weight without letting storage buy governance), while **finality is
anchored by a committee *sampled* from high-confidence verified humans**.
`mini-chain`'s current skeleton (`ValidatorSet`, equal weight per verified
identity root, `QuorumCertificate`/`verify_finality`) is the *finality-
committee* half, reasonably faithful once sampling-from-personhood-pool
replaces today's identity-root stand-in. The **proof-of-space-time block-
production half does not exist yet** — new consensus work, not a small
addition, tracked as a new task rather than folded silently into D-0034's
existing sequencing.

**4. `mini-value` builds Monero-style privacy for the *one* MINI ledger, not
a second currency.** D-0034 point 4's "separate spendable-value layer" wording
is corrected by point 1 above: there is one currency. `mini-value`'s job is
transaction-privacy primitives (ring signatures, stealth addresses, RingCT-
style confidential amounts) for `mini-chain`'s ledger — Bitcoin and Monero
appear in the whitepaper as **external assets contributed to a community
treasury in exchange for freshly issued MINI at a community-governed rate**
(§8.2, "how the rich contribute" — permissionless, no seller, contributor
gets value, zero extra voice), a *separate*, currently unbuilt mechanism
(treasury custody, price governance, BTC/XMR receipt verification) from
transaction privacy. Tracked as a new task, not conflated with `mini-value`.

**5. [FREEZE, new, explicit] Human-only authorship + external audit for the
highest-stakes cryptographic components.** Whitepaper §8.1 (hybrid
consensus), §9/treasury custody, and §11 ("the cryptographic privacy core is
written by humans, reviewed by humans, and audited externally, never
delegated to automated tooling") state this as a founder requirement, not a
suggestion. This project has, to date, had an AI author every crate
including `mini-crypto`, `did-mini`, and `mini-presence`'s cryptographic
logic, with external crypto review already tracked as a standing open item
(D-0030/D-0031) — consistent in spirit, but the whitepaper makes it
explicit and specifically names the hybrid consensus, treasury custody, and
the personhood behavioral/location ZK proof as requiring human authorship
before real deployment, not merely human review after the fact. Recorded
here so it governs how `mini-uniqueness`'s signal (b), the proof-of-space-
time consensus half, `mini-value`, and treasury custody get built: AI-authored
code for these four may exist as a prototype/reference implementation and
ships with that label, but is explicitly **not** a substitute for the
human-authored, externally-audited version the whitepaper requires before
real value or real personhood proofs depend on it.

**6. New, smaller items noted for future tasks, not acted on in this entry:**
onion-style multi-hop relay routing where "relays earn MINI for carrying it"
(whitepaper §6) — `mini-bearer`'s current channel is direct two-party, not
multi-hop, and `mini-net`'s gossip is flood-broadcast, not onion-routed
unicast; an influence/rank system from referrals and contribution that
"unlocks more ways to take part in running things" but **never adds vote
weight** (§10) — nothing in the tree today; and a "reach floor" guarantee
that no quantity of dislikes can fully bury a post (§8.4) — `mini-social`'s
ranking is a user-chosen filter (D-0025 [FREEZE]) but does not yet guarantee
a floor against a dislike-heavy filter choice.

**7. [RESOLVED 2026-07-08 by founder cohort] LoRa/radio: D-0033 wins.**
Whitepaper §6 lists "long-range low-power radio in the LoRa family" as one
of the overlay's interchangeable bearers, on equal footing with Bluetooth
and Wi-Fi — directly conflicting with D-0009/D-0033's *"radio/LoRa is
permanently out of scope... a closed question, not an open one to revisit."*
Put to the founder cohort directly rather than resolved by inference: the
whitepaper's bearer list is **aspirational v1.0 language, not binding** —
D-0009/D-0033 were made with real engineering-cost information the
whitepaper draft didn't have, and stand as written. Radio/LoRa remains
permanently excluded from `mini-bearer`/`mini-net`; this whitepaper mention
is a known, deliberate divergence between the founding document and the
implemented protocol, not an oversight, and should read that way if the
whitepaper is ever revised.

# Decision Log

The Phase 0/1 sprint backlog requires that *"every \[FREEZE\] choice is recorded
with rationale."* This is that log. It also records the foundational stack choices
so nothing is silently decided.

## Scope rule (added D-0045 batch, per founder review)

**This log records policy decisions only.** What's actually built, and how
far along it is, belongs in `docs/STATUS.md` — a living document, updated
as often as the tree changes. An entry's own "Implementation status" field
(see template below) is a snapshot at the time the decision was made, not
a substitute for `STATUS.md`; if the two ever disagree, `STATUS.md` wins,
because it's revisited far more often than any individual entry is.

This scope rule applies **going forward**. Entries before this rule
(everything prior to D-0045) predate the template below and are not
retroactively reformatted — this log is itself an append-only historical
record (see D-0034's "no preservation duty for now-contradictory docs,"
which applies to *other* documents' language, not to this log rewriting
its own past). Where an old entry needs tightening or correction, a new
entry supersedes it explicitly, the same way D-0045-D-0048 supersede/
tighten D-0037/D-0039/D-0041 below, rather than editing history in place.

## Entry template (D-0045 onward)

```
### D-00xx — Title  ·  *Accepted*
**Date:** ... · **Refs:** ...

**Decision:** what was decided, stated plainly.
**Reason:** why, in a sentence or two — the full reasoning can be longer,
but the one-line version should stand alone.
**Constitutional impact:** which principle(s)/invariant(s) this touches,
strengthens, or is constrained by. "None" is a valid, common answer.
**Implementation status:** a snapshot — real detail lives in `docs/STATUS.md`.
**Failure point:** the concrete way this could go wrong if the decision's
assumption stops holding. "None identified" is a valid answer, but should
be rare for anything touching a frozen domain.
**Required follow-up:** what has to happen next, if anything, and who/what
roadmap issue owns it.
**Supersedes / superseded by:** explicit links both directions, so the
history stays navigable without reading the whole log linearly.
```

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

**Design pass done 2026-07-08 (`mini-spacetime`), split by risk class —
mirroring the same split `mini-uniqueness` made for its own novel-crypto
signal:** the *scoring formula* (given already-proven capacity, how much
should it weigh) is ordinary deterministic arithmetic and is fully
implemented: `weight::proposer_weight` capped-then-square-rooted (concave:
doubling capacity yields ~1.41x weight, verified by test, never 2x) with a
bounded per-region diversity bonus, all integer (`isqrt`, from-scratch
Newton's method) for exact reproducibility. The *cryptographic proof itself*
— genuinely holding that capacity over a challenge period — is **not**
attempted: `proof::ProofOfSpaceTimeSource` is a seam only, `NoProof` its
correct permanent stand-in, per the whitepaper's own words ("the most
demanding engineering in the value layer... implemented human-only and
externally audited") and point 5 below. Structurally kept apart from
`mini-chain`: `proposer_weight` returns a plain `u64` with no shared type
with `ValidatorSet` and no path to `Capabilities::VOTE` — storage capacity
can make a node likelier to *propose* a block, never make a vote count for
more (P1, unchanged). Proposer rotation/leader-election, the state machine,
and networking remain unbuilt, the same boundary `mini-chain` already
states for its own half. 9 tests.

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

**Design pass done 2026-07-08 (`mini-treasury`), same risk-class split as
`mini-uniqueness` and `mini-spacetime`:** the *bookkeeping and arithmetic*
around a contribution are ordinary and fully implemented —
`rate::RateHistory`/`mint_amount_micro` (a governed exchange-rate lookup and
the multiplication that turns a contribution into a minted amount, all
integer for exact reproducibility), `receipt::ContributionReceipt` (the
record of a claimed contribution), and `signers::TreasurySignerSet`/
`meets_threshold` (who is authorized to approve treasury actions and
whether enough distinct authorized identities agreed — mirroring
`mini-forge`'s governance approval-counting pattern exactly: no weight
field, no path from signer membership to extra voting power, P1 unchanged).
The *actually security-critical* pieces are explicitly **not** attempted,
per the whitepaper's own words ("bridge and treasury custody is a permanent
honeypot by nature," §11) and point 5 below: `receipt::ExternalReceiptOracle`
(verifying a real Bitcoin/Monero transaction actually paid the treasury —
real cross-chain engineering) and any real threshold-signature scheme over
actual funds (`meets_threshold` answers "did enough people agree," never
"here is a valid signature the treasury would accept" — no such scheme
exists in this crate). `NoExternalReceiptOracle` is the correct, permanent
stand-in until human-authored, externally-audited work exists. 11 tests.

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

**All four now design-passed 2026-07-08 under this same rule, the last
(`mini-value`) built most conservatively of all, per founder direction:**
`mini-value::fee` (the whitepaper §8.4 fee mechanism — governed price
history and the arithmetic converting a real-world value target to a MINI
amount) is ordinary bookkeeping, fully implemented and tested, same shape
as `mini_treasury::rate`. The three actual privacy primitives —
`ring::RingSignatureScheme`, `stealth::StealthAddressScheme`,
`confidential::ConfidentialAmountScheme` — are seams only, each with a
`NoX` stub that **fails closed**: none of `NoRingSignature`,
`NoStealthAddress`, or `NoConfidentialAmount` will sign, derive, commit, or
verify anything as valid, so an absent real implementation can never be
mistaken for a working one. This closes D-0034's four-item sequence: every
item has either shipped in full (`mini-net`, UWB ranging) or shipped its
safe half with the genuinely novel cryptography honestly stubbed
(`mini-uniqueness`, `mini-spacetime`, `mini-treasury`, `mini-value`) — the
human-authored, externally-audited work this point requires remains
entirely ahead of this tree, not begun by proxy through any of these stubs.

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

---

### D-0036 — Founder override: AI-authored ring-signature/stealth-address prototype for `mini-value`, ahead of external audit  ·  *Accepted (explicit founder override of D-0035 point 5)*
**Date:** 2026-07-08 · **Refs:** D-0035 point 5, whitepaper §5/§8/§11, D-0014.

D-0035 point 5 recorded the whitepaper's own requirement that the highest-
stakes cryptography — the hybrid consensus, treasury custody, the
personhood ZK proof, and (this project's own extension) MINI's
transaction-privacy primitives — be **written by humans and audited
externally**, not AI-authored code with founder review standing in for
that. The founder cohort was asked directly whether to hold that line or
override it for this specific piece of work, with the tradeoff stated
plainly. **Explicit founder decision: override, for now, for
`mini-value`'s ring signatures and stealth addresses.** AI-authored code,
reviewed by the founder and Michal, proceeds as a real (not stubbed)
prototype. This is **not** a quiet retreat from D-0035 point 5's standard —
that FREEZE stands unchanged for the other three areas (hybrid consensus,
treasury custody, personhood ZK proof) and for `mini-value`'s own
confidential-amounts primitive, none of which this override touches. It is
a scoped, named exception, recorded so the gap between "founder-reviewed
prototype" and "human-authored, externally-audited" is never confused with
each other going forward: **this code must not be treated as production-
ready or as satisfying D-0035 point 5 merely because it now exists and
passes tests.** A specialized external cryptography audit remains a
precondition before any real value depends on it.

**Build approach, per founder direction ("use known tech and code but
build everything custom"):** the raw elliptic-curve group (Ristretto over
Curve25519, via `curve25519-dalek` — the same audited crate
`ed25519-dalek`/`x25519-dalek` already build on, D-0014's established
exception for primitive-layer math) is depended on rather than
reimplemented; the actual stealth-address and linkable-ring-signature
*protocols* — key derivation, challenge/response construction, key-image
linkability — are Mininet-owned code referencing published designs
(CryptoNote-style stealth addresses; MLSAG-style linkable ring signatures,
the pre-CLSAG/Bulletproofs Monero scheme, chosen for being the simplest
correctly-documented linkable ring construction and because amount-hiding
is `mini-value::confidential`'s separate, still-fully-stubbed concern).
Ristretto (not raw Edwards/Curve25519 points) is used specifically to
avoid the cofactor-related subtle-bug class that ad-hoc protocols built
directly on Edwards points are prone to.

**Implemented 2026-07-08.** `stealth_impl::MininetStealthAddress`: the
CryptoNote Diffie-Hellman construction (`P = H(rB)*G + A`, recognized via
the symmetric `H(bR)`), with `derive_spend_scalar` completing the round
trip (a recipient can actually reconstruct the one-time spending key, not
just recognize the output) — kept as a separate function from the
recognition trait since recognizing (view secret only) and spending (view
+ spend secret) are deliberately different privilege levels. `ring_impl::MininetRingSignature`:
the AOS/MLSAG Fiat-Shamir hash-chain construction described above, with a
deterministic key image `I = x*Hp(P)` for double-spend linkability. Both
use BLAKE3's extendable-output function for hash-to-scalar/hash-to-point
(wide 64-byte reduction, avoiding bias), and both fail closed on malformed
input (wrong-length keys, empty rings, out-of-range indices) rather than
panicking. Tests cover the real cryptographic properties, not just plumbing:
stealth — recipient recognizes their own output, an outsider does not, two
outputs to the same recipient are unlinkable on the wire yet both
recognized, and the derived one-time key actually opens the one-time
address; ring signature — a valid signature verifies regardless of which
ring position was real, a tampered message/response/decoy each
independently fail verification, the same real key produces the same key
image across two different signings (double-spend detection) while two
different real keys never collide. 32 tests total in `mini-value`.

**Still explicitly a prototype, not a substitute for external audit**
(this override does not extend to `mini-value::confidential`, the hybrid
consensus, treasury custody, or the personhood ZK proof — all four remain
governed by D-0035 point 5 unchanged).

---

### D-0037 — Founder policy change: D-0035 point 5 generalized to "human review, AI authorship permitted"  ·  *Accepted (supersedes D-0036's narrow scope)*
**Date:** 2026-07-08 · **Refs:** D-0035 point 5, D-0036.

D-0036 overrode D-0035 point 5 narrowly, for two named primitives in
`mini-value`. Asked directly whether to keep re-litigating this per
primitive or set a standing rule, **the founder cohort set a standing
rule**: across all four D-0035 point 5 areas (hybrid consensus, treasury
custody, the personhood ZK proof, and MINI's transaction-privacy
primitives), the bar is now **human review, with AI permitted to author
the code** — not human authorship with AI shut out, and not requiring a
specialized external audit before further work proceeds. D-0036's
narrower per-primitive override is superseded by this general rule, not
separately tracked going forward.

**What this changes:** AI-authored implementations of well-documented,
existing cryptographic designs (confidential amounts/range proofs,
proof-of-space-time challenge-response, treasury threshold-signature
custody) may now proceed the same way `mini-value`'s ring signatures and
stealth addresses did — built, tested, and shipped as founder-reviewed
prototypes — without waiting on a specialized external audit as a
precondition to keep building. An external audit remains desirable before
any of this carries real value or real personhood determinations in
production; it is no longer treated as a hard gate on development
continuing.

**What this does *not* change [important distinction, not a loophole]:**
this is a policy about *who reviews and who authors*, not a claim that
every remaining gap is now merely a process question. The personhood
behavioral/location ZK proof (whitepaper §5, signal (b)) is explicitly
described by the whitepaper itself as unsolved research — "has not yet
been shipped anywhere" — which is a **research-feasibility** blocker, not
an authorship-policy one. Relaxing who may author code does not make an
unsolved cryptographic research problem solved; that signal stays
unimplemented until a real, sound construction exists to author in the
first place, regardless of who is permitted to write it.

---

### D-0038 — Founder redesign: open-ended, multi-signal `HumanStatus` accumulator, replacing reliance on any single silver-bullet signal  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** D-0037, whitepaper §5/§11, `mini-uniqueness::status`.

Asked what to actually do about the personhood behavioral/location ZK
proof being unsolved research (D-0037), the founder cohort didn't pick one
of the offered options — they redirected the design itself: instead of a
fixed three-signal fusion where one signal (behavioral entropy) is a
research-grade unsolved problem, `mini-uniqueness::status` generalizes
personhood into an **open-ended set of independently-weighted
verification methods**, each optional, all adding up:

- `SignalSource` is extensible (`External(u32)` catch-all) rather than a
  closed enum of exactly three — new verification methods, including ones
  not yet designed, can contribute without a breaking change.
- `TrustWeights` encodes "us trusting our own the most": Mininet's own
  physical-presence and vouching-graph signals default to the highest
  weight; anything external starts low-trust, tunable by governance.
- `HumanRecord` accumulates evidence (a derived score + timestamp per
  source — never raw data, P5 unaffected) and computes one fused,
  decayed score.
- Promotion is two-stage and asymmetric on purpose: `VouchedHuman` is a
  fast path reachable from modest trusted evidence (e.g. one genuine
  vouch) so onboarding isn't blocked; `FullHuman` is reachable **only
  automatically**, requiring a high fused score, several currently-live
  *distinct* sources, and a minimum elapsed time since the record's first
  evidence — never from stacking one very strong signal, and never
  faster than the mandatory age floor regardless of score.

**Why this is a real answer to the Sybil-cost problem, not a workaround:**
no individual signal needs to be unbreakable. A farm must satisfy several
independent methods, each with its own real-world cost, keep them from
decaying, and wait out the minimum age — the same "by the time a fake
operation is profitable it is nearly indistinguishable from genuine
adoption" property the whitepaper states (§11), generalized from three
fixed signals to as many as the network ends up supporting. This does
**not** solve signal (b)'s underlying research problem (D-0037 stands:
that stays unimplemented until a real construction exists) — it makes the
*system* not depend on any one signal being unbreakable, behavioral
entropy included. `confidence::fuse_confidence` (the original fixed
three-signal fusion) is unchanged and still correct for what it does;
`status` is the generalized model going forward. 9 new tests.

---

### D-0039 — `mini-spacetime`: Merkle/PDP storage proof implemented as the honest interim scheme  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** D-0037/D-0038, whitepaper §7/§8.1, `mini-spacetime::storage_proof`.

Founder direction on the proof-of-space-time question: start with the
simpler, well-documented construction now, treat full proof-of-replication
as a separate, later, dedicated project. Implemented:

- `merkle::MerkleTree`/`MerkleProof` — a domain-separated Merkle tree
  (RFC 6962-style leaf/node prefix separation) over stored blocks.
- `storage_proof::verify_storage_challenge` — a challenge response must
  supply the *actual* block bytes, not just re-assert an already-published
  digest, so answering requires genuinely holding the data.
- `storage_proof::ProofHistory`/`StorageWindowPolicy` — repeated
  successful responses, without too large a gap, over a real span of time
  (month-scale default) before capacity counts as currently proven; a
  stale most-recent response invalidates the whole streak, and letting the
  gap run out demotes proven capacity back to `None`.
- `storage_proof::MerkleStorageProof` implements
  `proof::ProofOfSpaceTimeSource` for real, tying commitment + history +
  policy together.

**Explicitly, not implicitly, a partial answer.** This scheme proves
*continuous possession*, not *replication uniqueness* — it cannot
distinguish a thousand honest small devices each holding their own copy
from one well-resourced server answering every challenge from a single
copy, which is exactly the warehouse-consolidation attack the whitepaper's
egalitarian "thousand cheap machines beat one warehouse" thesis (§7)
depends on resisting. Real proof-of-replication (Filecoin-style
sequential/time-locked encoding) is the construction that closes that gap
and remains explicitly deferred, not silently dropped. 26 tests total in
`mini-spacetime` (up from 9).

---

### D-0040 — `mini-value`: Bulletproofs range proof implemented for confidential amounts  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** D-0035 point 4, D-0036/D-0037, whitepaper §8, `mini-value::{bp_generators,bp_ipa,bp_range,confidential,confidential_impl}`.

Founder direction: build the full Bulletproofs range proof, not a
placeholder. Implemented:

- `bp_generators` — deterministic, hash-derived (nothing-up-my-sleeve)
  generators for the commitment/proof system (`blinding_generator`,
  `value_generator`, `ipa_generator`, 64-long `g_vec`/`h_vec`), all
  independent of `curve::basepoint()` (the signing-key generator) so the
  commitment scheme never shares a discrete-log relationship with
  anything signature-related.
- `bp_ipa` — the generic inner product argument: recursive halving with
  Fiat-Shamir challenges compressing an `O(n)` opening to an `O(log n)`-
  size proof (`InnerProductProof`), with `prove`/`verify`.
- `bp_range` — the full single-value range proof (`prove_range`/
  `verify_range`/`RangeProof`): bit decomposition enforcing
  `value ∈ [0, 2^64)`, blinded vector commitments `A`/`S`, the folded
  polynomial `t(X) = <l(X), r(X)>`, commitments `T1`/`T2`, and the IPA to
  compress the opening.
- `confidential`/`confidential_impl::MininetConfidentialAmount` — the
  `ConfidentialAmountScheme` trait redesigned around the real protocol
  shape (`commit_with_proof`/`verify_range_proof`/`verify_balance`), and
  its implementation. `verify_balance` needs no extra proof: Pedersen
  commitments are additively homomorphic
  (`C(v1,b1) + C(v2,b2) == C(v1+v2,b1+b2)`), so balance is exactly an
  elliptic-curve point-sum equality check on the commitments themselves.

Two load-bearing algebraic identities were hand-derived and checked
term-by-term before writing any code, rather than trusted from memory of
the paper: the IPA folding relation
(`P_{i+1} = P_i + u_i²·L_i + u_i⁻²·R_i`), and the range-proof constant-term
relation (`t0 = value·z² + delta(y,z)`, with
`delta(y,z) = (z-z²)·Σyⁱ - z³·Σ2ⁱ`). Both are documented in `bp_range`'s
module docs. This produced zero test failures across all four new/changed
modules on first implementation. 59 tests total in `mini-value` (up from
32); debug builds are noticeably slower for the range-proof tests
(unoptimized big-integer curve arithmetic) — confirmed correct and fast
(well under a second for the whole suite) under `cargo test --release`.

**[FREEZE reminder — D-0036/D-0037 still applies.]** This is a founder-
reviewed, AI-authored prototype, not an external-audit-equivalent. Range-
proof soundness is exactly the class of property with no safe middle
ground between "provably correct" and "silently exploitable" — treat
`MininetConfidentialAmount` as pending a specialized cryptography audit
before any real value depends on it, same bar as the stealth-address and
ring-signature prototypes before it.

---

### D-0041 — `mini-treasury`: FROST threshold-signature custody + live multi-device demo  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** D-0035 point 5, D-0037, whitepaper §8.2/§10/§11, `mini-treasury::{frost_keygen,frost_sign}`, `examples/frost_live_demo.rs`.

The last of the four D-0035 point 5 areas the founder asked to be worked
on ("also wire a live multi-device signing demo," the larger of the two
offered options). Implemented:

- `frost_keygen::trusted_dealer_keygen` — Feldman-VSS Shamir secret
  sharing: a group secret key `f(0)` split into `n` shares `f(i)`, any
  `threshold` of which can later sign, each individually verifiable
  against published coefficient commitments without trusting the dealer's
  word.
- `frost_sign` — the full two-round FROST protocol (Komlo & Goldberg):
  round-1 nonce commitments, binding factors (`rho_i`) that close the
  Drijvers et al. adaptive-nonce attack on naive two-round Schnorr
  aggregation, round-2 responses weighted by each signer's Lagrange
  coefficient, per-share verification before aggregation (attributable
  failure, not just an unverifiable aggregate), and a final signature that
  is byte-for-byte an ordinary Schnorr signature — no verifier needs to
  know a threshold scheme produced it.
- `examples/frost_live_demo.rs` — five genuinely separate OS threads (one
  per committee device, each holding only its own share, moved into that
  thread and never shared with another) talking to a coordinator only
  through `std::sync::mpsc` channels, the same request/response shape a
  real transport would carry. Runs two live sessions: a 3-of-5 payout with
  two devices offline, and an adversarial session where one device's
  reported share is tampered with in transit — caught and attributed by
  per-share verification before any signature is produced.

Two load-bearing identities were hand-derived and checked term-by-term
before writing any code, the same discipline `mini_value::bp_range` used
for Bulletproofs: individual-share verification
(`z_i*G == R_i + c*lambda_i*Y_i`) and aggregate signature validity
(`z*G == R + c*Y`, via Shamir reconstruction-in-the-exponent,
`sum_i lambda_i*s_i = f(0)`). Both are documented in `frost_sign`'s module
docs. 28 tests in `mini-treasury` (up from 9), all passing on first
implementation.

**Explicitly, not implicitly, a partial answer — three honest limits, not
silently dropped:** (1) keygen is trusted-dealer, not distributed key
generation — a production deployment needs DKG so no single party ever
holds the full secret, even briefly; (2) `SigningNonces` are not
zeroized on drop; (3) the live demo's channels stand in for a real network
transport, which is not wired to `mini-net`/`mini-bearer` yet. None of
these are silently glossed — each is stated in `frost_keygen`'s and the
example's own doc comments.

**[FREEZE reminder — D-0037 still applies.]** Founder-reviewed, AI-
authored prototype, not an external-audit-equivalent — the whitepaper's
own "permanent honeypot by nature" framing (§11) applies to this crate
more than any other in the workspace. `receipt::ExternalReceiptOracle`
(verifying real Bitcoin/Monero transactions actually arrived) remains
completely out of scope here and is a separate integration surface, not
something FROST closes.

This closes out all four of the founder's originally-requested D-0035
point 5 areas: hybrid consensus (D-0039), personhood (D-0038), MINI
transaction-privacy primitives (D-0036/D-0040), and treasury custody
(this entry) — all now real, founder-reviewed prototypes pending external
audit, none claimed as production-ready.

---

### D-0042 — `mini-bearer`: real TCP transport (`TcpBearer`) + a live multi-process gossip demo in `mini-net`  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** whitepaper §7/§8.1, `mini-bearer::tcp`, `mini-net::examples::gossip_live_demo`, root README's "Path to a global launch."

Founder direction after reviewing the "path to a global launch" gap list:
prioritize a real network transport over the next crypto prototype, since
nothing in the workspace opened a socket before this — every demo so far
(including the FROST live demo, D-0041) simulated multiple parties inside
one process. Implemented:

- `mini_bearer::TcpBearer` — a real [`Bearer`] over TCP, using the framing
  (`encode_frame`/`FrameReader`) this crate already shipped for byte-stream
  bearers but had never had a real one to use them. Constructed via
  `connect` (dial out) or `from_stream` (wrap an accepted connection).
  `BearerError` gained an `Io(String)` variant (kept a `String`, not the
  raw `std::io::Error`, so the enum stays `Clone`/`PartialEq`/`Eq` like
  every other error type in this tree). 6 new tests: loopback round-trip,
  bidirectional traffic, back-to-back frame pipelining split correctly,
  peer-close surfaces as `Closed`, `try_recv` polling semantics, oversized-
  frame rejection.
- `mini_net::examples::gossip_live_demo` — three genuinely separate OS
  processes (not threads, not one process) relaying a message over real
  `TcpBearer` connections: a hub process accepts N leaf connections (each
  split via `TcpStream::try_clone` into an independent receive handle,
  owned solely by its reader thread, and a shared send handle for
  forwarding — no lock contention on the blocking `recv` path), runs
  `mini_net::GossipRouter`'s existing dedup-flooding logic against real
  inbound frames, and forwards new messages to every other connected leaf.
  Run and manually verified end-to-end: a message sent by one leaf process
  arrived at a second leaf process having genuinely crossed two real TCP
  sockets and a separate relay process in between.

**What this does and does not close.** This is a stand-in for local-Wi-Fi/
relay IP connectivity, proven over real sockets for the first time in this
workspace — it is explicitly **not** BLE (needs platform-native radio code
this environment cannot build or test), not peer discovery (`RoutingTable`
is unexercised by the demo), not a mesh (the demo is hub-and-spoke to keep
process orchestration simple, though `GossipRouter`'s dedup logic itself
doesn't care about topology), and carries no encryption at the bearer
layer by design (`mini_bearer::Channel` is the layer that adds that,
already demonstrated separately by the keystone demo). `mini-net`'s own
library code stays transport-agnostic on purpose — `mini-bearer` is a
`[dev-dependencies]`-only addition, used by the example, not the crate's
public API.

Updates the root README's status table (`mini-net`, `mini-bearer` both
move from "no real transport" toward "a real one exists for IP-reachable
connectivity") and the "Path to a global launch" list's item 2 accordingly
— still open for BLE specifically, and for wiring this into
`mini-bootstrap`/`mini-sync`, but no longer true that "nothing in this
tree opens a socket."

---

### D-0043 — `docs/FOUNDER_DIRECTIVES.md` adopted as a canonical document  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** the Constitution, `docs/INVARIANTS.md`, `CONTRIBUTING.md`, `.github/pull_request_template.md`, root `README.md`.

Founder directive, verbatim in substance: this repository now carries a
seventeen-directive founding document — *"MININET — Founder Directives:
The Principles Behind Every Engineering Decision"* — as required first
reading for every contributor, human or AI, before opening the codebase.

**What this document is, and is deliberately not**, per its own preface:
not the Constitution, not the Whitepaper, not a Specification. Those
documents say *what* to build and *what may never be violated*. The
Founder Directives say *why* — so that when a future engineer (the
document's own framing: "40-100 years from now") meets a problem no
existing Specification anticipated, and cannot ask a founder why a past
choice was made, this document is that conversation. It does not amend,
soften, or add to any Tier-F frozen invariant in `docs/INVARIANTS.md` —
it explains the reasoning those invariants were built to protect.

**What changed to wire it in:**
- `docs/FOUNDER_DIRECTIVES.md` — the seventeen directives, verbatim, plus a
  short closing section stating explicitly how the document relates to
  the Constitution/Whitepaper/Specs/decision log (it sits underneath all
  of them, not alongside).
- Root `README.md` — a callout naming it required first reading, directly
  under the opening paragraph, above every other pointer including the
  build instructions; also added as item 1 of "New here?" (bumping the
  prior four items down) and to the repository-map file tree.
- `CONTRIBUTING.md` — required reading before the existing contribution
  principles, with an explicit note that AI-assisted contributions under
  D-0037 are expected to reason from the same directives a human
  contributor would.
- `.github/pull_request_template.md` — referenced in the top comment
  alongside `docs/INVARIANTS.md`, and a reviewer-checklist line for
  judgment calls that fall outside any existing spec or invariant.

**Why this belongs in the decision log despite not being a protocol
change.** Every other entry here records an architectural or cryptographic
choice and its rationale. This entry records the adoption of the
document that all *future* such entries — and every future judgment call
a spec doesn't cover — are expected to be reasoned from. Per the
directives' own Directive 17 ("future child" test) and Directive 13
("think in centuries"), that adoption is exactly the kind of decision this
log exists to make permanent and attributable, not silently assumed.

---

### D-0044 — Master roadmap opened; first four audit issues closed; CI gained real dependency scanning and reproducibility checks  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** GitHub issue #92 (roadmap index), issues #8/#10/#29/#69/#71/#73, `docs/audits/`, `.github/workflows/ci.yml`.

Founder direction: convert the founder's own CTO-level engineering
roadmap (Phases 0-12, ~85 topics) into GitHub issues rather than a
document only, "otherwise engineers will start solving local problems
instead of building the civilization in the correct order." 84 issues
opened plus a master hub/index issue (#92) substituting for a GitHub
Project board, since no tool exists in this environment to create one
directly. Also established `docs/FAILURE_BOOK.md` (founder proposal,
issue #91) and `SECURITY.md`.

**First batch of issues actually closed, not just filed** — chosen for
being genuinely completable without external auditors, real hardware, or
business decisions:

- **#73 (dependency-vulnerability scanning) + #69 (reproducible builds)**,
  closed together in `.github/workflows/ci.yml`. Reproducibility: verified
  locally first (build the workspace twice from a clean target directory,
  hash the example binaries, confirm byte-identical output) before writing
  the CI job, rather than writing an untested job and hoping. Dependency
  scanning: attempting a naive `cargo install cargo-audit --locked` surfaced
  a real, non-obvious problem — the newest `cargo-audit` compatible with
  this workspace's pinned toolchain (rustc 1.83.0) cannot parse the current
  RustSec advisory database (it contains CVSS 4.0 entries a too-old
  `rustsec` crate version doesn't understand), so it would have "passed" a
  CI job that never actually scanned anything. Fixed by using the official
  `rustsec/audit-check` GitHub Action instead, which ships its own prebuilt
  binary decoupled from this repo's toolchain pin.
- **#71 (memory-safety audit)** — confirmed all 22 crates carry
  `#![forbid(unsafe_code)]`; audited the 40-crate external dependency tree
  (sources already cached locally) for `unsafe` usage and found every
  occurrence falls into one of four expected categories (SIMD intrinsics,
  OS-syscall FFI for entropy, zeroize's core correctness requirement, or
  build-time-only macro tooling) — none unexplained or obscure.
- **#29 (CID integrity review)** — traced content-addressing end to end
  across `mini-crypto::multihash` (algorithm downgrade/multicodec-confusion/
  encoding-malleability all closed), `mini_objects::ObjectId` (id always
  recomputed from parsed content, never trusted from the wire),
  `mini_store::Store` (content-addressing re-checked on every read, not
  just at insert), and `mini-media`'s chunked assembly (whole-payload
  length + digest both checked, closing the truncation concern). PASS on
  all four layers.
- **#8 (constitutional audit)** — a 26-row PASS/PARTIAL/FAIL matrix
  against every Tier-F row in `docs/INVARIANTS.md`, adding a
  centralization-vector and trust-assumption column INVARIANTS.md itself
  doesn't carry. Result: 18 PASS, 7 PARTIAL, 1 not-yet-built, **zero
  violations** — every PARTIAL traces to the same root cause (the
  networked chain and storage fabric not existing yet), which independently
  confirms the roadmap's own Phase 4/Phase 5 prioritization rather than
  surfacing a reason to reorder it.
- **#10 (frozen invariants review)** — the sharper, four-adversarial-
  question companion to #8 (institutional control? money buying
  governance, even indirectly? second-class humans? freedom-removing
  updates?), grouped thematically rather than row-by-row to avoid
  repeating identical answers 26 times. Its one real finding: **Sybil-cost
  economics is the sharpest "maybe" in the whole review** — an attacker
  with capital can indirectly buy governance/value by mass-producing
  verified-looking identities rather than by touching any balance-to-vote
  mapping directly. Not a code defect (nothing today violates P1/P2), but
  the clearest confirmation that Phase 2's Sybil-resistance work (#18/#20)
  is correctly the roadmap's highest-priority open question, not merely
  one item among many.

All four audits are filed under `docs/audits/`, one file per issue,
explicitly point-in-time and non-living (like this log and the Failure
Book) — a future code change that could affect a verdict gets a *new*
dated audit, not a silent edit to an old one.

---

### D-0045 — Canonical money finality during outages  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder review of `docs/INVARIANTS.md`/`docs/DECISION_LOG.md`, Directive 4/5, `docs/INVARIANTS.md` §4 (M2/M3).

**Decision:** local/offline value transfers are signed pending claims,
never final ownership. Finality occurs only on canonical chain inclusion.
Conflicting claims resolve strictly by canonical ordering; a later
conflicting spend is rejected outright, never merged or reconciled with
the earlier one.

**Reason:** the whitepaper's offline-first design and the constitution's
single-canonical-truth requirement (Directive 5: "money may never
disagree") are in real tension unless this rule is stated explicitly and
frozen. Without it, "offline-first" could be misread as license to treat
a locally-exchanged promise as settled.

**Constitutional impact:** directly implements Directive 4 ("the ledger
must always answer... who owns what... with certainty") and Directive 5
("during outages, users exchange signed promises — not final ownership").
Adds frozen invariants **M2** and **M3** to `docs/INVARIANTS.md` §4.

**Implementation status:** design-only. No code implements offline
settlement or double-spend reconciliation yet — see
`docs/STATUS.md` §4. This entry is the frozen constraint that design must
satisfy when built, not a claim it exists.

**Failure point:** if a future implementation ever lets a local
committee, relay, or cache mark a transfer "accepted" in a wallet UI
without clearly distinguishing that from canonical finality, users could
reasonably believe a transaction is settled when it is not — a UX failure
that becomes a constitutional violation if the distinction isn't
enforced in the data model itself, not just in how it's displayed.

**Required follow-up:** [roadmap #40](https://github.com/britak420/Mininet/issues/40)
(double-spend reconciliation rules) and
[#41](https://github.com/britak420/Mininet/issues/41) (offline transaction
settlement model) must both satisfy this entry's frozen constraint as an
explicit acceptance criterion.

**Supersedes / superseded by:** none — first entry on this specific
question; M2/M3 in `docs/INVARIANTS.md` are new rows, not amendments to
existing ones.

---

### D-0046 — Fork legitimacy continuity  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder review, Directive 7, `docs/INVARIANTS.md` §5 (F1), `docs/FAILURE_BOOK.md`.

**Decision:** forking the software is, and remains, free — anyone may
copy this repository and build something different. Inheriting
*Mininet's legitimacy* is not automatic and is not conferred by the code
alone. A fork carries Mininet's legitimacy only if it preserves
continuity of the frozen constitutional invariants, the personhood-root
history, release-registry continuity, and canonical chain state. A code
copy that breaks any of these is a different network, not Mininet under
a new name.

**Reason:** Directive 7 already states this in prose ("the canonical
network is defined by continuous adherence to the Constitution... not by
a repository or a trademark"); this entry makes it a checkable invariant
with named continuity criteria, rather than leaving "legitimacy" as an
undefined term someone could argue about later.

**Constitutional impact:** directly implements Directive 7. Adds frozen
invariant **F1** to `docs/INVARIANTS.md` §5. Does not restrict forking
itself in any way — P3/P7 (no owner, nobody forced to participate) are
unaffected; this entry is about what a fork *inherits*, never about
whether forking is permitted.

**Implementation status:** design-only / criteria-only. No code
represents "legitimacy" as a concept yet, since there is no networked
chain or release registry for continuity to be measured against —
see [roadmap #57](https://github.com/britak420/Mininet/issues/57).

**Failure point:** the criteria named here (invariants, personhood root,
release registry, canonical chain state) are a starting list, not
necessarily exhaustive. If a future fork preserved all four listed
criteria but violated the *spirit* of continuity through some mechanism
this entry didn't anticipate, the letter of F1 could be satisfied while
its purpose was defeated — exactly the kind of gap `docs/FOUNDER_DIRECTIVES.md`
exists to help a reviewer reason past.

**Required follow-up:** [roadmap #57](https://github.com/britak420/Mininet/issues/57)
(fork legitimacy criteria) owns turning this into a fully checkable
definition once the release registry and chain exist to check it against.

**Supersedes / superseded by:** none — first dedicated fork-legitimacy
entry in this log; `docs/FAILURE_BOOK.md`'s Cosmos/Go and Flutter entries
are related (paths not taken) but not the same question.

---

### D-0047 — Production audit gates (tightens D-0037)  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder review, tightens D-0036/D-0037, `docs/INVARIANTS.md` §9 (A1), `docs/audits/issue-8-constitutional-audit.md`.

**Decision:** D-0037's AI-authorship permission stands unchanged — AI may
still draft code across all four D-0035 point 5 domains, with mandatory
human review. What changes: **external cryptography audit is now a hard
gate for *production* use** — not "desirable before real value depends on
it," as D-0037's original framing could be read — for four specific
domains: MINI transaction-privacy primitives (`mini-value`), treasury
custody (`mini-treasury`), consensus (`mini-chain`), and personhood
proofs (`mini-uniqueness`). Passing tests is not audit. Founder review is
not audit. Both remain necessary; neither is sufficient for production
use of these four domains.

**Reason:** founder review of this log flagged D-0037's original language
as a red flag on its own terms — "audit only desirable" is exactly the
kind of soft language that erodes under schedule pressure. A hard gate,
stated plainly, closes that reading.

**Constitutional impact:** does not weaken D-0037's authorship policy.
Strengthens the practical protection around Directive 4/5 (money/
finality certainty) and Directive 2 (assume every authority, including
this project's own founders under time pressure, can drift). Adds frozen
invariant **A1** to `docs/INVARIANTS.md` §9.

**Implementation status:** no code path in this tree currently claims
production-readiness for any of the four gated domains — every relevant
crate (`mini-value`, `mini-treasury`, `mini-chain`'s eventual consensus,
`mini-uniqueness`) is explicitly labeled prototype/founder-reviewed-only
already. This entry keeps that true going forward rather than retrofits
anything.

**Failure point:** a gate stated in a document is only as strong as the
process that checks it before a release. Until a release pipeline exists
that can *mechanically* verify "has this had an external audit" before
allowing a production flag, this remains a policy commitment enforced by
review discipline, not by code — the same class of gap D-0033's
2-approval floor has today, and no worse.

**Required follow-up:** [roadmap #72](https://github.com/britak420/Mininet/issues/72)
(external cryptography review coordination) owns actually engaging an
auditor; the eventual release pipeline (Phase 9,
[#65](https://github.com/britak420/Mininet/issues/65)-[#70](https://github.com/britak420/Mininet/issues/70))
should encode this gate mechanically, not just in policy, once it exists.

**Supersedes / superseded by:** tightens D-0037 (D-0037's authorship
permission is unchanged and remains in effect; its audit language is
superseded by this entry's hard-gate framing for the four named domains
specifically — D-0037 still governs every other AI-authorship question).

---

### D-0048 — Trusted-dealer FROST sunset (P0)  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder review, D-0041, `docs/INVARIANTS.md` §4, `crates/mini-treasury/src/frost_keygen.rs`.

**Decision:** D-0041's trusted-dealer FROST keygen (`mini_treasury::
trusted_dealer_keygen`) is confirmed as **prototype-only, severity P0**
— the highest-priority category this log uses for a known limitation.
Production use of FROST threshold custody for any real treasury or
bridge value requires (a) distributed key generation (DKG), so no single
party ever holds the full secret at any point, even briefly, and (b)
zeroized nonces (`SigningNonces` currently does not zeroize on drop —
already stated as an honest limit in `frost_keygen.rs`'s module docs, now
elevated to a frozen production blocker here).

**Reason:** trusted-dealer keygen's exact failure mode is that one actor
briefly holds the whole secret while splitting it. For a prototype this
is an accepted, clearly-labeled limitation; for anything holding real
treasury or bridge value it is unacceptable regardless of how briefly the
exposure window is, or how trusted the dealer is assumed to be —
Directive 2 ("assume every authority is compromisable") applies to the
dealer role itself.

**Constitutional impact:** reinforces D-0047's audit-gate framing
specifically for treasury custody, the whitepaper's own named "permanent
honeypot" risk (§11). No existing invariant is weakened; this entry adds
specificity to what "production-ready" must mean for this one
subsystem.

**Implementation status:** trusted-dealer keygen is implemented, tested,
and demonstrated live (D-0041's multi-process signing demo) —
**exactly as a prototype**, which remains its correct classification. DKG
is not implemented. Nonce zeroization is not implemented. See
`docs/STATUS.md` §4.

**Failure point:** if trusted-dealer keygen is ever used to generate a
key for a committee that then custodies real value — even "just for a
testnet," even "just temporarily" — the P0 exposure window becomes real
regardless of intent. The risk is entirely in the gap between "this is
labeled a prototype" and "someone uses it anyway under time pressure."

**Required follow-up:** [roadmap #93](https://github.com/britak420/Mininet/issues/93)
(FROST DKG + nonce zeroization, filed alongside this entry) owns closing
this gap before any testnet or real treasury deployment.

**Supersedes / superseded by:** does not supersede D-0041 (the FROST
protocol design and trusted-dealer keygen's correctness as a prototype
both stand); adds a severity classification and production blocker
D-0041 itself did not state as explicitly.

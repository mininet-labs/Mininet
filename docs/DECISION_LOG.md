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

## Decision-number allocation across parallel tracks (added D-0200 batch)

When two agent tracks develop at once, both appending here, they collided on
the same next number (both grabbed `D-0076` before either merged). To stop
that permanently without racing to renumber on every rebase, decision numbers
are **banded by track**:

- **Main sequence (`D-00xx`)** — the primary/operational line. The
  self-hosted-forge-and-operational track continues it: `…D-0077`, then
  `D-0078`, `D-0079`, … The gap from `D-0080` up to `D-0199` is this line's
  to grow into.
- **`D-02xx` — the networking & consensus track** (roadmap #36–#45:
  `mini-consensus`, `mini-net`, transports). It allocates from `D-0200`
  upward, so it never collides with the main sequence regardless of merge
  order.
- **`D-03xx` — the privacy/cost-doctrine track** (D-0094's adopted
  research direction; lanes defined in `docs/design/
  privacy-cost-doctrine-parallel-execution-plan.md`: `mini-privacy-policy`
  and everything downstream of it — object-envelope privacy, transport
  policy routing, mix-network research, resource pricing, human-evidence
  taxonomy). It allocates from `D-0300` upward. Because this track is
  itself designed to run several lanes in parallel, a same-band collision
  is possible *within* `D-03xx` (two lanes both grabbing `D-0301`, say) —
  that is resolved exactly like every other collision in this log: the
  second PR to merge rebases and renumbers. Numbers are claimed at PR-open
  time, not reserved in advance per lane.

The bands are a coordination convenience, not a hierarchy — a banded
decision carries exactly the same authority as any other, and cross-track
references (`Refs:` / `Supersedes:`) point across bands freely. If a
further track appears, give it the next free hundreds band (`D-04xx`) and
add it here. The intentional gaps between bands are expected; they are
not missing history.

## Entry template (D-0045 onward)

```
### D-00xx — Title  ·  *Accepted*
**Date:** ... · **Refs:** ...

**Decision:** what was decided, stated plainly.
**Reason:** why, in a sentence or two — the full reasoning can be longer,
but the one-line version should stand alone.
**Constitutional impact:** which principle(s)/invariant(s) this touches,
strengthens, or is constrained by — cite them by ID (e.g. "Directive 4,
M2") so the chain in `docs/INVARIANTS.md`'s traceability section resolves
without guessing. "None" is a valid, common answer.
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

**Required follow-up:** [roadmap #40](../../issues/40)
(double-spend reconciliation rules) and
[#41](../../issues/41) (offline transaction
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
see [roadmap #57](../../issues/57).

**Failure point:** the criteria named here (invariants, personhood root,
release registry, canonical chain state) are a starting list, not
necessarily exhaustive. If a future fork preserved all four listed
criteria but violated the *spirit* of continuity through some mechanism
this entry didn't anticipate, the letter of F1 could be satisfied while
its purpose was defeated — exactly the kind of gap `docs/FOUNDER_DIRECTIVES.md`
exists to help a reviewer reason past.

**Required follow-up:** [roadmap #57](../../issues/57)
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

**Required follow-up:** [roadmap #72](../../issues/72)
(external cryptography review coordination) owns actually engaging an
auditor; the eventual release pipeline (Phase 9,
[#65](../../issues/65)-[#70](../../issues/70))
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

**Required follow-up:** [roadmap #93](../../issues/93)
(FROST DKG + nonce zeroization, filed alongside this entry) owns closing
this gap before any testnet or real treasury deployment.

**Supersedes / superseded by:** does not supersede D-0041 (the FROST
protocol design and trusted-dealer keygen's correctness as a prototype
both stand); adds a severity classification and production blocker
D-0041 itself did not state as explicitly.

---

### D-0049 — `mini-bounty`: anonymous developer bounty claims  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder direction (GitHub↔governance bridge discussion), D-0036/D-0037/D-0047, `crates/mini-bounty/`.

**Decision:** build anonymous bounty claiming as a direct composition of
two prototypes already built and reviewed in `mini-value` — linkable ring
signatures (D-0036) prove membership in an approved-contributor set
without revealing which member, and stealth addresses (D-0036) receive
the payout — rather than designing new cryptography. A `BountyPool` holds
every grant ever issued (claimed or not, so the ring never shrinks to
unmask the last claimant); `claim`/`verify_claim` bind the ring signature
to an exact `(pool_id, payout_address)` pair via a length-prefixed
signed message; the key image prevents double-claiming, tracked by a
`KeyImageLedger` mirroring `mini_presence::ReplayGuard`'s exact shape.

**Reason:** the founder's GitHub↔governance bridge discussion asked
specifically how a developer could claim a piece of a bounty "without
everyone knowing who they were." That's precisely the anonymity property
a linkable ring signature already provides for spending — the founder's
own contribution-approval flow (GitHub PR approved → grant published →
contributor claims) maps onto "one of N authorized keys signs" with no
conceptual gap, so building new cryptography here would have been
needless duplication (Directive 14: "the strongest protocol is usually
the one that removed the most unnecessary parts").

**Constitutional impact:** implements Directive 9 ("privacy is
architecture... depend only on mathematics") for a new use case. Adds no
new frozen invariant — governed by the same D-0036/D-0037/D-0047 regime
every other `mini-value`-derived prototype already falls under. Explicitly
does **not** claim anonymity from GitHub itself, only from Mininet and
the public ledger — stated plainly in the crate's own docs to avoid
overclaiming.

**Implementation status:** real, tested (15 tests, all passing on first
implementation — no new algebraic identity to hand-derive, since none of
the underlying math changed). No GitHub integration exists — this is the
claim cryptography only. See `docs/STATUS.md`.

**Failure point:** a pool with too few grants provides little or no real
anonymity (a ring of one is fully transparent) — this crate enforces no
minimum ring size, leaving that judgment to the caller
(`BountyPool::ring_size`). A funding process that mints one grant per
pool rather than batching approvals would defeat the entire point without
any code here reporting an error.

**Required follow-up:** the GitHub-reading half (webhook/API integration
that mints `BountyGrant`s from approved PRs) is unbuilt and not yet a
filed roadmap issue — should be filed once the broader GitHub↔governance
bridge design (proposal generation from commits, on-chain release
registry linkage) is further along, since bounty-grant minting is one
consumer of that same integration layer, not a separate one. Minimum
ring-size policy (if any) is a governance/economics question, not a
cryptography one — left open deliberately.

**Supersedes / superseded by:** none — first entry on this question.

---

### D-0050 — `docs/THREAT_MODEL.md` + the traceability chain convention  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Founder direction ("holy trinity" review + threat-model request), `docs/THREAT_MODEL.md`, `docs/INVARIANTS.md`.

**Decision:** adopt `docs/THREAT_MODEL.md` as the fifth canonical
document (alongside `FOUNDER_DIRECTIVES.md`, `INVARIANTS.md`,
`DECISION_LOG.md`, `FAILURE_BOOK.md`), cataloging civilization-scale
threats across five categories (Human, Technical, Economic, Political,
Civilization) rather than a conventional infosec checklist. Simultaneously
adopt an explicit **traceability chain** convention: every load-bearing
row in `INVARIANTS.md` now carries a stable ID and a **Directive** column,
so the chain `Founder Directive → Invariant → Source (Spec/D-00xx) →
Enforced by (crate + test)` is walkable in either direction. This entry's
own **Constitutional impact** field demonstrates the convention it
adopts, citing IDs directly rather than describing them in prose.

**Reason:** the founder identified that the existing three documents
(nicknamed the "holy trinity" — Directives/Invariants/Decision Log) plus
`FAILURE_BOOK.md` still left two gaps: nothing cataloged *what could kill
the project at civilization scale* (as opposed to what's already decided
or already tried and rejected), and nothing let a reviewer mechanically
walk from a founding value to the specific test that enforces it, or
backward from a failing test to the principle it protects. Both gaps are
about making the constitutional chain auditable rather than just
documented in prose.

**Constitutional impact:** Directive 13 ("Think in Centuries") directly
motivates `THREAT_MODEL.md`'s civilization-scale category. Every existing
invariant ID (P1-P6, M1-M3, F1, A1, V1-V4, PH1, ID1-ID5, U1-U3, PR1-PR2,
S1, N1-N2, AI1, X1-X3) now has an explicit Directive citation in
`INVARIANTS.md`; no invariant's *meaning* changed, only its traceability.
Adds no new frozen invariant itself — this is a documentation-structure
decision, not a protocol decision.

**Implementation status:** both `docs/THREAT_MODEL.md` and the
`INVARIANTS.md` Directive-column rewrite are complete and committed. Every
Tier-F section (9 domains) plus the Foundational table carries the new
column. `THREAT_MODEL.md` cross-references invariant IDs in its "Stopped
by" column per threat, and honestly marks several as "Aspirational" or
"Explicitly unresolved" (notably Sybil resistance, storage-consolidation
resistance, governance-capture-by-coordination, and founder-authority
limits) rather than overclaiming coverage.

**Failure point:** a catalog of unresolved threats is only useful if it's
kept current — `THREAT_MODEL.md` explicitly says a document like this
that stops growing is a sign no one is looking anymore. If new threats
are discovered but not added here, the document silently becomes
decorative rather than load-bearing, the same risk every other canonical
document in this project carries.

**Required follow-up:** several `THREAT_MODEL.md` entries name gaps with
no filed roadmap issue yet (coordinated-governance-capture detection,
coercion-resistant voting, first-contact eclipse resistance, traffic
obfuscation against ISP-level blocking, post-quantum migration path for
live funds/identities). These should be triaged into roadmap issues as
capacity allows, rather than left only as prose. Not done in this entry
to avoid filing issues faster than they can be meaningfully scoped.

**Supersedes / superseded by:** none — first entry on this question.

---

### D-0051 — Bounty & review system: money funds work, never a decision  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Directive 16, P1, D-0033, D-0049, `docs/design/bounty-and-review.md`, [roadmap #66](../../issues/66).

**Decision:** the developer bounty system (`mini-bounty`, funding/value) and
the code-review/merge system (`mini-forge`, review/voice) are kept as two
crates with **no dependency edge between them in either direction**, so that
funding a bounty can never express or influence a merge decision, and a
merge decision can never read or be swayed by any balance. Publishing a
`BountyGrant` sits strictly downstream of a completed merge — it records
that an already-made decision happened, and can never cause one.

**Reason:** roadmap #66's one hard requirement is that funding stays
"completely separate from any merge/review authority — money funds work,
never buys a decision." The strongest way to guarantee that is structural:
`mini-bounty` depends only on `mini-value` and produces no `Capabilities`
bit; `mini-forge` governance depends on no value crate and counts approvals
per identity root ("no balance, stake, or payment"). Making the wall a
property of the dependency graph means breaching it would require *adding* a
large, obvious, reviewable dependency edge that trips the frozen-domain
checklist — not a subtle runtime change someone could slip past review.

**Constitutional impact:** implements Directive 16 and invariant **P1** (no
balance maps to governance/vote weight) for the developer-contribution flow
specifically. Adds no new frozen invariant — P1 already covers it; this
entry documents the concrete design that realizes it and the tests that
enforce it (`mini-forge` governance suite: per-root approval counting,
two-approval protocol floor, deterministic competing-merge resolution).

**Implementation status:** review side built & tested (`mini-forge`); funding
side built & tested (`mini-bounty`, D-0049); the structural wall (no
dependency edge) holds in the current tree and is verifiable by inspecting
two `Cargo.toml` files. The GitHub-reading integration that would mint a
grant from an approved PR is not built (noted in D-0049) and, when built,
must sit downstream of the merge decision, never become an input to it.

**Failure point:** the wall depends on the dependency graph staying acyclic
between these crates. A future change that made merges consult funding (or
funding grant approval capability) would breach P1; this is exactly what
the frozen-domain review checklist exists to catch, but it is only as good
as reviewers actually running the "does this add a value→governance edge?"
check.

**Supersedes / superseded by:** none — first dedicated bounty/review-wall
entry; builds on D-0033 (two-approval floor) and D-0049 (mini-bounty).

---

### D-0052 — Fork legitimacy: four checkable continuity criteria  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Directive 7, F1, D-0046, `docs/design/fork-legitimacy.md`, [roadmap #57](../../issues/57).

**Decision:** a fork is the canonical Mininet if and only if it satisfies all
four continuity criteria — C1 constitutional-invariant continuity, C2
personhood-root history continuity, C3 release-registry continuity, C4
canonical chain-state continuity — each stated as a concrete check with a
named failure mode and a reason it is verifiable. Forking the software
remains free (a legitimate derivative that is honest about being different
is welcome); the criteria gate only whether a fork *inherits Mininet's
legitimacy*, never whether forking is permitted.

**Reason:** D-0046 froze the principle (F1) and explicitly deferred "a fully
checkable definition" to roadmap #57. This entry discharges that: it turns
D-0046's four named continuity criteria into conjunctive, chain-based
(not snapshot-based) checks a reviewer can actually apply, and addresses
D-0046's own flagged letter-vs-spirit gap by keeping Directive 7 as the
tie-breaker for anything the four criteria don't anticipate.

**Constitutional impact:** directly implements Directive 7 and makes
invariant **F1** (`docs/INVARIANTS.md` §5) checkable. Adds no new frozen
invariant — F1 already exists; this refines its meaning from "named criteria
exist" to "here is how each criterion is checked." Reaffirms that legitimacy
is a standard others verify and choose to honor, never one imposed by a kill
switch, trademark, or admin key (which would violate P3).

**Implementation status:** criteria complete (this document + F1 + D-0046).
C1 (invariant tests) and C2 (`did:mini` verification) are runnable against
the current tree today; C3 and C4 depend on a live, populated release
registry and chain that exist as logic (`mini-forge`/`mini-chain`) but not
yet as a running network, so running them at scale waits on the same
networking work V1 and roadmap #36-#45 track.

**Failure point:** the four criteria are not proven exhaustive — D-0046's
warning stands that a fork could satisfy the letter while defeating the
spirit. The mitigation (conjunctive + continuity-based criteria, Directive 7
as residual tie-breaker) reduces but does not eliminate this; a genuinely
novel continuity-severing mechanism would need a new criterion added here.

**Supersedes / superseded by:** refines D-0046 (does not supersede it —
D-0046's decision stands; this entry adds the checkable definition D-0046
said #57 would own).

---

### D-0053 — Identity audit hardening: recovery, threshold policy, delegation-chain refusal  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Directive 2/6/8, invariants ID1/ID3, SPEC-01 §5/§6, `docs/audits/issue-12-did-mini-security-audit.md`, `docs/audits/issue-13-identity-recovery-audit.md`, [#12](../../issues/12)/[#13](../../issues/13).

**Decision:** three `did-mini` changes ship from the identity audit: (1) a
real recovery path, `Controller::recover_from_kel`, reconstructing control
from a public KEL + escrowed next-key seeds via an ordinary rotation — the
lost-device/death recovery pre-rotation was always meant to enable; (2)
rotation now preserves the standing M-of-N next-threshold instead of silently
forcing N-of-N (policy changes require the new explicit
`rotate_with_next_and_threshold`); (3) `verify_delegation` rejects a
delegated identity acting as a delegator (`RootIsDelegated`), so no root
counter can be fed a device posing as a root.

**Reason:** the audit found #2 as a real bug (first rotation silently rewrote
an identity's threshold policy and could brick future rotations), #3 as a
cheap ambiguity worth closing, and #1 as a missing capability that left
pre-rotation's whole purpose unreachable after device loss. All three are
correctness/robustness fixes with new adversarial tests
(`crates/did-mini/tests/recovery.rs`, 8 tests).

**Constitutional impact:** strengthens invariants ID1 (keys never leave
device — recovery uses escrowed seeds, no new export path) and ID3
(pre-rotation) without changing their meaning. No frozen invariant altered.
No wire-format change — previously emitted KELs still verify.

**Implementation status:** shipped and tested; full workspace suite green.
The launch-blocking gap named by the audit — KEL freshness / duplicity
detection (stale-KEL revocation bypass) — is **not** fixed here; it is
inherent to the pre-witness milestone and owned by M3 (SPEC-01 §7). Interim
rule (fetch freshest KEL, pin highest sn per SCID) is documented in
`verify_delegation`.

**Failure point:** recovery depends on the holder actually escrowing the
next-key seed off-device; an identity that loses both device and escrow is
permanently orphaned by design. Client onboarding must enforce escrow — a
product requirement recorded in the #13 audit, not code here.

**Supersedes / superseded by:** none — first identity-hardening entry;
builds on the M1/M2 identity milestones.

---

### D-0054 — Personhood promotion requires a live seed-anchored signal  ·  *Accepted*
**Date:** 2026-07-08 · **Refs:** Directive 8/15, invariant P2 (+ its hard limitation), SPEC-02 / whitepaper §11, `docs/audits/issue-18-sybil-social-graph-review.md`, [#18](../../issues/18).

**Decision:** `mini-uniqueness`'s `PromotionPolicy` gains
`full_required_sources` (default `[VouchingGraph]`): reaching `FullHuman`
now requires the seed-anchored vouching-graph signal to be *live*, not just
any N signals summing to a high score.

**Reason:** the Sybil review found a farm-saturation bypass — because the
fused score's denominator counts only sources with evidence, a farm could
reach `FullHuman` on self-attestable physical presence (forgeable end-to-end
per #17) plus one friendly `External` method, with zero connection to the
honest graph. The vouching graph is the one signal a farm structurally
cannot fake (trust propagates only from the seed cohort), so it must be
mandatory rather than substitutable.

**Constitutional impact:** hardens P2's *target* (one human, one vote) at
the personhood layer, but does **not** resolve P2's standing hard limitation
(identity-root ≠ verified human) — the "no longer cheap" claim remains
unproven at production parameters. No frozen invariant changed; this tightens
a tunable policy toward the invariant's intent.

**Implementation status:** shipped and tested
(`a_farm_cannot_reach_full_human_without_the_seed_anchored_vouch_signal`,
`a_fully_decayed_vouch_signal_does_not_satisfy_the_required_gate`). Seed-set
governance, threshold calibration, and at-scale simulation remain open
(#11/#21).

**Failure point:** a nation-state adversary who slowly earns *genuine*
seed-anchored vouches via co-opted real humans still defeats this — the gate
raises cost, it is not a wall. Emptying `full_required_sources` reopens the
bypass; the default must not be emptied without a recorded decision.

**Supersedes / superseded by:** none — first Sybil-hardening entry; builds
on D-0038 (the multi-signal accumulator).

---

### D-0055 — `mini-settlement`: offline transaction settlement, implementing M1/M2/M3  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 5, invariants M1/M2/M3, D-0045, `crates/mini-settlement/`, [roadmap #41](../../issues/41).

**Decision:** build the offline settlement protocol as a nonce-based signed
payment claim (`PaymentClaim`: payer, payee, amount, monotonic per-payer
nonce, validity window, last-known-chain reference) with an explicit
wallet-facing state machine (`SettlementState`: SignedLocal → AcceptedLocal
→ PendingCanonical → Finalized | RejectedConflict | Expired), where only
`Finalized` is ever final. Reconciliation (`reconcile`) reads an abstract
`CanonicalLedgerView` trait rather than a real chain-execution engine —
the same seam `mini-forge::KelDirectory` already uses for identity lookups
— so the protocol's rules are fully specified and tested now, without
waiting for roadmap #36-#45's networked consensus to exist first. Local
double-spend detection (`ClaimWatcher`) is a separate, explicitly-labeled
risk heuristic, never a source of finality.

**Reason:** D-0045 froze M1/M2/M3 as constraints with no implementing
code; #41 asked for "the concrete protocol" turning Directive 5's prose
into something checkable. A nonce-based claim (not a UTXO/key-image model,
not a payment channel) is the minimal primitive matching Directive 5's own
wording ("signed promises"), reuses no new cryptography (only already-
reviewed `mini-crypto` Ed25519/BLAKE3), and composes for free with
anonymous addressing since this crate never inspects key contents beyond
signature verification.

**Constitutional impact:** implements Directive 5 and invariants M1, M2,
and M3 directly. M1 by omission (no merge function exists anywhere in the
crate's API surface); M2 via `SettlementState`/`WalletLabel` making the
pending/accepted/finalized distinction a type rather than a UI convention
(directly closing D-0045's own named failure point); M3 via `reconcile`
never finalizing a claim except by reading a `CanonicalLedgerView`. Adds
no new frozen invariant — M1/M2/M3 already existed; this is their first
implementation.

**Implementation status:** real, tested — 26 tests including an explicit
double-spend-across-two-partitions integration scenario proving exactly
one of two conflicting claims ever finalizes. `CanonicalLedgerView` has
only a test-only in-memory implementation (`InMemoryLedgerView`); a real
chain-backed implementation is roadmap #36-#45's job. Not yet wired to any
real transport, wallet, or `mini-value` addressing — this batch is the
protocol core only.

**Failure point:** the whole construction depends on a future
`CanonicalLedgerView` implementation actually enforcing nonce-ordering and
balance sufficiency correctly at the chain-execution layer — this crate
can only be as sound as whatever backs that trait. A wallet UI that reads
`SettlementState` directly instead of going through `wallet_label()` could
still blur AcceptedLocal and Finalized visually, even though the types
distinguish them; this is a client-implementation risk this crate cannot
fully close from the protocol layer alone.

**Required follow-up:** [roadmap #40](../../issues/40)
(double-spend reconciliation rules) should adopt this crate's
`reconcile`/`CanonicalLedgerView` split as its own concrete mechanism
rather than designing a separate one — the double-spend rule M3 requires
is already implemented here, gated only on a real ledger existing.
Confidential-amount integration with `mini-value` and payment-channel
constructions (if ever wanted) are both explicitly out of scope, noted as
future work in the crate's own docs.

**Supersedes / superseded by:** none — first implementation of M1/M2/M3;
refines D-0045's "design-only" status to "protocol implemented, ledger
pending."

---

### D-0056 — External Legitimacy Gates: naming what more code cannot close  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 2, Directive 6, `docs/gates/`, [roadmap #99](../../issues/99).

**Decision:** adopt an explicit category of roadmap issue — "external
legitimacy gate" — for work where the blocker is not missing engineering
effort but a genuine need for outside authority (cryptography audit,
legal counsel), real hardware, or a founder decision on open research
with no known construction. Each gate gets a scope package under
`docs/gates/` (what needs review, the exact questions to answer, the hard
constraints the review must respect) and is tracked on
[#99](../../issues/99), the same
milestone-substitute pattern hub #92 already uses. Labels
`outside-help`/`launch-gate`/`not-code-only`/`external-review` mark the
affected issues so they can't be accidentally "closed" with more Rust.

**Reason:** without this distinction, a roadmap built entirely from
GitHub issues risks looking complete once every issue *engineering can
act on* is closed, while the gates that actually determine whether
Mininet is safe for real value, real personhood claims, or real legal
operation stay invisible inside the same undifferentiated list. Naming
the boundary — what code can prepare versus what only an auditor,
counsel, a specialist, or the founder can close — is itself a form of the
honesty-over-polish discipline this project already applies to crate-
level claims (D-0037, D-0047), extended to the roadmap's own shape.

**Constitutional impact:** implements Directive 2 (assume central
authorities fail — including implicitly trusting AI-authored engineering
as sufficient for gates that structurally require independent human
judgment) and Directive 6 (design for failure — a roadmap that can't
distinguish "done" from "code-complete but unreviewed" is designed to
fail quietly). Adds no new frozen invariant; this is a process/roadmap
convention, not a protocol rule.

**Implementation status:** seven gates identified and scoped this pass:
external cryptography audit (#72), FROST DKG audit (#93), legal counsel
review (#96, newly filed), personhood signal-(b) research decision (#21),
presence/ranging hardware validation (#97, split from #22), treasury
economics/whale-attack modeling (#47/#50), and DTN/extreme-environment
design constraints (#28). #22 split into #97 (presence/ranging, a
security-relevant signal) and #98 (local Wi-Fi data bearer, ordinary
connectivity) because the two have different security requirements and
conflating them risked treating "reachable" as "physically nearby."

**Failure point:** a gate's scope package can be mistaken for the gate
itself being closed — checking a box on #99 must require the *named
outside action* (auditor sign-off, counsel opinion, a founder decision
recorded as its own D-number) to have actually happened, never merely
that engineering finished writing the handoff document. This is stated
explicitly on #99 itself specifically to prevent that failure mode.

**Required follow-up:** the founder engaging each named outside party
(auditor, counsel, tokenomics specialist, DTN expert) or making the one
decision that's the founder's alone (#21's signal-(b) path). Each
package's own "what closes this gate" section names the deliverable.

**Supersedes / superseded by:** none — first entry establishing this
convention.

---

### D-0057 — README as a human trust gateway; audience-door docs  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 1, Directive 13, founder review of the GitHub front page, `README.md`, `docs/HUMAN_START.md`/`DEVELOPER_START.md`/`AUDITOR_START.md`.

**Decision:** restructure `README.md` from an engineering brief into a
five-layer "century front door" — human promise, constitutional guarantees,
current honest reality, proof/start paths, and an audience router — and move
the deep engineering material (build commands, the runnable demos, the full
crate table, the launch-gap list, the FREEZE-domain reading order) into three
audience-specific door documents: `docs/HUMAN_START.md` (curious person),
`docs/DEVELOPER_START.md` (build/run/navigate), `docs/AUDITOR_START.md`
(invariants/threats/gates/how-to-verify). Also: converted every in-repo GitHub
issue link to repo-relative form (`../../issues/N`) so links survive account
renames, and updated the two spots that require an absolute owner name
(`Cargo.toml` repository field, the CLAUDE.md rename note) to the current
`mininet-labs/Mininet`.

**Reason:** founder review observed the front page opened like an internal
engineering dossier (founder directives, build commands, decision-log
discipline) rather than a doorway a curious human, a hostile auditor, a new
developer, and a future maintainer could each enter in the right order. For a
project meant to serve people for centuries (Directive 13) and to put
"humanity before technology" (Directive 1), the first screen must be a
civilizational doorway, not a repository summary — while preserving, not
diluting, the honesty about what is prototype and what is missing (that
honesty is itself a standing norm, D-0037-adjacent). The relative-link
conversion was prompted by the `britak420 → mininet-labs` account rename
exposing how many absolute owner-scoped links the docs carried.

**Constitutional impact:** implements Directive 1 (humanity before
technology — the front page now speaks to a person first) and Directive 13
(think in centuries — the doorway is written for a reader 100 years out). No
frozen invariant touched; no code behavior changed. The honesty requirement is
preserved and made *more* prominent (the "what exists / prototype / not ready"
trio and the gates pointer are now first-screen, not buried).

**Implementation status:** shipped. README restructured; three door docs
created; all engineering detail preserved (moved, not deleted — the crate
table, build/demo commands, and launch-gap list now live in
`DEVELOPER_START.md`). Doc-comment-only source edits (relative links in two
crate files) verified via the full workspace suite.

**Failure point:** a slimmer README can drift out of sync with the door docs
it now delegates to (e.g. the crate count, the D-number range). Mitigation:
the door docs are the single source for their detail, and the README carries
only summary claims that change rarely. A subtler risk: a "human-friendly"
front page could soften the honest limitations into marketing — explicitly
guarded against by keeping the "nothing here is ready for real people, real
money, or real custody yet" line on the first screen.

**Required follow-up:** none required. Optional, and only the founder can do
it (repo settings, not a file in the tree): set the GitHub repo
description/topics (currently "No description, website, or topics provided"),
and — flagged during this session — a founder personal email is exposed in
public merge-commit history, worth addressing via GitHub's private-email
setting and a commit-author rename for a privacy-focused project.

**Supersedes / superseded by:** none — refines the presentation established
across earlier README updates without reversing any of their content.

---

### D-0058 — `mini-settlement`: rename `nonce` field to `sequence` (terminology correction, not a design change)  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** D-0055, `crates/mini-settlement/`, GitHub code scanning (CodeQL) on PR #95.

**Decision:** rename `PaymentClaim`'s `nonce: u64` field, and every
identifier derived from it (`finalized_nonce` → `finalized_sequence`,
`tampered_nonce` → `tampered_sequence`, related test names), to `sequence`
throughout `mini-settlement`'s public API and docs. No field type, no
ordering rule, no signed byte layout, and no test assertion changed —
this is a rename only.

**Reason:** GitHub's CodeQL scan flagged 42 "critical" alerts against
`crates/mini-settlement/src/claim.rs` and `reconcile.rs` under
`rust/hard-coded-cryptographic-value` (CWE-798), each one an integer
literal passed into a parameter or field literally named `nonce`. All 42
were false positives: `mini-settlement` has no cryptographic nonce
anywhere — `mini_crypto::SigningKey::sign` is deterministic Ed25519
signing with no caller-supplied nonce. `PaymentClaim.nonce` was always a
monotonic per-payer *sequence number* for double-spend slot detection
(D-0055's own claim-message docs already described it that way), and
CodeQL's heuristic keys on the field/parameter *name*, not on any actual
cryptographic use. Renaming to the name that already describes what the
field does resolves the false positives and removes a name that would
mislead the next reader (or the next CodeQL run) into assuming
cryptographic content that was never there.

**Constitutional impact:** none. No frozen invariant is touched — M1, M2,
and M3 are enforced by `reconcile()`'s control flow and
`SettlementState`'s type structure, neither of which this rename changes.
Not a supersession of D-0055's protocol decision, only a correction to
that entry's own prose, which called the field a "nonce" — read
`sequence` wherever D-0055 says `nonce`; D-0055 itself is left unedited
per this log's append-only rule.

**Implementation status:** shipped — all 26 `mini-settlement` tests pass
unchanged in substance (only names differ); `cargo fmt`/`clippy -D
warnings` clean; `docs/gates/crypto-audit-scope.md`'s audit question
about the claim-message tuple updated to say `sequence` so the auditor
scope package matches the real field name.

**Failure point:** none introduced — this is a pure rename with no
behavioral surface. The only residual risk is documentation drift if a
future edit reintroduces "nonce" language for this field without
noticing the collision this entry records.

**Required follow-up:** none. Once this lands, PR CodeQL should show zero
alerts for `mini-settlement`; if a future crate genuinely needs a
cryptographic nonce, name it plainly (`nonce`) there — this entry is not
a ban on the word, only a correction of one specific misuse.

**Supersedes / superseded by:** corrects terminology used in D-0055's
prose without reversing or amending D-0055's decision itself.

---

### D-0059 — `mini-treasury`: zeroize FROST nonces on drop, gate trusted-dealer keygen behind an explicit acknowledgment  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** D-0048, [roadmap #93](../../issues/93), `crates/mini-treasury/src/frost_sign.rs`, `crates/mini-treasury/src/frost_keygen.rs`, CLAUDE.md's typed-domain rule.

**Decision:** two independent hardening changes to `mini-treasury`'s FROST
prototype, both named as P0 gaps in D-0048:

1. `SigningNonces` now implements `Drop` (zeroizing both scalars via
   `curve25519-dalek`'s `zeroize` feature) and a hand-written `Debug` that
   redacts them — the same redaction discipline `mini_crypto::SigningKey`
   already uses. `Copy` is removed (`Drop` and `Copy` are mutually
   exclusive in Rust, and `Copy` on a self-zeroizing secret would leave
   un-zeroized duplicate copies behind, defeating the point); `Clone` is
   also removed to keep every `SigningNonces` single-owner by construction.
2. `trusted_dealer_keygen` now takes a required
   `AcknowledgedPrototypeOnly` parameter, constructed only via
   `AcknowledgedPrototypeOnly::insecure_trusted_dealer_keygen_is_not_production_ready()`
   — the same typed-authority pattern CLAUDE.md requires for anything that
   exercises real authority: a specific named type a reviewer can see in a
   diff, not a bare function call easy to miss.

**Reason:** D-0048 named both gaps explicitly as P0 blockers on real
treasury value ("Nonce zeroization is not implemented" / trusted-dealer
keygen's exposure window). This closes the nonce-zeroization half
completely and adds real, compiler-enforced friction to the
trusted-dealer-keygen half (not a substitute for DKG itself — see (3)
below — but a mechanical guard against it being reached by accident or
"just for a testnet, just temporarily," the exact failure mode D-0048's
own failure point names).

**Constitutional impact:** implements Directive 2 ("assume every
authority is compromisable" — a party that briefly holds the whole secret
should not be reachable without saying so out loud) and CLAUDE.md's
typed-domain hard rule. No frozen invariant changed; FROST's signing
math, wire format, and test-proven correctness (D-0041) are untouched —
this is hardening around the existing protocol, not a new one.

**Implementation status:** shipped. All `mini-treasury` tests updated to
pass `AcknowledgedPrototypeOnly` explicitly; new test confirms `Debug`
output on `SigningNonces` never contains the raw scalar bytes.
`cargo fmt`/`clippy -D warnings`/`cargo test --workspace --all-features`
clean. `examples/frost_live_demo.rs` updated to construct the
acknowledgment once, at the one call site that needs it.

**Failure point:** zeroization here is `curve25519-dalek`'s ordinary
`Zeroize::zeroize()` on drop — best-effort, not a hardware-backed or
compiler-reordering-proof guarantee (the same honest caveat every other
zeroize use in this workspace carries, per CLAUDE.md's "no new
cryptographic primitives" rule: this composes an existing reviewed
mechanism rather than inventing one). `AcknowledgedPrototypeOnly` is a
marker with zero runtime behavior — it stops accidental reachability, not
a determined caller who explicitly decides to misuse a prototype in
production; the real fix for that is D-0048's other half, DKG itself,
which this entry does **not** implement.

**Required follow-up:** [roadmap #93](../../issues/93) remains open for
the DKG half — `DkgParticipant`/`DkgRound1`/`DkgRound2`/`DkgTranscript`
replacing `trusted_dealer_keygen` for any production custody use, per
`docs/gates/dkg-audit-scope.md`. This entry does not close #93, only its
nonce-zeroization sub-scope.

**Supersedes / superseded by:** does not supersede D-0048 — D-0048's P0
classification of trusted-dealer keygen stands exactly as written; this
entry records that one of its two named implementation gaps (nonce
zeroization) is now closed, and the other (DKG) is now harder to reach
by accident while remaining open.

---

### D-0060 — `mini-treasury`: real FROST DKG (Pedersen) and committee resharing, closing D-0048's remaining implementation gap  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 2, Directive 14, Directive 15, D-0048, D-0059, [roadmap #93](../../issues/93), `docs/gates/dkg-audit-scope.md`, `crates/mini-treasury/src/frost_dkg.rs`, `crates/mini-treasury/src/frost_reshare.rs`.

**Decision:** implement real distributed key generation
(`frost_dkg`, Pedersen DKG per RFC 9591 §4) and committee resharing
(`frost_reshare`, `KeygenMode::ReshareFromPreviousEpoch`) for
`mini-treasury`'s FROST custody prototype:

- **DKG:** every participant runs an independent Feldman VSS of their own
  random polynomial; the group secret is the additive sum of every
  non-excluded participant's contribution. No dealer, because there is no
  single party — not even briefly — who ever holds the full secret. A
  Schnorr proof of knowledge on each round-1 package's constant-term
  commitment prevents rogue-key attacks (a participant choosing their
  commitment as a function of others' already-published ones to bias the
  group key).
- **Misbehavior handling: exclude-and-continue via a self-verifying
  complaint/rebuttal mechanism**, not abort-and-restart. A recipient whose
  Feldman check on a received share fails may file a `DkgComplaint`; the
  accused gets one chance to publicly re-disclose the same value
  (`DkgRebuttal`) so every participant can independently verify the truth
  — Feldman's equation has exactly one satisfying value for a fixed
  commitment vector, so neither a genuinely bad sender nor a lying accuser
  can produce a false-looking result. `dkg_resolve` is a pure function of
  this public transcript: every honest participant computes the identical
  exclusion set with no voting, no consensus round, no coordinator
  authority beyond "everyone saw the same broadcast."
- **Resharing:** an active, threshold-sized old-committee subset
  redistributes the *same* group secret to a new (possibly differently
  sized) committee via Lagrange-weighted sub-sharing
  (`sum_i lambda_i*s_i == f(0)`, the identity `frost_sign::
  lagrange_coefficient`'s own tests already check), reusing DKG's
  commitment/proof-of-knowledge/complaint machinery unchanged.
  `reshare_finalize` independently recomputes and checks the resulting
  group public key against the old committee's, rather than only trusting
  the algebra.
- Both paths produce ordinary `KeyPackage`/`PublicKeyPackage` — identical
  to `trusted_dealer_keygen`'s output — so `frost_sign` needed zero
  changes; a DKG-or-reshared key signs through the existing FROST signing
  code unmodified (directly tested, both paths).
- Every DKG/reshare call site must pass an explicit
  `AcknowledgedUnauditedDkg`, the same typed-authority pattern D-0059
  established for `trusted_dealer_keygen`'s `AcknowledgedPrototypeOnly` —
  a distinct type because the honest limit differs ("unaudited," not
  "briefly centralized").

**Reason:** D-0048 named two P0 gaps: nonce zeroization (closed by
D-0059) and DKG itself. The founder's explicit direction on this batch —
checked against Directive 2 ("if removing one entity destroys Mininet,
the design has failed," applied narrowly: a DKG a single bad actor can
indefinitely stall by repeatedly forcing full restarts is not resilient
to one compromised participant) and Directive 15 ("welcome the honest
majority without trusting the malicious minority") — was exclude-and-
continue over abort-and-restart. Directive 14 (simplicity) is satisfied
without inventing new consensus machinery, because Pedersen DKG's own
complaint/rebuttal protocol (Pedersen 1991; Gennaro, Jarecki, Krawczyk &
Rabin) is already self-verifying against public data — implementing it
faithfully, rather than a bespoke voting mechanism, is both the simpler
and the more resilient choice, not a tradeoff between them.

**Constitutional impact:** implements Directive 2, Directive 14, and
Directive 15 directly in the DKG misbehavior-handling design; upholds
CLAUDE.md's typed-domain hard rule (`AcknowledgedUnauditedDkg`) and
"no new cryptographic primitives" rule (Pedersen DKG, Feldman VSS, and
Schnorr proofs of knowledge are all standard, already-composed
constructions — no bespoke crypto invented). No frozen invariant
touched. Does not itself close D-0048's P0 classification — see
"Implementation status."

**Implementation status:** shipped — 22 new tests across `frost_dkg.rs`
(14) and `frost_reshare.rs` (8, covering the checklist in
`docs/gates/dkg-audit-scope.md`: rogue-key rejection, malformed
commitments, session-replay rejection, missing shares, equivocation,
false-accusation resistance, resharing exclusion, tampered resharing
contributions, and group-key preservation through a full DKG-or-reshare-
then-sign round trip). All 50 `mini-treasury` tests pass; `cargo fmt`/
`clippy -D warnings`/full workspace `cargo test --all-features` clean.
`docs/gates/dkg-audit-scope.md` rewritten from a pre-implementation sketch
to describe the real shape, so the auditor maps straight to source.

**Failure point:** this is architecturally real but **not externally
audited** — D-0048's P0 classification of the DKG gap is not closed by
this entry, only its "not implemented at all" half. The complaint/
rebuttal mechanism's soundness (can a malicious participant falsely
accuse an honest one, or evade detection with a genuinely bad share) is
the single highest-value claim for an external auditor to try to break —
named explicitly as such in `dkg-audit-scope.md`. Resharing does **not**
revoke the old committee's shares (an old holder who doesn't delete their
key material can still reconstruct the secret after a "successful"
reshare) — a documented, code-cannot-close operational gap, not a bug.
No `TestOnlyTrustedDealer`-forbidden-at-mainnet runtime guard exists,
because no deployment-mode concept exists yet for one to gate against.

**Required follow-up:** [roadmap #93](../../issues/93) stays open —
external cryptography audit of `frost_dkg`/`frost_reshare` specifically
(separate scope from ordinary signing review, per D-0048's own framing).
A real production/deployment-mode guard forbidding `trusted_dealer_keygen`
outside tests is real follow-up work once #36-#45's chain/deployment
concept exists. FROST DKG's own remaining audit-scope gaps: an
aborting-mid-ceremony scenario beyond "share never arrives," and a
dedicated FROST-signing-nonce-reuse-attempt test (currently only covered
conceptually by D-0059's zeroize-on-drop).

**Supersedes / superseded by:** does not supersede D-0048 — closes the
DKG-implementation half of its two named gaps (nonce zeroization already
closed by D-0059), leaving only the external-audit requirement itself
open under #93.

---

### D-0061 — `mini-execution`: a real, chain-backed `CanonicalLedgerView`, closing roadmap #40  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 4, D-0045, D-0055, [roadmap #40](../../issues/40), [roadmap #41](../../issues/41), `crates/mini-execution/`.

**Decision:** build the smallest deterministic state machine that ties
`mini-chain`'s finality verification to `mini-settlement`'s reconciliation
— a new crate, `mini-execution`, rather than adding execution logic to
either existing crate (keeping `mini-chain` scoped to finality math and
`mini-settlement` scoped to protocol logic, each independently
reviewable, per this tree's established crate-boundary discipline):

- **`LedgerState`** — for each payer, only the *latest* finalized
  `(sequence, digest)` pair, because that is deliberately everything
  `mini_settlement::reconcile` ever reads from a `CanonicalLedgerView`.
  Implements `CanonicalLedgerView` directly; `commitment()` gives a block
  header's `state_root` real, checkable meaning (BLAKE3 over the
  canonically-sorted entries — `BTreeMap` iteration order makes this
  commitment order-independent by construction).
- **`apply_block`** — the state transition: within one block body, claims
  are processed in order (the canonical order M3 requires); a claim wins
  its `(payer, sequence)` slot only by strictly exceeding that payer's
  current high-water-mark. A bad signature, a stale or already-decided
  sequence, or a second claim at a just-taken slot is silently dropped —
  never merged, never partially honored (M1).
- **`LedgerChain`** — the one property that matters most: settlement state
  only ever advances behind a real, verified `mini_chain::QuorumCertificate`
  (`verify_finality`). There is no preview/tentative-apply path. Chain
  continuity (height, `prev_hash`) and state-root honesty (the header must
  match what the body actually produces) are both checked explicitly
  before any state change, not assumed.

**Reason:** D-0055 named this as its own required follow-up (`#40`'s
concrete mechanism), and `docs/INVARIANTS.md`'s M2/M3 rows have carried
"a real chain-backed `CanonicalLedgerView` is `pending`" since D-0055
shipped. Directive 4's exact test — "if two honest nodes can produce
different answers after reconciliation, the design is wrong" — was
previously only argued from the type system (M1/M2/M3's structure); this
batch makes it a property two independent `LedgerChain` instances
actually satisfy, checked by test.

**Constitutional impact:** implements Directive 4 directly and closes
M2/M3's "real chain-backed ledger" gap named in D-0045/D-0055. No frozen
invariant weakened — `reconcile`'s own rules (M1/M2/M3) are unchanged;
this crate is what makes them real instead of hypothetical. No new
cryptography (composes `mini-chain::verify_finality` and
`mini-settlement::verify_claim_signature`/`claim_digest`; the only new
content is deterministic bookkeeping and one content hash), so this is
not gated behind D-0047.

**Implementation status:** shipped — 14 tests (8 unit, 6 integration).
The integration suite directly proves: a double-spend across two
competing block-body proposals resolves to exactly one finalized winner
end to end (claim → block → real quorum certificate →
`LedgerChain::apply_finalized_block` → `mini_settlement::reconcile`
reading the resulting `LedgerState`); two independent `LedgerChain`s fed
the identical finalized-block sequence converge to bit-identical state
commitments; an unfinalized block (below-quorum votes), a wrong height, a
wrong parent hash, and a dishonest `state_root` are each rejected without
changing state. `cargo fmt`/`clippy -D warnings`/full workspace
`cargo test --all-features` clean.

**Failure point:** this is the state-machine piece only — it is **not**
networked consensus. Given a `(header, body, qc)` triple, this crate
answers "is this the next state" precisely; it has no opinion on how a
real network produces proposer rotation, vote gossip, or round timeouts/
view-change to actually generate that triple in the first place. Those
remain roadmap #36-#45's job, unchanged by this entry. `mini-execution`
also knows exactly one transaction type (`PaymentClaim`) — a real chain's
full state machine (governance, storage receipts, bounty claims) is
further, separate work.

**Required follow-up:** [roadmap #36-#45](../../issues/36) — the
networked BFT protocol itself. Once that exists, wiring it to
`LedgerChain::apply_finalized_block` is the remaining integration step;
this entry's state-machine logic does not need to change for that to
happen.

**Supersedes / superseded by:** does not supersede D-0045 or D-0055 — M1/
M2/M3's meaning is unchanged; this closes the specific "real chain-backed
implementation" gap both entries already named as outstanding.

---

### D-0062 — `mini-bootstrap`/`mini-sync`: proven live over real TCP, closing roadmap #23  ·  *Accepted*
**Date:** 2026-07-09 · **Refs:** Directive 14, [roadmap #23](../../issues/23), D-0042, `crates/mini-bootstrap/examples/bootstrap_live_demo.rs`, `crates/mini-sync/tests/sync_over_tcp.rs`.

**Decision:** prove `mini-bootstrap` and `mini-sync` interoperate over a
real socket, without adding any transport code to either crate's own
library API:

- `mini-sync` was already `Bearer`-generic (`sync_bidirectional`/
  `serve_pull` take `&mut dyn Bearer`) — its own docs already claimed
  "over any bearer." What was missing was a test actually exercising
  `TcpBearer` instead of only `InProcessBearer`. Added
  `tests/sync_over_tcp.rs`: two real threads, a real `TcpListener`/
  `TcpStream` pair on localhost, the same `Channel` handshake and
  `sync_bidirectional` call every other `mini-sync` test uses — proving a
  fresh peer pulls everything over the real socket, and two peers with
  disjoint content converge to an identical set.
- `mini-bootstrap` was **not** transport-generic at all (by design — its
  own crate docs say it never gets its own wire protocol; real transport
  is explicitly `mini-bearer`'s job). Rather than write a new bootstrap-
  specific wire protocol, `examples/bootstrap_live_demo.rs` composes three
  already-real pieces exactly as a real device would: the seed peer sends
  a `GenesisSeed` first (standing in for a BLE advertisement), the two
  sides handshake a `mini_bearer::Channel`, then `mini_sync::
  sync_bidirectional` pulls everything — the capsule header, its bundle
  manifest, and every chunk are already just `mini_objects::Object`s in a
  `mini_store::Store`, so ordinary bucketed set reconciliation is the
  entire data-transfer mechanism. A genuinely fresh device (empty store,
  empty `KelCache` — zero prior trust) reassembles and digest-verifies the
  bundle, byte-identical to what the seed peer published.

**Reason:** roadmap #23 asked to "prove a device can actually bootstrap
from a peer over a real connection, not just in-process." Writing a new
bootstrap-specific wire protocol would have duplicated what `mini-sync`
already solved (Directive 14: the strongest protocol is the one that
removed the most unnecessary parts) and would have contradicted
`mini-bootstrap`'s own documented transport-agnostic design. Composing
existing, already-tested pieces — rather than adding a fourth wire
protocol to the workspace — was the smaller, more honest answer, and
directly demonstrates why `mini-bootstrap` publishing ordinary
content-addressed objects (not a bespoke format) was the right call back
when it first shipped.

**Constitutional impact:** none — no frozen invariant touched, no new
cryptography (composes `mini-bearer::Channel`'s existing handshake/AEAD
and `mini-sync`'s existing verified-ingest pipeline unchanged). Directive
14 (simplicity) is the operative principle: the "gap" turned out to be a
missing *demonstration*, not missing *code*, for `mini-sync`; and for
`mini-bootstrap`, composition rather than a new protocol.

**Implementation status:** shipped. `sync_over_tcp.rs`: 2 new tests, both
passing. `bootstrap_live_demo.rs`: run manually as two real OS processes
(`cargo run ... -- seed 9100` / `cargo run ... -- fresh 127.0.0.1:9100`)
— confirmed producing byte-identical BLAKE3 digests on both sides over an
actual TCP connection. `cargo fmt`/`clippy -D warnings`/full workspace
`cargo test --all-features` clean.

**Failure point:** TCP stands in for BLE — the demo proves the protocol
pieces interoperate over a real socket, not that real BLE/Wi-Fi radio
adapters work (those need actual phone hardware this environment doesn't
have, roadmap #22, unchanged by this entry). One connection, one capsule:
no peer discovery, no multi-peer store-and-forward resumption across many
short encounters (that robustness testing is `mini-sync`'s own separate
scope, roadmap #26).

**Required follow-up:** roadmap #22 (real BLE/Wi-Fi radio `Bearer`
implementations) remains the actual hardware-dependent blocker; once a
real radio `Bearer` exists, this same `sync_bidirectional`/`Channel`
composition should work unchanged, since neither `mini-bootstrap` nor
`mini-sync` cares what implements `Bearer`.

**Supersedes / superseded by:** none — first live-transport demonstration
for these two crates.

---

### D-0063 — Clarify "no new cryptographic primitives": published, real-world-proven constructions are composition, not invention  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** Directive 2, Directive 14, founder direction, CLAUDE.md's hard-rules section, [roadmap #31](../../issues/31).

**Decision:** when scoping real proof-of-replication for `mini-spacetime`'s
named gap (#31), the founder explicitly directed: *"we absolutely should
implement crypto that has been proven working as tech for other projects,
but we should keep governance rather than outsource the whole system, that
is why you must do the coding for all crypto at first as you can use tech
from several projects."* This clarifies (does not weaken) CLAUDE.md's "no
new cryptographic primitives" rule: implementing an already-published,
peer-reviewed, real-world-deployed construction **end-to-end in-house** —
Filecoin's SDR (Stacked Depth-Robust Graphs) proof-of-replication being
the immediate case, `mini-value`'s Bulletproofs (D-0036/D-0040) the
existing precedent already in the tree — is composition of prior art the
wider field has already analyzed, not invention of new cryptography. What
remains forbidden, unchanged, is a genuinely novel, unreviewed
cryptographic design nobody outside this repo has ever scrutinized.
CLAUDE.md's hard-rules section is updated to state this distinction
explicitly, so future sessions don't have to re-derive it.

**Reason:** Directive 2 ("assume every central authority will eventually
fail... every dependency should be assumed temporary") argues against
*depending on* another project's running code/service for a security-
critical primitive — vendoring or wrapping an external library still
means trusting that project's maintainers, release process, and supply
chain indefinitely. Implementing the same published construction
ourselves keeps the code inside this repo's own governance and audit
boundary (D-0037/D-0047) while still only ever using techniques that have
already survived real-world adversarial deployment (Filecoin mainnet, in
SDR's case) — the opposite of inventing something new and untested.
Directive 14 (simplicity) is not in tension with this: SDR is not simpler
than not-having-replication-proof, but among constructions that solve the
replication-uniqueness problem at all, it is the one with the most
real-world scrutiny, which is the relevant "simplicity" comparison here
(fewest unknowns), not raw line count.

**Constitutional impact:** clarifies, does not weaken, CLAUDE.md's hard
rule (not a Tier-F `docs/INVARIANTS.md` row, so no invariant is touched).
Applies going forward to any future primitive-selection decision, not
just #31 — the test is "has this construction survived real-world
adversarial deployment and independent publication," not "did we invent
it here."

**Implementation status:** rule text updated in `CLAUDE.md`. The actual
proof-of-replication implementation this unblocks is `mini-porep`,
recorded separately as D-0064.

**Failure point:** "proven working for other projects" is a judgment call
per construction, not a blanket license — a scheme with only academic
publication and no real-world deployment history is a weaker claim than
SDR's Filecoin-mainnet track record, and should be named as such
explicitly (per the honesty-over-polish rule) rather than implicitly
treated as equally proven. This entry does not pre-approve every
published construction; it approves the *category* of reasoning.

**Required follow-up:** none — this is a standing interpretive
clarification, not a task.

**Supersedes / superseded by:** none — clarifies CLAUDE.md's existing
rule rather than replacing it.

---

### D-0064 — `mini-porep`: real proof-of-replication (Stacked Depth-Robust Graph sealing), closes roadmap #31  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0063, D-0037/D-0038/D-0039, [roadmap #31](../../issues/31).

**Decision:** ship a new crate, `mini-porep`, implementing a real (if
deliberately simplified) Filecoin-style Stacked Depth-Robust Graph (SDR)
proof-of-replication: `drg.rs` generates a per-layer depth-robust parent
graph (one sequential predecessor plus ~5 pseudorandom long-range
back-edges, degree 6); `seal.rs` computes stacked layered labels over that
graph (`label(0,i) = H(replica_id,0,i,D_i)`; `label(L,i) = H(replica_id,
L, i, [same-layer DRG parent labels], label(L-1,i))` for `L >= 1`) and the
final XOR-encoded replica (`R_i = label(num_layers,i) XOR D_i`);
`audit.rs` provides a registration-time probabilistic audit (random
`(layer,node)` challenges, direct hash recomputation against
pre-published Merkle roots) as the explicit substitute for a zk-SNARK
sealing circuit, which was judged too large and too risky to build
correctly from scratch this pass; `challenge.rs` provides ongoing
possession challenge-response by directly composing
`mini_spacetime::MerkleStorageProof`/`StorageCommitment` against the
sealed replica's own root (reuse, not duplication, of the existing PDP
machinery — same storage-risk domain), and `PorepStorageProof` implements
`mini_spacetime::ProofOfSpaceTimeSource` so `mini_spacetime::
proposer_weight` requires zero changes to consume it.

**Reason:** `mini_spacetime::storage_proof`'s own docs name the gap this
closes explicitly: Merkle/PDP possession challenges cannot distinguish a
thousand honest small devices each holding their own copy from one
warehouse machine holding a single copy and answering every challenge on
behalf of many claimed identities — exactly the attack the whitepaper's
"a thousand cheap, scattered machines outcompete one warehouse" thesis
depends on resisting. Making sealing genuinely, provably sequential
(shortcutting layer `L` requires having already computed all of layer
`L-1`, transitively down to layer 0) means producing `k` replicas costs
approximately `k` times the real work, closing the shortcut a
warehouse would otherwise exploit. Per D-0063's founder-directed
clarification, implementing this specific published, peer-reviewed,
real-world-deployed (Filecoin mainnet) construction end-to-end in-house is
composition of prior art, not invention of new cryptography, and keeps
the code inside this repo's own governance boundary rather than depending
on an external project's runtime.

**Constitutional impact:** advances `docs/STATUS.md` §7 (Storage) from
"real proof-of-replication is not started (#31)" to prototype/real-code
status; does not touch any Tier-F `docs/INVARIANTS.md` row. Continues to
respect the hard limitation that proof-of-space-time proves possession
(now: possession of a genuinely, provably sequentially-sealed replica),
not replication uniqueness at the level of "verified human," which
remains a separate, unsolved Sybil question (Directive-traced hard
limitation, unchanged).

**Implementation status:** real, tested code — 30 unit tests across
`drg`/`seal`/`audit`/`challenge`, including adversarial coverage: tampered
labels/parent-labels/data-nodes fail verification, a response fabricated
against a different replica's commitment fails, claiming a top-layer
replica leaf below the top layer fails, a "lazy prover" who fabricates
self-consistent-but-fake Merkle-committed labels without ever running the
real hash chain fails the audit, and changing an early data node
demonstrably ripples through to the final layer's labels (the sequential-
dependency property the whole construction rests on). Founder-reviewed
AI-authored prototype, **not externally audited** — same D-0047 gate every
other `mini-value`/`mini-treasury` prototype in this tree already carries.

**Failure point:** the DRG is a **simplified** construction — a sequential
edge plus pseudorandom long-range edges, degree 6 — not a byte-for-byte
reproduction of Filecoin's production `BucketGraph` probability-weighted
bucket-sampling distribution (reproducing that exact distribution from
memory was judged too much precision risk to get right from scratch). The
registration audit is **probabilistic, not succinct**: it is real spot-
check evidence, not a single small universally-checkable proof, and it is
non-zero-knowledge (reveals plaintext intermediate labels for challenged
indices) — accepted here because sealing isn't trying to keep data
confidential. Neither gap is hidden; both are stated in `mini-porep`'s own
crate docs and README per the honesty-over-polish rule.

**Required follow-up:** external cryptography audit (#72/#93's existing
gate, D-0047) before any real value or consensus weight depends on this;
wiring `PorepStorageProof` into an actual `mini-chain`/`mini-net` proposer-
selection path is separate future work, not part of this batch (this
crate only proves the `ProofOfSpaceTimeSource` seam is satisfied, the same
scope boundary `mini_spacetime::storage_proof` already drew for itself).

**Supersedes / superseded by:** none — first real proof-of-replication
implementation in this tree.

---

### D-0065 — `mini-erasure`: systematic Reed-Solomon erasure coding + self-healing repair, closes roadmap #30 and #32  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0063 (same in-house-composition reasoning applied to coding theory), [roadmap #30](../../issues/30), [roadmap #32](../../issues/32).

**Decision:** ship a new crate, `mini-erasure`, implementing systematic
Reed-Solomon erasure coding over `GF(2^8)` and a self-healing repair
layer built on it, per the founder's explicit "both together" scope
decision (erasure coding and self-healing storage tackled as one batch,
not sequenced). `gf256.rs` implements standard `GF(2^8)` arithmetic (the
same field, same reduction polynomial, used by QR codes, PDF417, and
RAID6); `matrix.rs` builds the systematic Vandermonde generator matrix
(top `k` rows identity, bottom `m` rows Vandermonde coefficients — the
maximum-distance-separable property guaranteeing every `k`-row subset
inverts) and Gauss-Jordan inversion over that field; `code.rs` is
`encode()`/`reconstruct()` — split data into `data_shards` pieces, compute
`parity_shards` more, recover from any `data_shards` of the
`data_shards + parity_shards` total; `health.rs` is the self-healing
layer — `plan_repair()` reports which shard indices can't currently be
trusted (missing, or present but failing a BLAKE3 integrity check, the
two treated identically), and `repair()` reconstructs the original data
and regenerates exactly the missing shards, ready for a caller to
redistribute.

**Reason:** plain replication tolerates `N-1` losses at `N x` storage
cost; systematic Reed-Solomon tolerates `parity_shards` losses at only
`(data_shards+parity_shards)/data_shards x` cost, the standard reason
every large-scale storage system (RAID6, Backblaze, Ceph, IPFS's optional
erasure coding) uses it instead of naive copies — directly serving
Directive 1/3's "cheap, ordinary hardware, at real-world scale" framing
for roadmap Phase 4's storage layer. Detecting loss and regenerating
exactly the missing pieces (rather than re-uploading a whole file, or
worse, silently trusting a present-but-corrupted shard) is what makes the
loss-tolerance self-*healing* rather than merely loss-*tolerant* — closing
#32 as the natural consequence of #30's coding scheme, which is why the
founder scoped them as one batch. Coded in-house (not depended on as a
library) for the same reasoning D-0063 gives for `mini-porep`'s
cryptography: composing an already-published, real-world-deployed
construction ourselves keeps it inside this repo's own governance
boundary. Erasure coding is coding theory, not cryptography, so CLAUDE.md's
crypto-invention hard rule does not technically apply here — but the same
Directive 14 "prefer the well-trodden construction" reasoning does, and is
followed the same way without needing a rule amendment.

**Constitutional impact:** advances `docs/STATUS.md` §7 (Storage) from
"not started" to real, tested code for both #30 and #32; touches no
Tier-F `docs/INVARIANTS.md` row (storage redundancy mechanics are not a
frozen domain).

**Implementation status:** real, tested code — 27 tests: exhaustive
reconstruction from every valid `k`-of-`n` shard subset for small
parameters (not sampled), non-shard-aligned data lengths round-tripping
exactly, a corrupted (not just missing) shard being caught and healed
identically, losing more than `parity_shards` holders failing cleanly
with a typed error rather than silently returning wrong data, and an
end-to-end two-separate-outage healing cycle that still reconstructs to
the exact original bytes afterward. Founder-reviewed AI-authored
prototype.

**Failure point:** this crate proves the erasure-coding and repair
*logic*. Deciding which peer should hold a regenerated shard and
transferring it to them is a distribution problem, not a coding-theory
one, and is explicitly out of scope — `mini-net`/`mini-store`'s job,
unstarted. `gf256::inv` is brute-force search over 255 candidates rather
than log/antilog tables; correct and adequately fast for matrices this
crate's small `data_shards` values ever produce, but not tuned for
per-byte throughput at scale — a later performance pass, not a
correctness gap.

**Required follow-up:** wiring `mini-erasure` into `mini-store`/`mini-net`
so shards are actually distributed to and repaired across real network
holders is separate future work; a storage economic-incentive review for
who gets paid to hold parity shards remains roadmap #33, untouched by this
batch.

**Supersedes / superseded by:** none — first erasure-coding/self-healing
implementation in this tree.

---

### D-0066 — Adopt external audit's sequencing: pause horizontal crate breadth, build the vertical self-hosted developer spine first  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** founder-adopted external technical assessment (2026-07-10), Directive 1, Directive 2, Directive 14, roadmap hub #92.

**Decision:** a founder-commissioned external auditor reviewed the repository
and delivered a technical assessment, which the founder has adopted as
direction. Its central finding: implementation breadth (identity,
presence, storage rewards, confidential value, treasury custody,
settlement, finality verification, social objects, forge) has run ahead
of *vertical integration* — there is no complete path from a developer's
change through review, governed merge, reproducible build, release
finality, safe installation, health check, and rollback. This session
adopts the auditor's recommended re-sequencing: pause opening new
horizontal protocol-prototype crates for unstarted roadmap phases, and
build that one narrow vertical "forge loop" end to end first, in the
six-batch order the report lays out (see
`docs/design/self-hosted-forge-spine.md` for the adapted plan):
1. developer spine (CLI + local daemon + Git bridge + proposal/review/
   merge metadata + machine-readable status), 2. in-house
   WASI/Wasmtime-sandboxed build pipeline, 3. TUF-style release
   verification (root/targets/snapshot/timestamp, expiry, rollback
   protection, independent-builder quorum), 4. a real transactional
   installer (`mini-installer`: stage → preflight → atomic activate →
   health-check → rollback), 5. making Mininet itself the primary forge
   (P2P proposal/review sync, so development survives a GitHub outage),
   6. only then resume broader protocol work (networked consensus, real
   BLE/UWB, personhood research, economics, anonymous value,
   proof-of-replication depth).

**Correction to the audit for the record:** the report states there is
"no complete vertical path" and separately recommends building a
"proposal/review/merge" object model as the first concrete PR, describing
it as new work. `crates/mini-forge/src/governance.rs` already implements
this: `propose()` creates a PR object binding an exact `head` commit and
`base` chain position; `approve()` records a verdict **bound to the exact
head commit reviewed** (invalidated by any later commit swap, exactly the
property the report asks for); `merge()`/`amend()` record chain entries;
`resolve_project()` deterministically walks the chain and counts quorum
in distinct verified identity roots, excluding the author, with fork
detection. This is real, tested, already-shipped code (predates this
session). What the report correctly identifies as missing around it:
review objects carry only an approve/reject bit, not free-text findings
or CI/test attestations bound to the reviewed commit; there is no
AI-assistance/human-owner metadata field; there is no way for a human to
actually drive any of this without hand-writing Rust against the library
API (no CLI, no daemon). Per Directive 14 and the honesty-over-polish
rule, the follow-up work in Batch 1 extends this existing model rather
than replacing it with a new one — see `docs/design/
self-hosted-forge-spine.md` for the itemized delta.

**Reason:** Directive 1/2 (build for centuries, assume every external
authority is temporary) is best served by a working self-sufficient
loop, not more prototype breadth that still depends on GitHub as the
de facto authority. Directive 14 (simplicity/composition of proven
constructions) is exactly why the report's recommendations (TUF-style
release metadata, WASI/Wasmtime sandboxing, in-toto/SLSA-style
provenance, Noise-framework handshakes) are the right shape: compose
already-published, real-world-deployed designs rather than invent new
ones, the same reasoning already recorded in D-0063 for `mini-porep` and
D-0065 for `mini-erasure`, now applied to the release/update layer where
the stakes (arbitrary code execution on a user's device) are highest in
the whole system.

**Constitutional impact:** does not touch any Tier-F `docs/INVARIANTS.md`
row by itself — this is a development-sequencing decision, not a change
to any frozen guarantee. It does re-affirm two existing FREEZE rules the
new work must never cross: no forced update / no kill path (mini-update's
existing invariant — `mini-installer` executes only what a device owner
already locally approved), and the voice/value wall (P1) — none of the
new forge-spine objects may ever gain vote weight from balance or
payment.

**Implementation status:** design doc and roadmap tracking added this
batch (`docs/design/self-hosted-forge-spine.md`, roadmap hub #92 update,
new tracking issue for the spine). `mini-cli` (Batch 1's first concrete
deliverable) is being built in this same PR — see the crate's own
README/decision-log follow-up entry for what shipped.

**Failure point:** re-sequencing does not itself close any gap; if this
batch stalls after the planning/tracking work, the actual auditor-
identified problem (no installable, safely-updatable client) remains
exactly as open as before. The exit condition for Batch 1
("two developers can exchange a signed proposed commit, review the exact
commit, and reach a governed canonical branch head without GitHub being
the authority") is the honest bar for calling this decision's intent
fulfilled, not merely opening a CLI crate.

**Required follow-up:** Batches 2-6 as scoped in `docs/design/
self-hosted-forge-spine.md`; the roadmap's existing horizontal items
(#33-#96 and beyond) resume only after Batch 4's installer exit condition
is met, per the auditor's explicit sequencing recommendation.

**Supersedes / superseded by:** none directly, but changes this
session's own prior operating assumption (continue through roadmap
breadth batch-by-batch) — that assumption is superseded by this entry's
sequencing for all future batches until Batch 4 completes.

---

### D-0067 — `mini-cli` + `mini-forge` review metadata: Batch 1's developer spine, first exit-condition demonstration  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0066, `docs/design/self-hosted-forge-spine.md`, tracking issue #102.

**Decision:** ship `mini-cli` (a new binary crate, the `mini` command) and
extend `mini-forge::governance` with two new, purely informational object
types — `declare_ai_assistance`/`ai_assistance` and
`record_findings`/`list_findings` — completing Batch 1 of the self-hosted
forge spine. `mini-cli` wraps `did-mini`/`mini-forge`/`mini-store`/
`mini-objects` in real commands (`identity init/show`, `kel export/trust`,
`repo init/track/commit/checkout/branch/status`, `pr propose/approve/
merge/ai-assisted/findings`) with a hand-rolled argument parser (no new
dependency, matching this workspace's existing "avoid a dependency where a
few dozen lines of plain Rust do the job" convention). Identity persists
across CLI invocations by replaying a deterministic inception +
device-delegation sequence from an on-disk seed file
(`SigningKey::to_seed_bytes`/`incept_single_from_seeds`); two `mini` homes
exchange signed objects via a shared `--store` filesystem path (a synced
folder, a USB stick — any medium that copies files), no new networking
code. `tests/two_developers.rs` demonstrates Batch 1's exit condition
directly: three independent homes reach a governed 2-of-3 merge, correctly
refusing to merge under insufficient quorum first, then converging on
identical canonical state as seen from a fully independent third home.

**Reason:** Batch 1's exit condition ("two developers can exchange a
signed proposed commit, review the exact commit, and reach a governed
canonical branch head without GitHub being the authority") requires a
human-usable tool, not just a library API — `mini-forge::governance`
already had the correct object model (predates this session, corrected in
D-0066) but nobody could drive it without hand-writing Rust. The AI-
assistance/findings additions answer the audit's real (not duplicated)
gap: reviews previously carried only an approve/reject bit. Both are
implemented as new, separate object types rather than changed payloads on
`propose`/`approve`, specifically so none of ~45 existing call sites in
`mini-forge`'s own test suite needed to change — Directive 14's "smaller
diff, well-trodden path" preference over a larger, riskier rewrite of
already-hardened quorum-counting logic.

**Constitutional impact:** the AI-assistance/findings objects are
explicitly, structurally excluded from quorum counting (never linked into
`quorum()`'s counting logic at all) — P1's voice/value wall precedent
("money never buys merge") extended to "metadata never buys merge"
either. No Tier-F `docs/INVARIANTS.md` row touched. Advances D-0066's
Batch 1 toward its stated exit condition (not yet fully closed — see
Required follow-up).

**Implementation status:** real, tested code. `mini-forge`: 21 governance
tests (6 new, including AI-assistance quorum-neutrality, rejection of an
AI-assisted declaration with no named human owner, and a non-author's
declaration on someone else's PR not being read back). `mini-cli`: 9 unit
tests (identity persistence round-trip, project aliasing) + 2 integration
tests (the full three-home governed-merge scenario, and an explicit
"untrusted author's project cannot be resolved" refusal case) + a manual
end-to-end shell smoke test during development that caught a real bug
(see Failure point) before the automated test was written.

**Failure point:** while building this, `mini kel export`/`trust`
initially handled only the human root's KEL. `mini-forge::oracle::
author_verified` needs *both* the human root's KEL and the signing
device's own KEL (`mini_objects::verify_provenance(object, root, device)`)
— exporting only the human KEL made every object silently unverifiable
from another home (`ForgeError::BadObject`, "malformed forge object")
despite the human KEL itself being correctly trusted. Fixed by bundling
both KELs in one `kel export`/`kel trust` exchange. Documented in
`mini-cli`'s own module docs as the reason both are required, not papered
over as an implementation detail. Remaining, named, honest gaps: no key
rotation from the CLI (full KEL persistence needed, not just seeds), no
`mini-devd` daemon, no live network sync (`mini sync`), no Git bridge, no
machine-readable status generation — all named explicitly in `docs/
design/self-hosted-forge-spine.md` as deferred fast-follows, none
blocking Batch 1's demonstrated exit condition.

**Required follow-up:** the deferred Batch 1 items above; Batch 2
(`mini-pipeline`, WASI/Wasmtime-sandboxed builds) is the next major piece
per the spine's sequencing.

**Supersedes / superseded by:** none — first CLI and first review-metadata
extension in this tree.

---

### D-0068 — `mini-provenance`: build provenance as signed objects (Batch 2a); Batch 2b (WASI/Wasmtime sandbox) deferred pending an explicit dependency decision  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0066, `docs/design/self-hosted-forge-spine.md`, tracking issue #102.

**Decision:** split Batch 2 of the self-hosted forge spine in two. Ship
`mini-provenance` now (Batch 2a): `record_provenance()` signs a builder's
environment digest, a commands/pipeline-recipe digest, every output digest
produced, network-enabled status, and a self-declared reproducibility
group, tied to a subject `ObjectId` (a commit or artifact);
`independent_agreement()` counts **distinct verified identity roots** —
excluding the subject's own author — that agree on a given output digest,
generalizing `mini_forge::verify_release_artifact_only`'s existing
independent-attestation pattern from *release* artifacts to the *build*
stage, before a release is even proposed. Defer Batch 2b (running build
steps inside WASI Preview 2 / the WebAssembly Component Model via
Wasmtime, with per-component declared capabilities) — that requires
embedding Wasmtime, a large dependency (cranelift JIT codegen, the
component model, ~20+ transitive crates), a genuine departure from every
dependency choice made elsewhere in this tree (no `rand`, no `clap`,
`mini-spacetime` depends on `blake3` alone, `mini-erasure`/`mini-cli`
hand-roll rather than reach for a crate). That tradeoff is named
explicitly rather than decided silently mid-session; `mini-pipeline`'s
manifest format is documented as a design, not implemented, until it is.

**Reason:** the founder-adopted external audit's specific, verifiable
critique was that this repository's current CI same-runner clean-rebuild
comparison "must never be called independent reproducibility." That
critique is answered directly by making "builder X reproduced digest D"
into a real, signed, independently-countable claim — exactly the pattern
`mini_forge::release` already uses for cut releases, now available before
a release exists at all. Introducing Wasmtime, by contrast, is not
answering a named critique with a proportionate fix; it is a large,
hard-to-reverse supply-chain decision on a project whose own audit is
specifically concerned about supply-chain minimalism (SLSA/in-toto
provenance, independent builders) — the kind of consequential,
architecture-shaping choice Directive 14 (simplicity, prefer the smaller
well-trodden path) and this session's standing practice both treat as
worth a deliberate decision, not a default.

**Constitutional impact:** no Tier-F `docs/INVARIANTS.md` row touched.
`independent_agreement`'s exclusion of the subject's own author mirrors
P1's "author never counts toward the quorum that approves their own work"
pattern, extended from merge quorum to build-provenance agreement.

**Implementation status:** real, tested code — 8 tests: rejection of
zero-output and finished-before-started claims, a full store round-trip,
three independent builders correctly counted, the subject's own author's
self-build correctly excluded, one builder signing from two devices still
counting once, disagreeing output digests not counted toward an unrelated
expected digest, and an unvouched builder (oracle never learns their KEL)
not counted at all.

**Failure point:** as stated in the crate's own docs — this proves
*distinct identity roots* agree, not *administratively independent
infrastructure*; three containers on one host under three keys one person
controls look identical to this crate. Named, not hidden, the same caveat
`mini_forge::release`'s docs already carry. Nothing in this crate executes
a build; it only records and counts claims about builds that ran
elsewhere, by whatever means (currently: nothing in this tree runs one).

**Required follow-up:** Batch 2b (WASI/Wasmtime sandboxed execution)
remains open pending an explicit founder decision on the Wasmtime
dependency; Batch 3 (TUF-style release verification) does not strictly
require Batch 2b to proceed and may be the next piece instead.

**Supersedes / superseded by:** none — first build-provenance
implementation in this tree.

---

### D-0069 — Adopt Wasmtime as the reference executor for untrusted pipeline components, isolated to a dedicated runner  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0066, D-0068, `docs/design/self-hosted-forge-spine.md`, tracking issue #102.

**Decision:** *"Adopt Wasmtime as Mininet's reference executor for
untrusted pipeline components. Wasmtime shall be isolated in a dedicated
build-runner process and shall not become a dependency of Mininet's
identity, chain, forge-policy, update-verification, or ordinary node
binaries. Pipeline capability declarations must be enforced by
construction with deny-by-default host interfaces; metadata-only
capability claims do not qualify for trusted provenance. Wasmtime
execution covers WebAssembly components only and is not represented as a
complete sandbox for arbitrary native toolchains. Native build tools
remain prohibited from trusted pipelines until a separate digest-pinned,
OS-isolated execution mechanism is implemented. The dependency is
accepted because sandboxing attacker-controlled build logic is a
specialist security boundary where an established implementation is safer
than a smaller Mininet-specific replacement."* — founder decision,
resolving the dependency question D-0068 left explicitly open.

**Reason:** the two rejected alternatives were each worse in a specific,
named way. Metadata-only capability declarations (`capabilities =
["network:none"]` on an otherwise-unrestricted process) would describe
desired behavior without enforcing it — the opposite of this tree's
honesty-over-polish rule, and explicitly barred from ever producing a
trusted build attestation. A home-grown OS sandbox (namespaces/seccomp/
Landlock on Linux, sandbox/AppContainer elsewhere) would become its own
multi-platform security project, and Wasmtime's import-based guest
capability boundary is the portable, already-adversarially-tested
alternative — this is Directive 14's "prefer the established, well-
trodden construction" reasoning (already applied to `mini-porep`'s SDR
construction, D-0063, and `mini-erasure`'s Reed-Solomon field arithmetic,
D-0065) now applied to a sandbox runtime instead of a cryptographic or
coding-theory construction. Architecturally isolating Wasmtime to one
replaceable binary (`mini-build-runner-wasmtime`) rather than the core is
what makes the large dependency acceptable at all: only machines
volunteering as build workers ever link it, never `mini-cli`, `mini-forge`,
`mini-chain`, identity, or the eventual update-verification/installer path.

**Constitutional impact:** no Tier-F `docs/INVARIANTS.md` row touched
directly, but this decision sets a standing constraint on all Batch 2b
work: Wasmtime may never appear in the dependency graph of any
identity/chain/forge-governance/update-verification/ordinary-node crate,
checked the same way the voice/value wall (P1) is checked on every
`Cargo.toml` diff. A `wasm-component` pipeline step may earn a trusted
build-provenance record (`mini-provenance`, D-0068); a `native-tool` (raw
shell) step may never earn one until its own separate OS-isolated
mechanism exists and is decided the same explicit way this decision was.

**Implementation status:** design only as of this entry — see
`docs/design/self-hosted-forge-spine.md`'s expanded Batch 2b section for
the full architecture (three-crate split, deny-by-default capability
model, out-of-process execution, resource limits, trimmed Wasmtime
feature set, dependency-governance checklist, the twelve-point exit
criteria). Implementation tracked as this session's immediate next work.

**Failure point:** Wasmtime does not, by itself, sandbox arbitrary native
build tools (`cargo build`, `npm install`, `bash build.sh` are host
processes, not Wasm guest instructions) — stated explicitly so this
decision is never later read as "the Rust build is now hermetic." Batch
2b's `wasm-component` step class is the only trusted-attestation-eligible
path until a separate native-tool sandbox is designed and decided.

**Required follow-up:** implement in the sequence the founder specified —
2b.1 (pure `mini-pipeline` manifest/policy types, no Wasmtime dependency),
2b.2 (the isolated `mini-build-runner-wasmtime` process), 2b.3 (adversarial
capability/resource tests against the twelve-point exit criteria) — before
Batch 3 (TUF-style release verification) resumes, though Batch 3 does not
strictly depend on 2b's completion.

**Supersedes / superseded by:** resolves D-0068's "Required follow-up"
(the Wasmtime dependency question left open pending founder input).

---

### D-0070 — Ship self-hosted forge spine Batch 3: TUF-adapted release verification (rollback protection, transparency log, freshness, provenance quorum)  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0066, D-0068, D-0069, `docs/design/self-hosted-forge-spine.md`, tracking issue #102.

**Decision:** ship Batch 3 of the self-hosted forge spine as four additive
gates layered in front of `mini_forge::verify_governed_release` rather
than a rewrite of it: (1) `mini_forge::release::{Version,
check_no_rollback}` — a strict dotted-numeric version type and
component-wise, zero-padded comparison refusing any non-upgrade; (2)
`mini_forge::release::{list_releases, detect_equivocation}` — a release
transparency log built directly on the object store's existing append-
only, content-addressed nature (no separate signed snapshot metadata
format), flagging any two releases for the same project/branch that claim
the same version but disagree on the artifact digest; (3)
`mini_update::FreshnessPolicy` — refuses an adoption decision if the
device's own caller-supplied `last_synced_ms` is too stale relative to a
policy-bounded ceiling (`FRESHNESS_MAX_ALLOWED_STALENESS_MS`, 30 days
provisional), checked before any governance gate runs; (4)
`mini_update::ProvenancePolicy` + `AdoptionState::evaluate_with_provenance`
— an optional additional gate requiring `mini_provenance::
independent_agreement` over the release's source commit to meet a
threshold, alongside (never instead of) `mini-forge`'s existing release-
attestation quorum. Adapted from TUF's root/targets/snapshot/timestamp
role separation to Mininet's identity-root/governance model, per
Directive 14: reuse existing object/index machinery instead of inventing
a parallel signed-metadata format.

**Reason:** the design doc's Batch 3 note named four concrete gaps against
TUF's role separation, all real and worth closing, none requiring a new
trust model — Mininet already has timelock + independent-attestation-
quorum release verification (`mini-forge::release`/
`verify_governed_release`); what was missing was rollback protection, a
queryable transparency log, a freshness/staleness bound, and a second,
independently-computed build-provenance quorum as defense in depth. Each
gate is layered in front of, not folded inside, the existing verification
function, because that function is deliberately stateless and these gates
each need either `mini-update`'s own device-local state or a second crate
(`mini-provenance`) that `mini-forge` must not depend on — keeping a
caller that never touches the new types seeing identical behavior to
before this batch.

**Constitutional impact:** no Tier-F `docs/INVARIANTS.md` row touched or
weakened. Reinforces the existing "no forced updates" freeze
(`mini-update`'s module doc comment, unchanged in substance): all four
gates are additional reasons `evaluate`/`adopt` can refuse or defer, never
a new path that installs, fetches, or executes anything — `AdoptionState`
still only records what the device owner explicitly chose. The
provenance-quorum gate repeats `mini-provenance`'s (D-0068) honest limit
verbatim: it counts distinct identity roots, not administratively
independent infrastructure.

**Implementation status:** shipped and tested — `mini-forge::release`
(`Version`, `check_no_rollback`, `list_releases`, `detect_equivocation`,
11 unit + integration tests), `mini_update::{FreshnessPolicy,
ProvenancePolicy, AdoptError, AdoptionState::evaluate_with_provenance}`
(14 integration tests covering every new gate's rejection and passing
paths plus every pre-existing path still working unchanged). See
`docs/STATUS.md` for the living detail.

**Failure point:** the freshness ceiling and the provenance-quorum
threshold are both caller-supplied policy values with only a loose upper
bound enforced in code (`FreshnessPolicy::validate`) — a device owner (or
a compromised client) can still choose values that make either gate
practically meaningless (a huge `max_staleness_ms` just under the
ceiling, or `min_independent_builders: 0`), the same class of "policy
value, not a code guarantee" caveat every timelock/quorum knob in this
tree already carries. `list_releases`/`detect_equivocation` are
read-only reporting: nothing in this batch automatically refuses an
equivocating release on a caller's behalf, since detection and adoption
policy are deliberately kept separate.

**Required follow-up:** Batch 4 (`mini-installer`, the state machine
that actually executes/downloads/activates a verified release) is the
next piece of this plan — none of Batch 3's gates change the fact that
`mini-update::AdoptionState::adopt` today only records a decision.

**Supersedes / superseded by:** none — first implementation of these four
gates in this tree.

---

### D-0071 — Ship self-hosted forge spine Batch 4: real installation, `mini-installer`  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0066, D-0070, `docs/design/self-hosted-forge-spine.md`, tracking issue #102, `docs/INVARIANTS.md` U1.

**Decision:** ship a new `mini-installer` crate implementing the design
doc's named state machine — `Discovered → Verified → Downloading → Staged
→ PreflightPassed → AwaitingOwnerApproval → Activating → HealthChecking →
Active` or `RolledBack` — as a real, type-state pipeline over an
already-verified `mini_forge::VerifiedRelease` (from `mini-update`/
`mini-forge`, which this crate never re-derives trust from). `stage()`
fetches genuine bytes from the store (`mini_media::assemble`) and
independently re-verifies the digest; `preflight()` re-reads and
re-verifies the staged bytes on disk immediately before activation;
`activate()` atomically flips a `current` symlink (temp-symlink +
`rename`) but only given an explicit, caller-constructed `OwnerApproval`
naming the exact release id it authorizes (the typed-domain rule:
authority-exercising functions take a specific named request type, never
a generic "approve"); `health_check()` runs a caller-supplied predicate
and automatically rolls back to whatever was running before on failure,
clearing `current` entirely (never leaving known-unhealthy software
marked active) if there was nothing to fall back to; `rollback()` is
directly callable and consumes its recorded pointer so repeated calls
fail cleanly instead of toggling between two releases.

**Reason:** the founder-adopted external audit named this the plan's most
safety-critical, most honestly-named gap — `mini-update::AdoptionState::
adopt` verifies and records a decision, but nothing in that crate
executes, fetches, or installs, by design (the no-forced-update freeze).
A separate crate is the only way to add real installation without
weakening that freeze: `mini-update` stays exactly what its own docs
already claimed it was, and the new capability (actually touching disk)
lives somewhere its own honest limits can be stated plainly, rather than
retrofitting `mini-update` into something its docs no longer accurately
describe. The typed `OwnerApproval` requirement is Directive 14's
composition-over-novelty instinct applied to authority, not cryptography:
reuse the tree's existing typed-domain convention (CLAUDE.md) instead of
inventing a bespoke authorization mechanism for this one crate.

**Constitutional impact:** touches Tier-F row U1 (`docs/INVARIANTS.md`,
"No forced auto-update / no off switch") — its "Enforced by" cell is
updated to name `mini-installer::Installer::activate`'s `OwnerApproval`
requirement alongside `mini-update::AdoptionState`'s existing entry. No
weakening: `activate` cannot be called without a caller-constructed
approval naming the exact release id, this crate never constructs one
itself, and a failed health check moves the device *backward* to known-
good software (or to nothing, if there was nothing before), never
forward — automatic rollback-to-safety is not a forced-update path.

**Implementation status:** shipped and tested — `mini-installer` (10
adversarial/integration tests against real files on real disk: full happy
path, digest mismatch at staging, on-disk corruption caught at preflight,
mismatched `OwnerApproval` refused, staged directory removed mid-flight
refused, failed health check rolling back to the prior release, failed
health check on the very first activation leaving nothing active,
rollback with nothing to undo erroring cleanly, rollback not toggling
back and forth, a full upgrade-then-rollback round trip). See
`docs/STATUS.md` §5/§10 for the living detail.

**Failure point:** Unix-only (`std::os::unix::fs::symlink`) — no Windows
support exists. No process supervision — this crate stages files and
flips a pointer; it does not start, stop, restart, or supervise any
process, so "activation" alone does not make newly staged software
actually run anywhere. No real package-manager/OS integration — wiring
the atomic pointer flip into an actual running system is explicitly left
to the caller. The health-check predicate is entirely caller-supplied;
this crate cannot itself judge whether newly activated software is
healthy, the same caller-supplied-policy pattern already accepted for
`mini_update::FreshnessPolicy`.

**Required follow-up:** wiring `mini-installer` into an actual running
system (service restart, binary-on-`PATH` swap, Windows support) is
explicitly out of scope and left to whatever concrete deployment target
adopts this crate next. Batch 6's stated exit condition (a deliberately
broken release detected and auto-recovered with a verifiable event
history) is demonstrated in this crate's own test suite, honestly
caveated as a real local disk in a test environment, not yet a live
distributed system — Batch 5 (Mininet as the primary forge) vs. resuming
Batch 6's horizontal roadmap breadth is the founder's next priority call,
not decided by this entry.

**Supersedes / superseded by:** none — first implementation of real
installation in this tree.

---

### D-0072 — Fix `mini-erasure`'s generator matrix: naive Vandermonde-append did not have the promised MDS property  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** D-0065, `crates/mini-erasure/src/matrix.rs`.

**Decision:** replace `mini-erasure::matrix::generator_matrix`'s
construction. D-0065 shipped it as an identity block (for `data_shards`)
with raw, un-normalized Vandermonde coefficient rows (`x_i^j`) appended
below it for the `parity_shards` rows. An external code review found a
concrete, reproducible counterexample: with `data_shards=4,
parity_shards=6`, the four rows at shard indices `[1, 4, 5, 9]` formed a
rank-3 (singular) matrix, meaning reconstruction from those four
surviving shards — a loss well within the nominal `parity_shards=6`
tolerance — would incorrectly fail with `SingularSubmatrix`. This was
independently reproduced against the exact shipped code before any fix
was written (`sub matrix: [0,1,0,0] [1,5,17,85] [1,6,20,120]
[1,10,68,146]`, confirmed singular). The fix builds a full
`(data_shards + parity_shards) x data_shards` Vandermonde matrix `V` and
normalizes it against its own top `data_shards x data_shards` block:
`G = V * V_top^-1`. `matrix.rs`'s own doc comment on `generator_matrix`
carries the full argument for why this — and not the naive append —
actually has the maximum-distance-separable (MDS) property for *every*
`k`-row subset, not just the identity block.

**Reason:** the naive construction's flaw is a standard, well-known trap
in do-it-yourself Reed-Solomon implementations — appending unrelated rows
below an identity block does not preserve the Vandermonde-determinant
argument that any `k` rows of a *single, consistent* Vandermonde matrix
are linearly independent, because the identity rows and the raw parity
rows are not expressed in the same basis. Normalizing the full matrix
against its own top block puts every row through the same linear map,
which is what actually preserves the MDS property for arbitrary subsets.
This is exactly the kind of mathematical-correctness bug D-0065's own
"in-house, well-trodden construction" framing (Directive 14) depends on
getting right — composing a published construction only holds if the
composition is actually equivalent to the published one, which the
original code was not.

**Constitutional impact:** none new — reinforces Directive 4
(correctness/determinism over implementation speed) against the same
crate D-0065 already cited it for. No `docs/INVARIANTS.md` row changes;
`mini-erasure` was never itself a listed invariant, only supporting
infrastructure for storage-fabric rows still marked `pending`.

**Implementation status:** fixed and tested — the exact counterexample is
now a permanent regression test
(`a_previously_singular_subset_is_now_invertible`), the pre-existing
exhaustive all-subsets test now runs across four `(data_shards,
parity_shards)` pairs instead of one, and a new randomized test samples
500 subsets of a larger `(10, 10)` configuration with a fixed seed for
reproducibility. All 29 `mini-erasure` tests (unit + integration) pass,
including the crate's pre-existing `self_healing_cycle.rs` integration
suite, unmodified.

**Failure point:** this fix addresses the generator-matrix construction
specifically; it does not constitute an external cryptographic/coding-
theory audit of this crate (still explicitly not done — see
`docs/STATUS.md`). The same external review raised several further
findings (execution-provenance binding, WASI capability-contract
precision, CI gate strictness, PoRep construction rigor, and others)
that are explicitly deferred to the pre-mass-launch external audit
process per standing founder direction, not addressed in this entry.

**Required follow-up:** none scoped here; the deferred findings above
remain tracked informally pending the external audit engagement
(`docs/STATUS.md` §9's "not started — an actual external audit
engagement").

**Supersedes / superseded by:** corrects a defect in the construction
D-0065 shipped; does not supersede D-0065's broader decision to build
Reed-Solomon in-house.

---

### D-0073 — Treasury economic model: XRPL/XMR two-bridge design replaces BTC/XMR framing  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** roadmap #47, D-0008 (XRPL settlement bridge),
`docs/gates/economic-simulation-spec.md`, `docs/design/
treasury-economic-model.md`, Directive 16 (P1 voice/value wall).

**Decision:** the whitepaper's original "BTC/XMR-to-MINI contribution
mechanism" framing is superseded by a three-part model: XRPL is Mininet's
public/banking-adjacent settlement bridge (already D-0008), Monero is its
private/censorship-resistant liquidity bridge, and treasury-contribution
minting is a distinct transaction type from ordinary bridge trading —
never silently conflated in the wallet UI. Bitcoin has no required role:
disabled at launch, may never become a primary bridge, pricing reference,
governance asset, or mandatory reserve. Full parameters (50/40/10 reserve
split and operating bands, monthly contribution epochs, weighted-median
valuation with ≥3 independent sources, 5% reserve-protection spread,
90-day linear vesting, 0.25%/yr issuance ceiling with a 1%-per-human-per-
epoch sub-cap, cellular ≤10%-per-vault custody, and an explicit governance
may/may-not list) are recorded in `docs/design/
treasury-economic-model.md` rather than duplicated here.

**Reason:** external capital must be able to enter Mininet and receive
economic value without ever purchasing political authority — the same
requirement Directive 16 already states for value/voice generally, applied
concretely to the bridge and treasury-contribution mechanisms specifically,
which the original BTC/XMR framing left unspecified.

**Constitutional impact:** applies Directive 16 / P1 (voice/value wall) to
the treasury-contribution and custody layer specifically — no contribution
size, custody role, or oracle role may ever translate into governance
weight (§11–§12 of the design doc). No `docs/INVARIANTS.md` row changes;
this is calibration and mechanism design within the existing wall, not a
new invariant.

**Implementation status:** design only. `mini-treasury::rate`/`receipt`
already exist as prototypes (D-0041/D-0055) but do not yet implement the
epoch mechanism, the swap-vs-contribution transaction-type split, the
weighted-median oracle, the 5%/90-day parameters, or cellular vault
custody described here. See `docs/STATUS.md`.

**Failure point:** the specific numeric parameters (reserve split, spread,
ceilings) are founder-set starting values, not values a simulation or an
external mechanism-design specialist has yet validated — `docs/gates/
economic-simulation-spec.md` still gates that calibration work, and
`docs/gates/dkg-audit-scope.md`/#93 still gates the chain-specific FROST/
XRP and FROST/XMR custody integration audits this design depends on.

**Required follow-up:** build the deterministic simulation harness and run
the 16-scenario stress list in `docs/design/treasury-economic-model.md`
§13; engage a mechanism-design/tokenomics specialist per `docs/gates/
economic-simulation-spec.md`; design external-receipt verification for
XRPL and XMR; implement the swap/contribution transaction-type split in
`mini-treasury`; external audit of chain-specific custody integrations
before any real value moves. Roadmap #47 stays open, retitled to
"Treasury contribution, XRPL/XMR bridge-liquidity, and reserve-allocation
audit," tracking exactly this follow-up.

**Supersedes / superseded by:** supersedes the interpretation of #47 as a
BTC/XMR contribution mechanism; does not supersede D-0008 (XRPL as
settlement bridge), which this decision builds directly on.

---

### D-0074 — Long-term issuance envelope and formal anti-whale wall  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** roadmap #50, `docs/gates/
economic-simulation-spec.md`, `docs/design/inflation-and-whale-resistance.md`,
Directive 16, Directive 17 (future-child test), Directive 13 (century
timescale), P1.

**Decision:** total gross annual MINI issuance is capped at **3%** of
circulating supply, split into three channels that can never be
reallocated into each other without a constitutional amendment: a
protected **2%/yr Human Share floor** (equal per active verified human,
365-day linear vesting, no wealth/age/hardware/history input in the
formula), up to **0.75%/yr** for concave, capped, per-human-limited
network-service rewards, and up to **0.25%/yr** for the treasury-
contribution mechanism (D-0073). Unused channel capacity expires rather
than accumulating. A formal anti-whale wall is adopted: MINI balance in
any form (direct, historical, delegated, locked, liquidity supplied,
contributions, fees paid, a balance-purchased credential, or any
wealth-derived proxy) must never be read by any function determining
vote weight, proposal eligibility/ordering, quorum, finality weight or
committee selection, treasury-signer eligibility, constitutional-review
authority, identity/personhood confidence, dispute/appeal rights,
moderation authority, governance-feed visibility, or protocol-update
approval. Full parameters, formulas, and the required 200-year adversarial
simulation suite are recorded in `docs/design/
inflation-and-whale-resistance.md`.

**Reason:** an unbounded or uncapped-channel emission schedule could let a
large early holder's position translate into disproportionate influence
over time even without any single governance-weight rule being violated —
the same concern `docs/THREAT_MODEL.md`'s "whale concentration" row
already named as an open, undecided distributional question. Bounding the
envelope and enumerating every governance input balance must never touch
closes that gap explicitly rather than leaving it implicit in "money never
buys voice."

**Constitutional impact:** directly implements Directive 16 ("not
directly, not indirectly, not accidentally") as an enumerated technical
checklist rather than a general principle; implements Directive 17 (a
later-born human's Human Share formula has no birth-date-dependent
penalty); reinforces Directive 13 by using the annual-ceiling/vesting
design specifically to prevent a founding generation from retaining a
fixed share of the economy over century timescales. No `docs/
INVARIANTS.md` row changes — this calibrates P1 rather than creating a
new invariant; `docs/THREAT_MODEL.md`'s whale-concentration and
treasury-capture rows are updated to cite this entry (see below).

**Implementation status:** design only. No `crates/mini-reward` or chain
state-machine code yet enforces the 3%/2%/0.75%/0.25% split, the 365-day
vesting window, or the enumerated anti-whale-wall checklist as a compile-
time or runtime guarantee — see `docs/STATUS.md`.

**Failure point:** the specific ceiling percentages and vesting window are
founder-set starting parameters pending the 200-year simulation suite in
`docs/design/inflation-and-whale-resistance.md`; the anti-whale wall as
enumerated here constrains *protocol-native* governance inputs only — it
cannot prevent off-protocol vote-buying/coercion, which is why §9 of the
design doc separately calls for receipt-free ballots as a further,
not-yet-built mitigation.

**Required follow-up:** build the simulation harness and run the
population/whale-position/behavior/shock matrix in `docs/design/
inflation-and-whale-resistance.md`; engage the same mechanism-design
specialist as D-0073 per `docs/gates/economic-simulation-spec.md`; wire
the enumerated anti-whale-wall checklist into `mini-chain`/`mini-forge`
as actual code-level guarantees, not just a design list; design and
implement receipt-free governance ballots. Roadmap #50 stays open,
tracking exactly this follow-up.

**Supersedes / superseded by:** none — first decision resolving #50's
open question; builds on D-0074's own dependency on D-0073 for the
treasury-contribution channel's parameters.

---

### D-0075 — Private Human Continuity Proof redefines personhood signal (b)  ·  *Accepted*
**Date:** 2026-07-10 · **Refs:** roadmap #21, D-0038, D-0054, `docs/gates/
personhood-signal-b-decision.md`, `docs/design/human-continuity-proof.md`,
Directives 8, 9, 11, 13, 15, 17.

**Decision:** the whitepaper's behavioral/location-entropy signal is
redefined, not permanently dropped nor implemented as originally
specified. It becomes a **Private Human Continuity Proof**: an optional
signal accumulating expiring evidence from independent classes (seed-
connected human vouching, repeated physical presence, home/device
continuity, government/external credentials, authenticated web-life
continuity, household relations, ephemeral live interaction), each
capped, none individually sufficient. No raw behavioral, browsing,
location, credential, biometric, family, or graph data ever enters the
network — only a zero-knowledge aggregate proof plus anti-reuse
nullifiers (an epoch-claim nullifier and a per-anchor binding nullifier).
Evidence maturity governs *vesting speed* of the Human Share (D-0074), on
a 10%/25%/50%/75%/100% schedule over 365 days — never the total
entitlement or any governance weight. Full construction, signal weights,
and the research program are recorded in `docs/design/
human-continuity-proof.md`.

**Reason:** a single behavioral/location classifier cannot prove global
human uniqueness and would create unacceptable surveillance/exclusion
risk, matching `personhood-signal-b-decision.md`'s own conclusion that no
known construction satisfies private + weak-device-friendly + platform-
vendor-independent simultaneously. A diverse, time-separated evidence
collection cannot make Sybils impossible either, but can convert mass
identity creation from a cheap digital action into the expensive
maintenance of many credible human-life footprints — the same economic
argument the whitepaper already makes for the system as a whole (§11),
extended to this one optional signal rather than left unsolved and idle.

**Constitutional impact:** strengthens Directive 8 (human-rooted vouching
stays the required legitimacy anchor — D-0054's live-vouching requirement
is unchanged, this signal only adds to the score); strengthens Directive 9
(only aggregate predicates leave the device, never raw life data);
strengthens Directive 11 (mandatory no-government/no-biometric/no-modern-
hardware/offline-heavy paths, §11 of the design doc); strengthens
Directive 13 (expiring, replaceable, versioned evidence methods instead
of one permanent technology dependency); strengthens Directive 17 (a
future child is not disadvantaged by government, wealth, hardware,
family structure, mobility, or which companies they can reach). No
`docs/INVARIANTS.md` row changes — extends D-0038's existing open-ended
accumulator architecture rather than creating a new invariant.

**Implementation status:** design only. `mini-uniqueness::status`
(D-0038) and its `SignalSource::External` extension point already exist
and can host this signal without a breaking change, but no
`EvidenceStamp` type, pairwise-pseudonym derivation, nullifier registry,
or aggregate ZK proof exists yet. See `docs/STATUS.md`.

**Failure point:** stated plainly and not softened — paid or coerced
genuine humans, corrupt credential issuers, and a sufficiently patient
nation-state can still eventually satisfy this policy's thresholds; the
system raises cost and improves detection, it does not mathematically
prove one biological human has exactly one identity. The specific
per-signal weights and thresholds in §3 of the design doc are founder-set
starting parameters pending adversarial simulation (Phase 5 of the design
doc), not validated values.

**Required follow-up:** implement the evidence/nullifier framework
(Phase 1 of the design doc); prototype web and home-device continuity
(Phase 2); build the aggregate ZK statement (Phase 3); fund Research
Tracks A-F (private TLS predicates, sensor provenance, blind uniqueness
credentials, private co-presence diversity, coercion modeling,
weak-device benchmarking); calibrate all thresholds via adversarial
simulation before any of it becomes load-bearing; preserve
`VouchingGraph` as a required live `FullHuman` source (D-0054) unless a
later recorded decision provides an equally human-rooted replacement.
Roadmap #21 stays open, retitled to "Private human-continuity proof
research — optional behavioral, web, device, and credential evidence,"
as a research-and-integration issue, not a launch blocker.

**Supersedes / superseded by:** supersedes the interpretation of signal
(b) as a single behavioral/location silver bullet, and updates `docs/
gates/personhood-signal-b-decision.md`'s Option A/B/C framing (this
decision is a redefinition, not a selection among the three). D-0038
remains controlling for the open-ended multi-signal accumulator
architecture this signal is hosted inside; D-0054 remains controlling for
the live-vouching requirement.

---

### D-0076 — `mini-installer` gains a persisted, hash-chained event log, separate from and subordinate to its type-state pipeline  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #102 (self-hosted forge spine
Batch 4), PR #109 (self-hosted spine E2E harness, whose own module docs
first named this gap), `docs/design/self-hosted-forge-spine.md` Batch 4
section, `crates/mini-installer/src/event_log.rs`.

**Decision:** `mini-installer`'s existing type-state pipeline (`Discovered
→ ... → Active` or `RolledBack`, D-0071) stays the sole in-process
authority for what the installer is allowed to do — this decision adds
nothing to that authority. Alongside it, every real transition
(`Installer::stage`/`preflight`/`activate`/`health_check`/`rollback`) now
also appends a record to a durable, append-only, hash-chained event log
on disk, queryable after any process exit via a new `Installer::event_log`
reader and a standalone `verify_install_event_log` function that
independently checks hash-chain integrity, sequence contiguity, and
per-release state-machine validity (rejecting invalid transitions,
unexplained rollbacks, and stale rollback targets). `StagedRelease`/
`PreflightPassed`/`ActivationRecord` gained a `version: String` field so
later pipeline stages can recover a release's version without a
side-channel lookup; `Installer::stage`/`preflight`/`health_check`/
`rollback` gained a `timestamp_ms: u64` parameter (matching this
workspace's existing no-internal-`SystemTime::now()` convention) since
`activate` already had one via `OwnerApproval::approved_at_ms`.

**Reason:** a type-state pipeline is a real, compiler-enforced
correctness mechanism *while a process is running*, but it leaves nothing
behind for a fresh process, an auditor, or a future `mini installer
history`/`verify-log` CLI command to inspect once that process exits —
exactly the gap PR #109's own E2E harness named as unproven when it
first drove the full spine. A self-updating system needs durable evidence
of what it did to itself, not just correctness of what it's doing right
now.

**Constitutional impact:** implements the same "typed domain, never
generic sign/finalize" discipline (CLAUDE.md hard rule) at the evidence
layer: `InstallEvent` is a specific, named record type, not an open
`serde_json::Value` blob a caller could shape into anything. No
`docs/INVARIANTS.md` row changes — this adds evidence, it does not touch
the installer's authority model (U1, no-forced-update/no-kill-path)
which the boundary rule below exists specifically to protect.

**Implementation status:** shipped. `crates/mini-installer/src/
event_log.rs` (encode/decode, hash chaining, the verifier), wired into
every existing `Installer` method; 7 new adversarial tests
(`tests/event_log.rs`) plus the 10 pre-existing `tests/installer.rs`
tests updated for the new method signatures; `crates/mini-cli/tests/
self_hosted_spine_e2e.rs` reopens the log from disk post-hoc and asserts
the exact event-kind sequence for both the good and the deliberately
broken release.

**Failure point:** the log's append path re-reads the entire log on every
write to derive the next sequence number and hash-chain link (no
in-memory counter, matching this crate's existing "no in-memory state to
get out of sync with a process restart" design) — fine for a device's
real lifetime event count, but a caller than logs pathologically often
would see this degrade linearly. No encryption or access control on the
log file itself; anyone who can read the installer's root directory can
read the full install history (matches this crate's existing "real local
files, not a service" trust model — no new exposure beyond what already
existed for `current`/`previous`).

**Required follow-up:** wire `Installer::event_log`/
`verify_install_event_log` into a real `mini installer history`/`mini
installer verify-log` CLI command once CLI wiring for build/release/
install exists (the next PR in this stack); consider whether a device
with a very long install history needs a compaction/rotation policy
before that becomes a real CLI surface most users touch.

**Supersedes / superseded by:** none — first decision on this specific
gap. Builds directly on D-0071 (mini-installer itself) without altering
its authority model.

### D-0077 — `mini-cli` gains real `build`/`release`/`provenance`/`installer` subcommands  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #102 (self-hosted forge spine
Batch 4/5 boundary), PR #109 (E2E harness whose own module docs first
named "no CLI subcommand yet" as the gap), D-0076 (installer event log),
`crates/mini-cli/src/{build,release,provenance,installer}.rs`,
`crates/mini-cli/tests/cli_spine_commands.rs`.

**Decision:** `mini` gains four new top-level commands, each a thin
wrapper over an already-real library, none re-implementing or weakening
anything the library already enforces: `mini build run` spawns the real
`mini-build-runner-wasmtime` binary as a genuine subprocess (never linked
in-process — `mini-cli`'s own dependency on it stays `[dev-dependencies]`
only, preserving D-0069's "only that one crate links Wasmtime" boundary)
and speaks real `mini-pipeline-protocol` framing over its stdin/stdout;
`mini release create/attest/verify/list` wraps `mini_forge::release`/
`attest`/`verify_governed_release`/`list_releases`; `mini provenance
record/verify` wraps `mini_provenance::record_provenance`/
`independent_agreement`; `mini installer stage/preflight/activate/
health-check/rollback/status/history/verify-log` drives the real
`mini_installer::Installer` pipeline one step per CLI invocation.

The installer commands solve a problem the type-state pipeline was never
designed for: each `mini installer <step>` is a fresh process, so it
cannot hold a `StagedRelease`/`PreflightPassed`/`ActivationRecord` value
across invocations the way an in-process caller (the E2E harness) can.
Three new `Installer` methods — `staged_release`, `preflight_passed`,
`activation_record` — reconstruct the minimal typed value from the
installer's own disk state and persisted (D-0076) event log, each
refusing (`InstallerError::{NoSuchRelease,WrongState,NotCurrentlyActive}`)
unless the log's own record shows the release genuinely completed the
expected prior step. `staged_release`'s reconstructed digest comes from
the log's `Staged` event, not from re-hashing the file on disk right now
— preserving the exact tamper check `preflight` exists to perform, rather
than having it trivially agree with itself. `mini installer activate`
constructs the `OwnerApproval` itself, right there, naming exactly the
release id on the command line — invoking the command *is* the explicit
device-owner action; nothing reads one from the log or anywhere else
(unchanged from D-0071/D-0076's boundary rule: the log is evidence, never
permission).

**Reason:** PR #109's own module docs named this exactly: "There is no
CLI subcommand yet for build/provenance/release/install… CLI/`--json`
wiring for release/install is real, separate follow-up work, not yet
done." A constitutional protocol meant to outlive its creators cannot
stay a set of library calls only its own test suite exercises — a real
developer needs to type `mini release create` and have it work.

**Constitutional impact:** upholds CLAUDE.md's typed-domain rule (`mini
installer activate` builds a real, request-shaped `OwnerApproval`, never
a generic "approve" flag) and the D-0069 Wasmtime-isolation boundary
(`mini build run` spawns, never links, the runner). No
`docs/INVARIANTS.md` row changes — this is a new access path onto
already-governed authority, not new authority.

**Implementation status:** shipped. Four new `mini-cli` modules plus
`cli.rs` dispatch wiring; three new `Installer` reconstruction methods
(`crates/mini-installer/src/lib.rs`) plus three new `InstallerError`
variants; `crates/mini-installer/tests/cross_process_reconstruction.rs`
(8 new tests covering the reconstruction methods directly, including the
tamper-preservation case); `crates/mini-cli/tests/
cli_spine_commands.rs` (2 new tests: one drives `mini build run` against
a real compiled guest component through the real runner subprocess, one
drives release/attest/verify/list, provenance record/verify, and the
full installer stage→preflight→activate→health-check→verify-log→history
chain through the real text-based CLI, plus a standalone `mini installer
rollback` failing cleanly with nothing to roll back to). Full workspace
`cargo test --workspace --all-features` green (no regressions).
`self_hosted_spine_e2e.rs` deliberately left calling `mini_forge`/
`mini_media`/`mini_provenance`/`mini_installer` directly rather than
rewired onto the new CLI surface — see its own updated module docs for
why (no `--json` output exists yet, so re-threading its rich internal
assertions through today's human-readable text would be fragile
scraping, not a stronger proof).

**Failure point:** every value threaded from one CLI command's output
into the next command's input is scraped out of human-readable text
(`last_word`, matching `two_developers.rs`'s existing precedent) — brittle
against future output-format changes, and exactly the gap the next PR
(stable `--json` output) exists to close. `mini build run`'s CLI-only
directory-content-hashing helper duplicates (rather than depends on)
`mini-build-runner-wasmtime::content_store::hash_directory_tree`'s
byte-for-byte encoding, per D-0069's subprocess-only boundary — verified
to match (little-endian length prefix) during this batch, but the two
implementations are not statically guaranteed to stay in sync if either
changes independently.

**Required follow-up:** stable `--json` output for all of these commands
(next PR in the stack), replacing text-scraping with a real machine-
readable contract; adversarial release/install CLI fixtures (the PR
after that).

**Supersedes / superseded by:** none. Builds on D-0069 (Wasmtime
isolation), D-0071 (mini-installer), D-0076 (event log) without altering
any of their authority models.

### D-0078 — `mini-cli` gains stable `--json` output for build/release/provenance/installer, hand-rolled  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** D-0077's own "Required follow-up",
`crates/mini-cli/src/json.rs`, `crates/mini-cli/tests/cli_json_output.rs`.

**Decision:** a global `--json` flag makes `mini build run`/`release
create|attest|verify|list`/`provenance record|verify`/`installer
stage|preflight|activate|health-check|rollback|status|history|
verify-log` emit a single-line, machine-parseable JSON document instead
of human text: `{"ok":true,"kind":"<verb.noun>",...fields}` on success,
`{"ok":false,"kind":"<verb.noun>","error_code":"...","message":"..."}`
on failure. Every command that creates or inspects a specific object
(`release create`'s `release_id`, `installer stage`'s `digest`,
`provenance verify`'s `agreement` count) exposes that value as a real
typed JSON field — a caller chains commands by reading a field, never
by re-parsing a human sentence with `last_word()`-style text scraping.

No `serde`/`serde_json` dependency: `crate::json` is a hand-rolled
emitter (`JsonValue` enum, single `render()` method, no parser — this
crate only ever produces JSON, never consumes its own output), matching
this workspace's established convention (`mini-forge`'s git-object
framing, `mini-installer`'s event log encoding) of a few dozen lines of
plain Rust over a dependency where one would do the job.

`--json` is a clean usage error (`CliError::Usage`) on `identity`/`kel`/
`repo`/`pr`/`sync` — none of those commands' internals were touched;
scope stayed to exactly the commands D-0077's own follow-up named. A
scripting caller passing `--json` must never silently get human text
back with no indication — an explicit rejection is safer than a partial,
undocumented contract.

**Reason:** D-0077 shipped the CLI commands themselves but explicitly
named this as unfinished: its own PR body says every value threaded
between commands in `cli_spine_commands.rs` "is scraped out of today's
human-readable text" and calls that "the explicit next PR in the stack."
A stable machine-readable contract is what actually lets an external
tool (a CI pipeline, a future `mini-devd` daemon, a second implementation
in another language) drive this spine without depending on prose wording
that is free to change.

**Constitutional impact:** none — presentation-layer output formatting
only. No `docs/INVARIANTS.md` row changes; no new authority, no new
access path to anything the commands didn't already do. The
`CliError::error_code()` mapping is new public surface (a stable
snake_case identifier per error variant) but carries no capability.

**Implementation status:** shipped. `crate::json::{JsonValue,
CommandResult, ok_envelope, err_envelope}`; `CliError::error_code()`;
`mini_cli::json_error_envelope`/`command_kind` (used by `main.rs`, since
`mini_cli::run`'s `Result<String>` contract keeps `Err` meaning "the
command failed" for every existing Rust caller — turning a failure into
`Ok(json_string)` there would silently break that for any in-process
embedder or test, so the error-envelope rendering lives at the actual
process/stdout boundary instead). `build`/`release`/`provenance`/
`installer` module functions now return `CommandResult` (human text +
typed fields) instead of a bare `String`; `cli.rs`'s dispatch renders
either representation based on the flag. 4 unit tests (`json.rs`) plus
3 new integration tests (`cli_json_output.rs`): a real field
(`release_id`) extracted from one command's JSON output and fed directly
into a second command with no text parsing; `--json` cleanly rejected
for an unsupported command; the actual compiled `mini` binary (not
`mini_cli::run` in-process) spawned as a real subprocess to prove the
error envelope path in `main.rs` itself. Full workspace `cargo test
--workspace --all-features` green, no regressions — every pre-existing
text-based test still passes unchanged, since `--json` defaults to off
and `CommandResult::render(false, _)` returns exactly the prior human
string.

**Failure point:** `command_kind()` (best-effort "what command was this"
label for a failed `--json` invocation, computed in `main.rs` from raw
args by skipping known global value-flags) is a heuristic, not a parse
of the same grammar `cli.rs`'s dispatch actually uses — a future global
flag added to `run()` without updating `command_kind()`'s skip-list would
make the `kind` field on some error envelopes wrong (never absent,
always a best-effort guess). `identity`/`kel`/`repo`/`pr`/`sync` still
have no `--json` support at all; a caller wanting structured output for
those must still scrape text.

**Required follow-up:** extend `--json` to `identity`/`kel`/`repo`/`pr`/
`sync` once there is a real caller that needs it (matching this batch's
own scoping rule — build only what the next real integration needs);
adversarial release/install CLI fixtures (the PR after that, per
D-0077's own sequencing).

**Supersedes / superseded by:** none. Fulfills the follow-up D-0077
named; does not alter D-0077's command surface or D-0076's event log.

### D-0079 — Adversarial `release`/`installer` CLI fixtures  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** D-0077 ("Required follow-up: ...
adversarial release/install CLI fixtures, the PR after that"),
`crates/mini-cli/tests/adversarial_release_install.rs`.

**Decision:** ten new tests drive `mini release`/`mini installer`
through the real text-based CLI (`mini_cli::run`, not direct
`mini_forge`/`mini_installer` calls) against specifically adversarial
inputs, proving the CLI layer added in D-0077 introduces no bypass of
any safety property `mini-forge`'s/`mini-installer`'s own library-level
adversarial suites already established: a lone real attester is refused
(`need 2 independent attestations, got 1`); a release author's
self-attestation of their own release never counts toward quorum; two
attestation calls from the *same* identity still count once, not twice;
an attestation naming a digest that doesn't match the release's real
artifact digest is silently excluded rather than counted; `release
verify` refuses before the adoption timelock elapses and refuses a
branch the release didn't actually claim; and, on the installer side,
calling `activate` before `preflight`, or `preflight` on a release that
was never `stage`d, both fail cleanly with the expected
`InstallerError` variant rather than silently proceeding or panicking.
A tenth "sanity anchor" test confirms the identical setup *does* verify
successfully once every condition above is genuinely met — so the
failure tests above are proven to fail for the right reason, not
merely because something else broke.

**Reason:** D-0077's CLI wrappers reconstruct governance/attestation
state (via `release::verified_release`) and installer pipeline state
(via the new cross-process reconstruction methods) from scratch on
every invocation; a wiring bug in that reconstruction path could
plausibly weaken a check that the underlying library enforces correctly
in-process, without any existing test catching it, since neither
`mini-forge`'s nor `mini-installer`'s own suites go through the CLI at
all, and `cli_spine_commands.rs`/`cli_json_output.rs` only exercise
happy paths.

**Constitutional impact:** none — no behavior changed, only proven.
Confirms (does not alter) that self-attestation exclusion and
attestation deduplication, both load-bearing for the "N distinct
verified identity roots, author excluded" adoption quorum pattern
(mirrored from `mini_forge::release` into `mini_provenance` per D-0068),
survive the CLI's re-derivation of that state on every invocation.

**Implementation status:** shipped, all 10 tests passing on first run
against the real CLI (no implementation changes were needed — the
adversarial fixtures confirmed D-0077's reconstruction logic was
already correct, not that it needed fixing). Full workspace `cargo test
--workspace --all-features` green.

**Failure point:** these fixtures cover `release`/`installer` only, the
two command groups D-0077's own follow-up named — `repo`/`pr`'s CLI
layer (governed merge quorum, author-exclusion in PR approval counting)
has its own adversarial coverage from earlier batches
(`two_developers.rs`'s untrusted-author test, `mini-forge::governance`'s
own suite) but nothing analogous to this file's "attack the CLI
reconstruction path specifically" framing.

**Supersedes / superseded by:** none. Fulfills the follow-up D-0077 and
D-0078 both named.

### D-0080 — Prove the full spine (governed merge -> release -> install) reaches a peer over `mini sync` alone  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #102 (self-hosted forge spine
Batch 5), D-0062 (the original `mini-bootstrap`/`mini-sync` live-TCP
demo whose composition insight this reuses), `tests/network_sync.rs`
(Batch 5's first piece, governed-merge-over-the-wire only),
`crates/mini-cli/tests/network_sync_release.rs`.

**Decision:** one new test proves `mini_sync::sync_bidirectional`
already replicates the *entire* self-hosted forge spine over a real TCP
connection with zero shared filesystem, not just the governed-merge
slice `network_sync.rs` already covered: three identities (Alice,
Carol, Dave) do every authoring/review/attestation step in one local
store — commit, PR, review, merge, `release create`, two independent
`release attest` calls — and a fourth, Bob, whose store has *never*
touched that filesystem, connects once over loopback TCP
(`mini sync connect`/`listen`) and then, using only what arrived over
that connection, independently runs `release verify` (real governance +
attestation-quorum + timelock resolution) and the full
`installer stage → preflight → activate → health-check` sequence to a
genuinely active, passing install. No new replication code was written
— `mini_sync::sync_bidirectional` iterates `store.all_ids()`, type-
agnostic over every signed object, exactly as it always has; this test
exists to demonstrate that fact for release/attestation/install objects
specifically, the same way `network_sync.rs` already demonstrated it for
commits/PRs/reviews.

**Reason:** the roadmap named this milestone explicitly ("sync:
replicate proposal/review/merge/release objects over Mininet-native
sync... no new wire protocol needed per the same composition insight
D-0062 already proved") but no test had actually driven a release
through `mini sync` before, and — more importantly — nothing had ever
proven a receiving peer could *install* from purely-synced data. A
`mini_sync` bug narrow enough to only affect release/attestation/
installer object types (a filter, an allowlist, a type check somewhere)
would have shipped invisibly without this.

**Constitutional impact:** none — confirms existing behavior, adds no
new code path. Reaffirms that `mini_sync`'s trust boundary (out-of-band
`kel trust`, unchanged since D-0062) is what actually gates which
objects a peer accepts, not object type — the same "identity, not
schema, is the security boundary" property release/installer verification
already relies on.

**Implementation status:** shipped, test passing on first run — no
`mini-sync`/`mini-cli` implementation changes were needed. Full
workspace `cargo test --workspace --all-features` green (110 test
results, 0 failures).

**Failure point:** this is one scenario, not an adversarial suite — it
does not prove a *malicious* peer can't abuse `sync` to smuggle a bad
release into a store (that is `mini-forge`'s/`mini-installer`'s own
verification job at read time, already covered by D-0079, and untouched
by this test). `mini sync`'s own honest limits (one bounded connection
per invocation, no daemon, no witness/discovery layer) are unchanged and
still apply.

**Required follow-up:** the no-GitHub outage demo (next in the roadmap
sequence) is the natural place to combine this with adversarial-fixture-
style pressure — a release proposed, reviewed, merged, released, and
installed with GitHub genuinely unreachable throughout.

**Supersedes / superseded by:** none. Extends D-0062's composition
insight and Batch 5's first piece (`network_sync.rs`) without altering
either.

---

### D-0200 — Networked BFT consensus round (`mini-consensus`), round-0 slice  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0008, D-0055,
D-0061, `docs/design/networked-consensus.md`, Directive 4, Directive 14,
INVARIANTS P1/P2.

**Decision:** add a new crate, `mini-consensus`, that carries
`mini-chain`'s finality math and `mini-execution`'s state machine off a
single machine for the first time. It contributes exactly the pieces
`mini-chain`'s and `mini-execution`'s own docs named as their explicit
non-goals: a canonical bounded **wire codec** for the two messages a
round exchanges (a block proposal, and a signed `mini_chain::Vote`); a
pure, deterministic, order-insensitive **round driver** (`Round`) that
selects a proposer deterministically (`proposer_for`) and, from proposals
and votes arriving in any order, drives prevote → precommit → quorum
certificate; a **`ConsensusNode`** that ties one round per height to a
`mini_execution::LedgerChain`, advancing only behind a certificate its own
execution layer re-verifies; and a real **`TcpMesh`** transport over
`mini_bearer::TcpBearer` with a `run_to_height` loop. A companion
integration test runs four validator nodes in four OS threads, each with
an independent ledger sharing no memory or filesystem, to bit-identical
finalized state over a real socket mesh. This decision covers the
**round-0 happy path only** — a single round per height, assuming the
height's proposer is online and honest.

**Reason:** the audit that produced D-0066 found implementation breadth had
outrun vertical integration; the settlement/execution vertical
(D-0055/D-0061) explicitly ended at "given a `(header, body, qc)` triple
from *somewhere* (a real network, eventually)." Nothing produced that
triple across a process boundary. This is the smallest honest slice that
does, reusing only already-decided constructions (D-0008's Tendermint-
style `>2/3`-distinct-roots finality, D-0015's TCP bearer) so it composes
prior art rather than inventing anything (Directive 14: prefer the
smaller, well-trodden construction).

**Constitutional impact:** upholds Directive 4 (the integration test proves
independent honest nodes converge on bit-identical state — the property
Directive 4 demands of the real network this stands in for); preserves
P1/P2 (finality is still exactly `mini_chain::verify_finality`'s equal-
weight-per-identity-root, `>2/3`-distinct rule — this crate counts one
root at most once at every layer and adds no weight field, no stake, no
new authority); honors the typed-domain rule (votes are signed only via
`mini_chain::sign_vote`'s typed request, never a generic `sign(bytes)`;
the private `Vote::signature` field's invariant is kept local to
`mini-chain` even though votes now cross a wire). No voice/value edge:
`mini-consensus` depends on no value crate. No `docs/INVARIANTS.md` row
changes — this is new networking/protocol code atop frozen finality math,
not a change to what "final" means.

**Implementation status:** shipped and tested — 17 unit tests
(wire round-trip/truncation/bounds, round happy-path/reordering/dedup,
node proposal-validation/buffering) plus a 4-node real-TCP convergence
integration test; `mini-chain` gains `Vote::to_wire_bytes`/`from_wire_bytes`
and a `ChainError::Malformed` variant; `mini_execution::SettlementBlockBody`
gains `PartialEq`/`Eq`. `cargo fmt`/`clippy -D warnings`/`test` all clean
for the touched crates (the pre-existing `mini-build-runner-wasmtime`
adversarial suite still needs a `wasm32-wasip2` toolchain this environment
lacks — unrelated).

**Failure point:** stated plainly and not softened — **round-0 only, no
liveness under proposer failure.** There is no round timeout, no `nil`
prevote, no view-change to a fresh proposer, so a single silent or
equivocating proposer stalls its height (it never *finalizes the wrong
thing* — safety holds — it just makes no progress). No equivocation
evidence/slashing, no dynamic validator set, and `TcpMesh` is a cleartext,
discovery-free, no-reconnect pipe (no `mini_bearer::Channel` authenticated
encryption, no `mini-net` overlay yet). "Multi-process/multi-machine" is
demonstrated as threads over loopback, not machines over the internet.

**Required follow-up:** implement Tendermint-style view-change (round
timeouts, `nil` prevotes, proposer rotation across rounds) so a crashed
proposer no longer stalls a height — the single largest gap before this is
a live protocol; wrap `TcpMesh` in `mini_bearer::Channel` for
authenticated, encrypted links and route peer discovery through
`mini-net`; add equivocation evidence; wire dynamic validator-set changes.
Keep roadmap #36–#45 open (this discharges the "networked consensus that
produces the triple" piece, not view-change or transport hardening); mark
🟡 in #92, not closed.

**Supersedes / superseded by:** supersedes nothing. Extends D-0008
(finality math), D-0055 (offline settlement claims), and D-0061
(chain-backed `CanonicalLedgerView`) by adding the networking layer they
each deferred; does not alter any of them.

---

### D-0201 — `mini-consensus` gains Tendermint view-change (multi-round, locking, timeouts): a crashed proposer no longer stalls a height  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0200, D-0008,
`docs/design/networked-consensus.md`, arXiv:1807.04938, Directive 4,
Directive 14, INVARIANTS P1/P2.

**Decision:** replace `mini-consensus`'s round-0-only driver with a
faithful implementation of the full, published Tendermint consensus
algorithm — Buchman, Kwon & Milosevic, *"The latest gossip on BFT
consensus"* (arXiv:1807.04938), Algorithm 1, the construction CometBFT
runs in production. The `Round` state machine now runs **multiple rounds**
per height with the complete `upon`-rule set (each citing the paper's line
numbers), `lockedValue`/`lockedRound` + `validValue`/`validRound` locking,
`nil` prevotes/precommits (the all-zero block-hash sentinel, which
`verify_finality` never counts toward a real certificate), POLC
re-proposal, and the `f+1`-higher-round skip. Timeouts stay **clock-free
in the state machine**: it emits `ScheduleTimeout` intents that the host
(`ConsensusNode`/`net::run_to_height`) turns into real timers whose
durations widen with the round, and feeds back as `on_timeout`. Result:
when a height's proposer is silent or crashed, honest nodes time out,
prevote/precommit `nil`, and roll to the next round with a fresh proposer,
instead of stalling.

**Reason:** the D-0200 slice's single largest named gap and stated next
step was exactly this — "no liveness under proposer failure." A partial or
homegrown view-change would be worse than none, because cross-round
*safety* (never two conflicting decisions at one height) depends on the
locking rules being exactly right; so the decision is to adopt the
peer-reviewed algorithm wholesale rather than invent one (Directive 14,
and the project's standing "compose vetted prior art, don't design novel
mechanisms" rule applied to consensus, not just cryptography).

**Constitutional impact:** upholds Directive 4 — a new networked test
(three online validators of a four-validator set, the fourth permanently
offline) proves the cluster survives a crashed proposer *via view-change
over a real socket mesh* and still converges to bit-identical state;
preserves P1/P2 — finality is still exactly `verify_finality`'s equal-
weight, `>2/3`-distinct-roots rule, unchanged; `nil` and multi-round add
no weight, no stake, no new authority, and one root is still counted at
most once at every layer. Votes remain signed only through the typed
`mini_chain::sign_vote` request (now also over the `nil` sentinel), never a
generic `sign(bytes)`. No voice/value edge. No `docs/INVARIANTS.md` row
changes — new liveness machinery around frozen finality math.

**Implementation status:** shipped and tested. `round.rs` is a full
rewrite to Algorithm 1 (7 pure state-machine tests: happy-path decide,
silent-proposer→round-advance, locking-forbids-a-conflicting-prevote,
locked-value-re-prevote-with-POLC, `f+1` skip, order-insensitive decide);
`node.rs`/`net.rs` gained `nil` signing, per-round proposing/re-proposing
from a `BodySource`, and a real widening-timeout clock; the proposal wire
message gained `round` and `valid_round` fields. Both networked tests
(happy-path convergence and crashed-proposer view-change) pass repeatedly
over loopback TCP. `cargo fmt`/`clippy -D warnings`/`test` clean for the
crate.

**Failure point:** stated plainly. Safety holds in full; the residual gaps
are liveness/DoS and deployment, not correctness: **proposals are
unsigned** at this layer, so a Byzantine node can waste a round by
front-running the proposer (never finalize a wrong block — an unwanted
value simply fails to gather `2f+1` honest prevotes — but it can slow a
height); vote **gossip is single-hop broadcast**, not full re-gossip of
past rounds, so the POLC-re-proposal path depends on those prevotes still
being reachable (the crash-recovery path does not, and is what the test
exercises); a **truly dead peer eventually back-pressures** the blocking
TCP `send` (the test sidesteps this by not meshing to the offline node —
the online set is exactly quorum); still **no equivocation evidence, no
dynamic validator set, no authenticated/encrypted links** (`TcpMesh` is
cleartext), and the demonstration is threads over loopback, not machines
over the internet.

**Required follow-up:** sign proposals (close the front-running/DoS
surface); add gossip re-delivery of past-round votes so POLC re-proposal is
robust on a lossy network; make `TcpMesh` broadcast non-blocking (or move
to `mini_bearer::Channel`) so a dead peer cannot back-pressure honest
nodes; wrap links in authenticated encryption and route discovery through
`mini-net`; add equivocation evidence and dynamic validator-set changes.
Keep roadmap #36–#45 open; mark 🟡 in #92.

**Supersedes / superseded by:** supersedes D-0200's round-0-only round
driver (its wire codec, node/execution integration, and TCP mesh are
carried forward and extended, not replaced). Does not alter D-0008,
D-0055, or D-0061.

---

### D-0202 — `mini-consensus` proposals are now signed by the round proposer (closes the front-running gap)  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0201, D-0200,
`docs/design/networked-consensus.md`, Directive 14, the typed-domain rule
(CLAUDE.md), INVARIANTS P1/P2.

**Decision:** consensus proposals are now authenticated. A `Proposal`
carries the current round's `proposer_root`, the delegated
`proposer_device` that signed it, and a signature over a typed transcript
binding `(domain, height, round, valid_round, block_hash, proposer_root)`
— built by `wire::sign_proposal` and checked by `wire::verify_proposal`
(the signature field is private, so a `Proposal` cannot be constructed
except by signing). A node accepts a proposal for `(height, round)` only
if its `proposer_root` is exactly `proposer_for(height, round)`'s
selection *and* the signature verifies as a `VOTE`-capable delegated
device of that root; anything else is dropped before the value is even
considered. `proposer_root` is deliberately the *current round's*
proposer, distinct from the value's own `header.proposer`, so a
re-proposed `validValue` (built by an earlier round's proposer) is
re-signed by whoever legitimately re-proposes it.

**Reason:** the D-0201 view-change slice's top stated follow-up and largest
residual gap — proposals were unsigned, so a Byzantine node could
front-run the designated proposer with its own valid value and waste a
round (a liveness/DoS attack; never a safety hole, but a real one). Signed
proposals close it with no new cryptography — composing `did_mini`'s
existing delegation/signing exactly as `mini_chain::sign_vote` already
does (Directive 14).

**Constitutional impact:** honors the typed-domain rule — proposing is now
a specific, named, signed request (`sign_proposal` over a domain-tagged
transcript), never a generic `sign(bytes)`, the same discipline
`sign_vote`/`sign_release_attestation` follow; preserves P1/P2 and
finality (unchanged — `verify_finality` still decides what is final; this
only gates which proposals a round will *consider*). Reuses the existing
`Capabilities::VOTE` for proposing rather than inventing a capability
(equal-validator duty). No voice/value edge; no `docs/INVARIANTS.md` row
changes.

**Implementation status:** shipped and tested. `wire::Proposal` gains
`proposer_root`/`proposer_device`/`signature` and typed
`sign_proposal`/`verify_proposal`; the node authenticates every incoming
proposal and signs its own (and re-signs on re-proposal). Three new node
tests (designated-proposer proposal is prevoted; a valid proposal from the
*wrong* proposer is dropped; a proposal whose signer is not a delegated
device of the claimed root is dropped) plus a wire verify/tamper test; the
happy-path and crashed-proposer view-change networked tests still pass.
`cargo fmt`/`clippy -D warnings`/`test` clean. The crate-level docs, stale
since D-0201, were corrected in the same change to describe the
multi-round, view-change, signed-proposal reality.

**Failure point:** stated plainly. Front-running is closed, but the other
D-0201 residuals stand and are not correctness bugs: vote broadcast is
still single-hop (no past-round re-gossip, so POLC re-proposal is only as
robust as the links are lossless); a truly dead peer can still
back-pressure the blocking TCP `send`; there is still no equivocation
evidence (a proposer that signs two different values for one round is
detectable now — both are signed — but this crate does not yet collect or
act on that proof); the validator set is static; and `TcpMesh` remains
cleartext with no discovery. Still threads over loopback, not machines
over the internet.

**Required follow-up:** collect proposer/vote equivocation as slashable
evidence (now that both are signed, double-signing is provable); robust
vote gossip (re-deliver past-round votes; non-blocking broadcast); wrap
links in `mini_bearer::Channel` and route discovery through `mini-net`;
dynamic validator sets. Keep roadmap #36–#45 open; mark 🟡 in #92.

**Supersedes / superseded by:** extends D-0201 (adds proposal
authentication to its round engine, wire, and node) without altering its
locking, view-change, or finality semantics. Supersedes nothing.

---

### D-0203 — `mini-consensus` transport is non-blocking and buffered: a dead peer can no longer back-pressure honest nodes  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0200, D-0202,
`docs/design/networked-consensus.md`, Directive 4.

**Decision:** `net::TcpMesh` no longer sends over blocking
`mini_bearer::TcpBearer` links. Each link is now a non-blocking socket
with a bounded per-link outbound buffer (`MAX_LINK_OUTBOUND_BYTES`),
framed with `mini_bearer`'s public `encode_frame`/`FrameReader`. A
broadcast queues bytes and flushes as far as the socket accepts right now;
partial writes and `WouldBlock` leave the remainder buffered, and a peer
that has stopped reading fills its buffer and then has further frames
dropped best-effort — it can never block or back-pressure the sender.
Receiving is likewise non-blocking. `broadcast`/`poll` keep their
signatures, so the node and round layers are untouched.

**Reason:** the D-0202 residual list named this precisely — "a truly dead
peer eventually back-pressures the blocking TCP `send`," which the
crashed-proposer test only sidestepped by not meshing to the offline node.
With blocking writes, once a wedged peer's receive window fills, an honest
node's `write_all` blocks and the whole node stalls — a real liveness
hole. Non-blocking buffered links close it with no protocol change,
composing `mini_bearer`'s existing framing rather than adding a new
transport primitive.

**Constitutional impact:** none at the invariant level — this is transport
robustness, not consensus semantics. It strengthens Directive 4's
liveness posture (an honest node's progress no longer depends on every
peer draining its socket). Finality, locking, and vote/proposal
authentication are untouched; the consensus payload stays
self-authenticating, so the non-blocking best-effort transport can drop or
delay a frame but never forge or reorder a *decision*.

**Implementation status:** shipped and tested. `net::TcpMesh` rewritten
around a private non-blocking buffered `Link`; a deterministic unit test
(`a_peer_that_never_reads_cannot_block_us_or_grow_our_buffer_past_the_cap`)
offers 64 MiB to a peer that reads nothing and asserts the call never
blocks and the outbound buffer stays within its cap. Both networked tests
(happy-path convergence and crashed-proposer view-change) still pass.
`cargo fmt`/`clippy -D warnings`/`test` clean.

**Failure point:** the buffer cap means a *slow* honest peer that briefly
falls far enough behind loses the frames dropped past the cap — acceptable
because vote/proposal delivery is best-effort and safety never depends on
any single message, but it underscores the still-open **application-level
re-gossip** gap (D-0202's residual #1): a genuinely dropped past-round vote
is not re-delivered, so the POLC-re-proposal path is only as robust as the
links are lossless. This decision fixes the *transport* half of "robust
vote gossip"; the re-flooding half remains.

**Supersedes / superseded by:** extends D-0200's `TcpMesh` (replaces its
blocking `TcpBearer` links with non-blocking buffered ones) without
altering the D-0201/D-0202 round engine, wire, or node. Supersedes
nothing.

---

### D-0204 — `mini-consensus` detects and surfaces validator equivocation (double-signing) as verifiable evidence  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0201, D-0202,
`docs/design/networked-consensus.md`, Directive 4, INVARIANTS P2.

**Decision:** now that every vote is signed (D-0201) and every proposal
too (D-0202), `mini-consensus` detects when a validator root double-signs
— two *different* votes for the same `(height, round, phase)` — and
surfaces it as a portable `EquivocationEvidence` (the two conflicting
signed votes), verifiable by anyone via `verify_equivocation`. The
`Round` now records each root's *first* vote per phase and counts only
that: a conflicting second vote is dropped from the tally (not merged) and
returned as `Action::Equivocation`, which the node re-emits as
`Emit::Equivocation`. Detection and proof only — no slashing, no
validator-set mutation, no finality gated on it.

**Reason:** the D-0202 residual list named equivocation evidence as a next
slice, and signed proposals/votes make it essentially free: double-signing
is now cryptographically provable. It also tightens the tally — previously
an equivocating root's two votes could each land in a *different* hash's
prevote set (never enough to push either past quorum, since each hash
dedupes by root, but untidy); counting only the first vote per phase makes
"one root, at most once" (P2) hold per-phase, not just per-hash.

**Constitutional impact:** strengthens P2's "counted at most once" posture
(now enforced per phase, and a violation is *detected*, not merely
diluted) and Directive 4's accountability posture. No invariant is
weakened and none is added: finality is still exactly
`mini_chain::verify_finality`, and an equivocator could never manufacture a
quorum before or after this change — the difference is that its attempt is
now surfaced as evidence rather than silently absorbed. No
`docs/INVARIANTS.md` row changes; no voice/value edge.

**Implementation status:** shipped and tested. New `mini_consensus::evidence`
module (`EquivocationEvidence` + `verify_equivocation`); `Round` gains
per-root first-vote tracking and `Action::Equivocation`; node gains
`Emit::Equivocation`. Six new tests: genuine evidence verifies; identical
votes, votes at different slots, votes from different roots, and a forged
second vote are all rejected as non-evidence; and a `Round`-level test
proves a double-signing root is reported *and* its first vote still counts
exactly once toward quorum (the second does not). `cargo fmt`/`clippy -D
warnings`/`test` clean; the happy-path and view-change networked tests
still pass.

**Failure point:** stated plainly. This is detection, not enforcement —
there is no slashing, no automatic ejection, and nothing consumes the
evidence yet, so a double-signer pays no penalty beyond being provably
caught. Evidence is only produced for votes a node actually *receives*;
without full vote re-gossip (still open) a node may never see both halves
of a distant equivocation. And proposal-equivocation (a proposer signing
two different proposals for one round) is not yet collected here — only
vote-equivocation is.

**Supersedes / superseded by:** extends D-0201's round engine (adds
per-phase first-vote counting and equivocation surfacing) and builds on
D-0201/D-0202's signed votes/proposals. Supersedes nothing.

---

### D-0205 — `mini-consensus` re-gossips messages and runs over partial (connected) meshes  ·  *Accepted*
**Date:** 2026-07-11 · **Refs:** roadmap #36–#45, #92, D-0200, D-0203,
`mini-net::GossipRouter` (the dedup-flood shape this reuses),
`docs/design/networked-consensus.md`, Directive 4.

**Decision:** `net::run_to_height` now **dedup-floods** every consensus
message: the first time a node sees a message (id = BLAKE3 of its canonical
wire bytes, tracked in a bounded `SeenCache`) it re-broadcasts it across its
own edges and processes it; a repeat is dropped. A node also marks its own
outgoing messages seen, so a copy flooded back is not re-flooded. Paired
with a new `TcpMesh::establish_topology`, which builds an arbitrary
(partial) edge set rather than only a full mesh, this makes consensus live
over **any connected graph** — a vote reaches a non-adjacent peer purely by
relay.

**Reason:** the D-0203 residual list's top item was "robust vote gossip";
D-0203 shipped its transport half (non-blocking sends), and this ships the
application half. It is the difference between "every validator must be
directly connected to every other" and "the validator graph need only be
connected," which is what any real P2P deployment requires. Reuses the
"forward once, then drop duplicates" design `mini-net::GossipRouter`
already owns rather than inventing a new one (Directive 14).

**Constitutional impact:** none at the invariant level — this is message
propagation, not consensus semantics. It strengthens Directive 4's liveness
posture (honest nodes converge without a complete graph). Finality,
locking, signing, and equivocation detection are unchanged; re-gossiped
messages are re-verified on receipt exactly as directly-delivered ones are,
so flooding a tampered or duplicate message changes nothing.

**Implementation status:** shipped and tested. `net.rs` gains `SeenCache` +
`message_id` + re-gossip in the run loop, and `TcpMesh::establish_topology`.
A new real-socket test runs a four-node **line** 0—1—2—3 (endpoints share no
link) and asserts all four still finalize and converge — and it was
confirmed to *stall* when the re-broadcast is removed, so it genuinely
exercises relay. Both full-mesh tests still pass. `cargo fmt`/`clippy -D
warnings`/`test` clean.

**Failure point:** stated plainly. Re-gossip re-delivers messages *still
circulating*; it is **not state-sync** — a node that was down for a whole
height and missed those messages entirely will not catch up from flooding
alone (a snapshot/catch-up protocol is separate, later work). The
`SeenCache` is bounded, so on a very long-running or high-volume node an id
evicted long ago could be re-processed once more (harmless — the round
layer is idempotent and re-verifies). Flooding also multiplies traffic on
dense graphs (every node re-sends every new message once); fine at this
scale, a gossip-degree/fanout limit is future tuning. Still threads over
loopback, not machines over the internet, and peer *discovery* is still not
built — the topology is supplied, not learned.

**Supersedes / superseded by:** extends D-0200's `TcpMesh`/`run_to_height`
(adds partial topologies and re-gossip) and completes the application half
of the "robust vote gossip" work D-0203 began. Supersedes nothing.

---

### D-0206 — `mini-consensus` links are now confidential and tamper-evident: `mini_bearer::Channel` wired into `TcpMesh`  ·  *Accepted*
**Date:** 2026-07-12 · **Refs:** roadmap #36–#45, `crates/mini-consensus/src/net.rs`,
`mini_bearer::Channel`/`Initiator`/`Responder`, the founder-supplied
2026-07-12 in-depth review (`5.3 Connectivity`/`5.4 Consensus`: "wire
authenticated encrypted channels into consensus now; do not wait for the
full mixnet"), D-0203, Directive 14.

**Decision:** every [`TcpMesh`] link now completes a `mini_bearer::Channel`
handshake (ephemeral X25519 + HKDF-SHA256 + ChaCha20-Poly1305, forward-
secret, anonymous — the exact same construction `mini-sync`/`mini-cli`'s
`sync connect`/`listen` already use, composed here rather than reinvented)
before any consensus byte crosses the wire. The dialer is always the
handshake initiator, the accepter always the responder — the same
asymmetry `establish_topology` already uses to stay deadlock-free, now
extended to the application-level handshake too, not just the TCP
connect. Every `queue()`d frame is sealed through the link's `Channel`
(with a fixed `CONSENSUS_AAD` domain tag, the same discipline `mini-sync`'s
`SYNC_AAD` follows) before framing; every received frame is opened before
being handed to `ConsensusMessage::from_wire_bytes`. The handshake itself
is a small, bounded blocking exchange (bounded by a new
`HANDSHAKE_TIMEOUT`) that runs before the socket switches to non-blocking
mode for ordinary operation — everything after the handshake keeps
D-0203's non-blocking, never-back-pressured behavior unchanged.

**Reason:** `mini_bearer::Channel` already existed, already composes only
reviewed primitives, and was already wired into `mini-sync`/`mini-cli`'s
sync protocol — `mini-consensus` was the one real-network transport in
this tree that still moved votes and proposals as a cleartext pipe, a gap
this crate's own docs and D-0203's "Required follow-up" already named.
The founder's 2026-07-12 review named this the correct next step ahead of
a full mixnet ("wire authenticated encrypted channels into consensus now;
do not wait"), and closing it required no new cryptography — composition
only (Directive 14).

**Constitutional impact:** none at the invariant level — this is
transport confidentiality, not consensus semantics. `Channel`'s handshake
is deliberately anonymous (proves nothing about which validator is on the
other end), so it adds no authentication, authority, or identity beyond
what the self-authenticating signed payload already provides; finality,
locking, signing, and equivocation detection are entirely unchanged, and
every consensus payload is re-verified after decryption exactly as it was
after cleartext delivery before. No `docs/INVARIANTS.md` row changes.

**Implementation status:** shipped and tested. A new adversarial test
(`queued_frames_cross_the_wire_as_ciphertext_never_plaintext`) sends a
distinctive plaintext marker and proves it never appears verbatim in the
raw bytes that actually cross a real loopback socket, then proves that
isn't mere corruption by decoding the same bytes through the real channel
and recovering the exact original marker — the concrete regression this
decision closes. The existing `a_peer_that_never_reads_...` liveness test
was updated to complete a real handshake on both sides first (the
outbound-buffer-capacity property it tests is otherwise unaffected). All
three existing real-socket networked tests (full mesh, partial line mesh
via re-gossip, crashed-proposer view-change) pass unmodified — the
handshake is fully transparent to `TcpMesh::establish`/`establish_topology`'s
public API and `run_to_height`'s driver loop, neither of which changed.
`cargo fmt`/`clippy -D warnings`/`test` clean.

**Failure point:** stated plainly. This closes eavesdropping and
tampering by an on-path observer, not Sybil connections — there is still
no discovery, so a malicious *first* connection from an unknown address
is exactly as possible as before; `Channel`'s handshake proves the two
ends share a fresh private session, never *which* validator is on the
other end. The blocking handshake means link establishment is no longer
instant-return the way a bare TCP `connect` was (a dialer now genuinely
waits on the peer's application-level response, not just the kernel's
accept backlog) — bounded by the new `HANDSHAKE_TIMEOUT`, and the
dial-only-higher-indexed-peers convention keeps the wait graph acyclic,
so this remains deadlock-free, just no longer free of latency. A single
decryption failure on a link (garbage, or a peer whose channel state
somehow desynced) permanently strands that link — `Channel` requires
strict in-order processing with no resync mechanism — which degrades
that link to the same fate a silently-dead peer already had; safety never
depends on it since payloads are still independently self-authenticating.

**Required follow-up:** peer discovery and NAT traversal (`mini-net`'s
job, unstarted for consensus specifically); a link that dies from a
desync should eventually be redialed rather than staying permanently
inert for the life of the process (no reconnect logic exists anywhere in
`TcpMesh` yet, encrypted or not — a pre-existing gap this decision doesn't
change); the review's broader `5.3`/`5.4` asks (formal BFT modeling,
dynamic validator sets, state sync, chaos testing at real scale) remain
entirely open and are much larger, separate work.

**Supersedes / superseded by:** extends D-0203's non-blocking `TcpMesh`
(the handshake precedes, and does not alter, its non-blocking buffered
send/receive behavior) without altering D-0200/D-0201/D-0202/D-0204/D-0205's
round engine, wire format, or re-gossip logic. Supersedes nothing.

---

### D-0081 — No-GitHub outage demo: a real, narrated, runnable script through the whole spine  ·  *Accepted*
**Date:** 2026-07-12 · **Refs:** roadmap #102 (self-hosted forge spine),
`tools/no_github_outage_demo.sh`, `crates/mini-cli/tests/
no_github_outage_demo.rs`, D-0080 (its own "Required follow-up" named
this exact combination).

**Decision:** `tools/no_github_outage_demo.sh` is a real, narrated shell
script — driving the compiled `mini` binary, never a library call —
that carries three identities through the entire self-hosted forge
spine lifecycle in one continuous run: identity init, out-of-band KEL
trust, `repo init`/`commit`/`pr propose`/two independent `pr approve`
calls/`pr merge`, `release create`/two independent `release attest`
calls/`release verify`, a full install (`installer stage → preflight →
activate → health-check`) that passes — and then, because a real system
must survive its own mistakes too, a second, deliberately broken
release through the identical path that fails its health check,
auto-rolls back with no manual intervention, and leaves behind an
event log that `installer verify-log` confirms is clean. Every step
uses `--json` (D-0078) to extract real fields (`release_id`,
`artifact_digest`) between commands rather than scraping text, doubling
as a demonstration of that contract. `crates/mini-cli/tests/
no_github_outage_demo.rs` runs the script itself as a subprocess so a
broken demo fails `cargo test --workspace` like any other regression,
rather than silently rotting until a human runs it by hand.

**Reason:** nothing in this codebase has ever made a network call to
any GitHub endpoint — there is no "outage" to route around at the code
level, so the honest, checkable claim is narrower and more useful than
a network-partition drill would be: read this script, run it yourself,
and see the entire developer lifecycle — including the failure-recovery
path — complete without GitHub ever being named, required, or even
capable of blocking it. D-0080 already proved the wire protocol needs
nothing GitHub-shaped; this is the narrated, single-artifact version a
non-Rust-reading auditor or founder can actually run and follow.

**Constitutional impact:** none — a demonstration artifact, no new code
path in any library crate. Reaffirms the same claim CLAUDE.md's own
"GitHub is this project's UAT/mirror, never its source of truth" line
already makes, now backed by a runnable proof rather than only a
policy statement.

**Implementation status:** shipped. The script needed two real fixes
found only by actually running it against the compiled binary, not by
reasoning about it on paper: (1) a `--json`-based artifact-digest
extraction replaced an initial draft that computed a SHA-256 digest
locally (wrong algorithm entirely — this workspace uses BLAKE3
throughout) and then tried to recover the real digest through a
fragile probe-release fallback; (2) Bob and Carol needed an explicit
`repo track` step before `pr merge`/`installer stage` could resolve the
project alias, the same requirement every other multi-identity CLI test
in this crate already has to satisfy. Full workspace `cargo test
--workspace --all-features` green (111 test results, 0 failures).

**Failure point:** this environment has no controlled way to actually
sever GitHub reachability and verify nothing breaks — the claim rests
on reading the codebase's dependency graph (no `octocrab`/`reqwest`-to-
github.com/GitHub-API-client dependency exists anywhere) plus this
script's own successful run, not a live firewall drill. `bash`-specific
syntax (not POSIX `sh`) is required to run it, matching this repo's
existing `tools/` scripts' conventions (`mininet_nav.py` already assumes
a real Python interpreter, not portable-shell-only).

**Required follow-up:** per the roadmap's own sequencing, the project
has now earned the right to widen into Branch A (hardware gates #97/
#98), Branch B (economic simulation #47/#50), Branch C (personhood
research #21), and Branch D (DTN #28) — a founder priority call, not
something to pick unilaterally.

**Supersedes / superseded by:** none. Composes D-0078's `--json`
contract and D-0080's sync proof into one narrated artifact without
altering either.

---

### D-0082 — Integrate the founder-supplied Governance Pack v1.0 as subordinate, supplementary process material  ·  *Accepted*
**Date:** 2026-07-12 · **Refs:** `docs/GOVERNANCE_PACK_INTEGRATION.md`
(full compatibility matrix), `docs/governance/*` (50 docs, `CHANGELOG.md`,
`RFC-0001`–`RFC-0005`), `forge-native/schemas+examples`, `governance/`
(policy config), `tools/check_governance.py`, CLAUDE.md's founder
"AI Contributor Transition Plan" instruction.

**Decision:** land the founder-supplied `mininet-governance-pack-v1.0.zip`
(83 files) as a new, explicitly subordinate documentation/tooling layer:
`docs/governance/` (the pack's ~50 normative process/specification
documents plus its RFCs and changelog, copied verbatim), `forge-native/`
(five JSON Schemas + three worked examples for a future signed Forge
governance-object encoding, verbatim, all validated as parseable JSON),
and a new top-level `governance/` policy-config directory
(`policy.yml`/`exceptions.yml`/`document-summary.schema.json`) plus
`tools/check_governance.py`, the pack's reference validator. On the
GitHub-facing side: `.github/ISSUE_TEMPLATE/*` (new issue forms, purely
additive — none existed before) and `.github/CODEOWNERS.template` (kept
as a template, not a live `CODEOWNERS`, since the GitHub teams it
references don't exist yet) are activated; a `governance-policy.yml` CI
workflow is activated but trimmed to only its `governance-baseline` job
(`continue-on-error: true`, matching `ci.yml`'s existing
`dependency-audit`/`dependency-deny` advisory pattern). The pack's
expanded 13-heading PR template and its second CI job
(`proposal-metadata`, which hard-requires those headings) are staged at
`repository-template/` — present, reviewable, verbatim — but **not**
wired into any live path, because this repo's actual
`.github/pull_request_template.md` doesn't produce those headings yet and
wiring the checker in regardless would fail on every PR by construction.

**Reason:** the founder's own transition-plan instructions require (1)
reading every existing governance-related document first, (2) building an
explicit compatibility matrix per pack document (already exists / overlaps
/ supersedes / supplements / conflicts) before adding anything, and (3)
never silently replacing documentation or inverting the constitutional
hierarchy. `docs/GOVERNANCE_PACK_INTEGRATION.md` is that matrix,
maintained as a living document for future pack versions (v1.1 was
flagged as forthcoming).

**Constitutional impact:** none — the pack's own `docs/governance/
00_GOVERNANCE_INDEX.md` and `01_DEVELOPMENT_CONSTITUTION.md` both state
explicitly that the pack is subordinate to `SPEC-00`/`docs/INVARIANTS.md`/
`docs/DECISION_LOG.md` and "does not create a second Constitution." This
PR changes none of `docs/FOUNDER_DIRECTIVES.md`, `docs/INVARIANTS.md`, or
any prior `docs/DECISION_LOG.md` entry — verified byte-identical before
and after. No voice/value edge; no new authority granted to anyone or
anything (the CI job that runs is read-only and advisory; the CODEOWNERS
file that would grant real review-routing authority is deliberately kept
inert).

**Implementation status:** shipped as documentation, schemas, and
non-blocking tooling only — explicitly **not** claimed as implemented
governance. `python3 tools/check_governance.py --mode baseline` passes
clean (0 errors, 0 warnings) against this repo's real tree. All eight
`forge-native/` JSON files parse. `docs/_generated/*` regenerated to
index the new files. One field deliberately deviated from the pack as
shipped: `.github/ISSUE_TEMPLATE/config.yml`'s `blank_issues_enabled`
was changed from the pack's `false` to `true`, to avoid silently
disabling the free-form issue creation the founder has used for the
existing #8–#93 roadmap issues — recorded explicitly in
`docs/GOVERNANCE_PACK_INTEGRATION.md`'s "Deviated from the pack" section
rather than adopted silently.

**Failure point:** stated plainly. Every pack document not already backed
by real code in this tree (the large majority — see the compatibility
matrix's "specified only" / "net-new" rows) is exactly that: a design
proposal, not evidence of enforcement, per the pack's own truth-boundary
language quoted in `docs/GOVERNANCE_PACK_INTEGRATION.md`. `governance/
policy.yml`'s `protected_paths` and `.github/CODEOWNERS.template`
reference GitHub teams (`reviewers-constitution`, `security-stewards`,
...) that do not exist; nothing in this repository grants them any real
review-routing authority until the founder creates those teams and
promotes the template to a live `CODEOWNERS` file. Three numbering
systems (`D-00xx`/`D-02xx`, `SPEC-xx`, and the pack's new `RFC-000x`) now
coexist in this repository with different authority levels — flagged
explicitly so a future contributor does not conflate an RFC reference
with an accepted decision.

**Required follow-up:** founder-privileged GitHub setup (team creation,
branch rulesets, CODEOWNERS activation — `docs/governance/
13_REPOSITORY_OWNER_SETUP_GUIDE.md` and `repository-template/
GITHUB_RULESETS_BLUEPRINT.md` have the concrete steps); a founder decision
on whether/when to adopt the expanded PR template and wire in the
`proposal-metadata` CI job (Phase B of the pack's own `27_
REPOSITORY_INTEGRATION_PLAN.md`); reconciling `38_
V05_V06_IMPLEMENTATION_BACKLOG.md`'s parallel backlog against the
existing GitHub roadmap (#8–#93, hub #92) rather than letting two
backlogs drift independently; repeating this same read-everything-first,
build-a-matrix-first process for the promised v1.1 pack.

**Supersedes / superseded by:** none. Supplements every existing
canonical document without altering any of them.

---

### D-0083 — Temporary Founder-guarded GitHub integration exception  ·  *Accepted (explicit Founder bootstrap override; temporary)*
**Date:** 2026-07-12 · **Refs:** D-0033, D-0082,
`governance/policy.yml`, `governance/exceptions.yml`, Founder instruction in
the recorded PR session.

**Decision:** temporarily and explicitly supersede D-0033's repository
approval floor for integration into GitHub `main` during the founder-only
bootstrap window. This is the current repository approval rule while the
Founder is the only human contributor.
The Founder is the sole mechanical GitHub merge operator. A pull request may
merge without two independent human approvals after its required checks pass,
review conversations are resolved, the final head is inspected, AI assistance
is disclosed, and the Founder accepts responsibility for that exact state.
AI agents may engineer, test, and review; their combined approval weight is
zero.

This is a real procedural weakening of D-0033, not a claim that one AI or one
Founder equals independent quorum. Its narrow purpose is to keep engineering
moving until independent human maintainers join. It does **not** lower
`mini-forge::governance::PROTOCOL_MIN_APPROVALS`, the two-attestation governed
release floor, cryptography audit gates, owner-adoption freedom, any Tier-F
invariant, or the quorum needed for Forge canonicalization or a production
release.

The exception ends at the earliest recorded occurrence of:

1. 2026-10-12T23:59:59Z;
2. appointment of two independent human maintainers able to review the same
   exact state;
3. preparation of any release represented as production-ready; or
4. Mininet Forge becoming the canonical integration surface.

`governance/bootstrap-operating-state.json` records these trigger facts. The
blocking validator fails when the expiry arrives or any earlier trigger is
recorded. External reality still requires an honest update to that record;
the file cannot discover appointments, release claims, or Forge cutover by
itself. On sunset, D-0033's normal two-human floor returns without another
Decision. If platform rules have not yet been updated, canonical merges must
stop until they match. Renewal requires a new D-number before expiry; silence
cannot renew it. Each use must remain visible in its pull request and preserve
the head digest, checks, adverse AI findings, and Founder merge action.

**Constitutional impact:** temporary, scoped weakening of D-0033's GitHub
integration procedure; no frozen principle or participant-adoption right is
weakened.

**Implementation status:** represented in `governance/policy.yml` and
`governance/exceptions.yml`, with trigger facts in
`governance/bootstrap-operating-state.json`; enforced on GitHub through a pull-request-only
`main` ruleset with blocking checks, zero required approvals, no force pushes,
and no branch deletion. AI remains non-authorizing.

**Supersedes / superseded by:** partially supersedes D-0033 for GitHub `main`
integration only and only until the first sunset condition. D-0033 remains in
force everywhere else and resumes fully at sunset.

---

### D-0084 — Activate the Primary AI Engineer Charter v1.1  ·  *Accepted (operational, non-authorizing)*
**Date:** 2026-07-12 · **Refs:** D-0082, D-0083, `GOV-AI-050`,
`governance/ai-charter-activation.json`,
`governance/decisions/D-0084.json`, `governance/current-phase.json`.

**Decision:** recognize the `founder-guarded` phase recorded by D-0083 and
activate the exact v1.1 Primary AI Engineer Charter, repository-root
`AGENTS.md` adapter, and machine-readable summary bound by the structured
final Decision at `governance/decisions/D-0084.json`. Classification is
operational. The charter coordinates engineering work and grants no AI
approval, quorum, merge, canonicalization, release, administration, secret,
treasury, constitutional, or owner-adoption authority.

No cooling interval is required: the Founder explicitly selected the bounded
bootstrap profile, the charter is non-authorizing, the activation is
content-addressed and reversible only by a later recorded Decision, and the
temporary canonical-integration authority is separately and visibly governed
by D-0083.

The activated charter remains applicable only while the canonical phase is
`founder-guarded` or `maintainer-assisted` and all activation digests and time
gates verify from a separately trusted canonical checkpoint. A changed
instruction surface cannot activate itself. Append-only supersession uses:

`AI-Charter-Activation-Superseded: D-0084 -> <new-decision>`

**Implementation status:** activated for the exact digests in the structured
Decision. Presence of a different local file or proposal branch is not
activation and does not prove model compliance.

**Supersedes / superseded by:** activates the v1.1 charter introduced by
D-0082; superseded only by a later exact-state Decision and the append-only
marker above.

---

### D-0085 — Consensus edge-case attack review: timestamps, replay, fee manipulation (closes #44)  ·  *Accepted*
**Date:** 2026-07-12 · **Refs:** roadmap #44, `crates/mini-chain/src/vote.rs`,
`crates/mini-execution/src/chain.rs`, `crates/mini-execution/src/error.rs`,
`crates/mini-consensus/src/node.rs`, `crates/mini-value/src/fee.rs`,
`crates/mini-value/src/error.rs`, `docs/THREAT_MODEL.md` §2, CLAUDE.md's
typed-domain rule, Directive 4, Directive 14.

**Decision:** a real, code-first review of the three attack classes
issue #44 named as sharing one root cause ("trusting caller-supplied
context without independent verification"), fixing what was actually
found rather than only cataloging it:

1. **Timestamps.** `BlockHeader::timestamp_ms` was proposer-controlled
   and completely unchecked. `LedgerChain::apply_finalized_block` (the
   one authoritative, unconditional gate every honest chain applies) now
   rejects a header whose `timestamp_ms` does not strictly exceed the
   previous finalized block's, returning a new
   `ExecutionError::NonMonotonicTimestamp`. `mini-consensus`'s
   `validate_proposal` mirrors the identical check at prevote time as a
   cheap early filter (rejecting before a round wastes a step on it), via
   a new `LedgerChain::last_timestamp_ms()` getter.
2. **Replay (domain confusion).** Every *other* signed transcript in this
   workspace (`mini-consensus::wire::Proposal`, `mini-settlement::claim`,
   `mini-bounty::claim`) already prepends a fixed ASCII domain tag before
   signing — `Vote::transcript` was the one exception, signing a bare
   `kind || height || round || block_hash` with no tag identifying it as
   a vote, since `did_mini::Controller::sign_message` has no domain
   concept of its own. A new `VOTE_SIGN_DOMAIN` constant closes the gap,
   deliberately distinct from `Vote::to_wire_bytes`'s own framing tag
   (the same separation `mini-consensus::wire` already keeps between its
   `DOMAIN` and `PROPOSAL_SIGN_DOMAIN`).
3. **Fee manipulation.** `mini_value::fee::PriceHistory::add_entry`
   accepted a governed price of `0`, which would make every fee free
   regardless of the real-world value target. A new `ValueError::ZeroPrice`
   variant rejects it unconditionally.

**Reason:** issue #44 asked for a review; a review that only documents
"this is trusted but shouldn't be" without fixing what's fixable inside
existing crate boundaries would leave the same exploitable gaps in place
under a green checkmark. All three fixes compose only what already
exists in this tree (an existing error-enum pattern, an existing
domain-tag convention, an existing rejection pattern in the same
function) — no new cryptography, no new wire fields, no new public API
surface beyond one getter and two error variants (Directive 14).

**Constitutional impact:** strengthens Directive 4 (two honest chains
enforcing the same monotonicity rule can never disagree because of it —
the check is unconditional and identical on every node) and the
typed-domain rule (CLAUDE.md) directly — `Vote::transcript` now matches
every sibling signed transcript's domain-separation discipline instead of
being the one exception. No `docs/INVARIANTS.md` row changes: these are
hardening fixes within already-decided constructions, not new invariants.
No voice/value edge — the fee fix rejects an objectively-broken value
(zero), it does not add or change any authorization/governance rule
(this crate still has, and states it has, no opinion on *who* may call
`add_entry`).

**Implementation status:** shipped and tested. Four new adversarial
tests, one per finding plus the domain-confusion regression: `mini-chain`
proves a signature over the pre-fix undomained transcript layout no
longer verifies as a vote (while the real domain-tagged signature still
does, isolating the assertion to the domain tag specifically);
`mini-execution` proves a height-2 block with the identical `timestamp_ms`
as height-1 is rejected with the chain correctly not advancing;
`mini-consensus` proves an authentic, correctly-signed, wrong-proposer
timestamp gets prevoted `nil` (not silently dropped, and never for its
own invalid hash) — verified against the *networked* real-TCP-mesh tests
too, confirming the monotonicity check doesn't break real convergence
(production code already always sets `timestamp_ms: height`, which is
naturally strictly increasing); `mini-value` proves a zero-price entry is
rejected and never recorded, both as a first entry and as a follow-on
entry after a real one. Full workspace `cargo test --workspace
--all-features` green (114 test-binary results, 0 failures) after the
change; `cargo fmt`/`clippy -D warnings` clean.

**Failure point:** stated plainly, per finding. *Timestamps:* this closes
monotonicity only, not a real wall-clock freshness/skew bound — no
wall-clock semantics exist anywhere in this tree yet (`timestamp_ms` is
still documented as "ordering hint," not real time), so inventing a
clock-skew check now would be premature relative to that larger
undecided design question. *Replay:* this closes same-workspace
cross-domain confusion (a vote signature can no longer be replayed as
anything else, or vice versa); it does **not** add a chain-id/network-id
concept, so a validly-signed vote from one *separate deployment* of this
protocol (e.g. a testnet sharing a validator set with a devnet) remains
structurally identical to one from another — no such multi-deployment
concept exists anywhere in this codebase today, and introducing one is
materially larger, protocol-wide, wire-breaking work belonging to its own
decision, not bundled into this review. *Fee:* only the zero-price defect
is fixed; there is still no rate-limit/max-jump bound between consecutive
governed prices, and this crate still enforces no authorization on who
may call `add_entry` at all — both are genuine policy questions for
whoever wires real fee governance, not something to invent unilaterally
here.

**Required follow-up:** a real wall-clock freshness bound for
`timestamp_ms` once this tree adopts real wall-clock semantics; a
chain-id/network-id concept threaded through `Vote`/`Proposal`/
`PaymentClaim` transcripts if/when multiple separate deployments of this
protocol are expected to coexist (materially larger, its own decision);
a rate-limit/max-jump bound on governed price changes, and real
authorization enforcement on `PriceHistory::add_entry`, both founder/
governance-policy calls. `docs/THREAT_MODEL.md` §2 gained three new rows
recording all of the above honestly (✅/Partial, not overclaimed).

**Supersedes / superseded by:** none. Extends D-0200-D-0203's consensus
work (`mini-consensus`) and D-0055/D-0061's settlement/execution work
without altering either's existing behavior for any previously-valid
input.

---

### D-0086 — Rename `HumanStatus::FullHuman` to `EvidenceQualifiedHuman` (personhood-honesty naming fix)  ·  *Accepted*
**Date:** 2026-07-12 · **Refs:** founder review
`Mininet_In_Depth_Review_20260712.md` (`personhood-honesty` finding),
`crates/mini-uniqueness/src/status.rs`, `docs/INVARIANTS.md`'s hard
limitations section, `docs/DECISION_LOG.md` D-0054 (the original
`HumanStatus` accumulator), Directive 4, "Honesty over polish" (CLAUDE.md).

**Decision:** rename the `HumanStatus` enum variant `FullHuman` to
`EvidenceQualifiedHuman` across `mini-uniqueness` (`status.rs`, `lib.rs`),
its five directly-named test functions, and the one current-state doc
reference in `crates/mini-uniqueness/README.md`. A doc comment on the
enum now states plainly why: this crate cannot yet distinguish one human
from several colluding identity roots (Sybil resistance is the still-open
question `docs/INVARIANTS.md` names as a hard, temporary limitation at
its top), so no variant name may imply a certainty — "full," "verified" —
the evidence doesn't support. `VouchedHuman` and `Unverified` were already
honestly named; only the top tier overclaimed.

This is a pure rename: no field, threshold, promotion rule, decay
behavior, or public function signature changes. `PromotionPolicy`'s
internal tuning-knob field names (`full_score_threshold`,
`full_minimum_age_ms`, `full_minimum_distinct_sources`,
`full_required_sources`) are deliberately left as-is — they are private
configuration knobs, not the public-facing status claim the review
flagged. Historical references to `FullHuman` in `docs/DECISION_LOG.md`
(append-only, never edited), `docs/design/human-continuity-proof.md`,
`docs/gates/personhood-signal-b-decision.md`, and
`docs/audits/issue-18-sybil-social-graph-review.md` are left unchanged as
an accurate record of what was true when each was written.

**Reason:** the founder review's exact words: *"The current naming is
also dangerous: `HumanStatus::FullHuman` can sound stronger than the
evidence justifies. Until the mechanism resists duplicate roots, this
should be named `EvidenceQualifiedIdentity` or similar."* `EvidenceQualifiedHuman`
was chosen over the review's suggested `EvidenceQualifiedIdentity` to keep
the noun consistent with its sibling variants (`VouchedHuman`,
`Unverified` — implicitly of a human) and the crate's existing
`HumanRecord`/`HumanStatus` naming family, while still landing on the
same "evidence-qualified, not verified" framing the review asked for.

**Constitutional impact:** direct compliance with CLAUDE.md's "Honesty
over polish" hard rule and the "Never claim 'one human, one vote'"
rule's spirit — no code path anywhere in this tree now uses a status
name that could be read as a personhood guarantee this system does not
provide. No `docs/INVARIANTS.md` row changes: this does not touch the
Sybil-resistance mechanism itself, only its self-description. No
voice/value edge — `mini-uniqueness` has no dependency on any value
crate in either direction, unaffected by this change.

**Implementation status:** shipped. `cargo fmt --all`, `cargo clippy
--all-targets --all-features --workspace -- -D warnings`, and `cargo test
--workspace --all-features` all pass unchanged in behavior (test count
and outcomes identical; five test functions renamed to match, not
rewritten). `python3 tools/mininet_nav.py build` regenerated the nav
index.

**Failure point:** if a future PR reintroduces a status name implying
verified personhood (`FullHuman`, `VerifiedHuman`, or similar) anywhere
in this tree, that is the exact regression this Decision closes —
`docs/INVARIANTS.md`'s Sybil-unsolved limitation still applies to every
`HumanStatus` variant, including this one.

**Required follow-up:** none code-side. The underlying Sybil-resistance
question (roadmap #18) remains open and is not advanced by this naming
fix — it only prevents the naming from overstating progress on it.

**Supersedes / superseded by:** none. Purely a naming clarification on
top of D-0054's `HumanStatus` accumulator; does not alter its behavior.

---

### D-0087 — Tighten timestamps to deterministic logical time; fix fee-conversion overflow (roadmap #44, reconciles PR #121)  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** roadmap #44, D-0085, PR #121
(`agent/issue-44-consensus-edge-cases`), `crates/mini-execution/src/chain.rs`,
`crates/mini-execution/src/error.rs`, `crates/mini-consensus/src/node.rs`,
`crates/mini-chain/src/block.rs`, `crates/mini-value/src/fee.rs`,
`crates/mini-value/src/error.rs`, `docs/audits/issue-44-consensus-edge-cases.md`,
`docs/THREAT_MODEL.md` §2, Directive 4, Directive 14.

**Decision:** two independent AI sessions were directed at issue #44 in
parallel against the same pre-D-0085 `main`: this repository's own PR #119
(shipped as D-0085) and a second review submitted as PR #121. Rather than
merging two diverging fixes for the same already-shipped mechanism, PR
#121's diff was read in full and reconciled here on top of the already-
merged D-0085, keeping what was genuinely stronger and skipping what was
redundant with D-0085's own fix:

1. **Timestamps, tightened.** D-0085 required each block's `timestamp_ms`
   to strictly exceed the previous finalized block's. PR #121 correctly
   noted the residual gap: a merely-increasing value still lets a
   malicious proposer jump straight to `u64::MAX` in one step. Adopted
   PR #121's stronger design instead: `timestamp_ms` is deterministic
   logical time, required to equal the block's own height exactly, in
   both `LedgerChain::apply_finalized_block` (the authoritative gate) and
   `mini-consensus`'s `validate_proposal` (the cheap early filter). The
   proposer now has no discretion over this field at all, not merely a
   bound on it (Directive 14: the smaller, fully-determined rule beats a
   bespoke bound). `LedgerChain::last_timestamp_ms()` is removed — it is
   now redundant with `height()`, and an unused getter is worse than none.
2. **Fee-conversion overflow, a genuine new finding.** PR #121 found a
   real, previously-undetected bug D-0085 did not touch:
   `fee_in_micro_mini`'s final step cast a `u128` intermediate down to
   `u64` with `as`, which truncates silently on overflow instead of
   failing — `fee_in_micro_mini(u64::MAX, u64::MAX)` produced a wrong,
   wrapped-around fee rather than an error, despite the function's own doc
   comment claiming overflow safety. `fee_in_micro_mini` now returns
   `Result<u64>`, using `u64::try_from` to reject an unrepresentable quote
   as a new `ValueError::FeeOverflow`. It also now rejects a zero rate
   directly (not only at `PriceHistory::add_entry`'s ingress), matching
   PR #121's "defense in depth" framing — a `PriceEntry`'s fields are
   public, so a zero rate can reach the conversion function without ever
   passing through `add_entry`. A new `PriceHistory::fee_at` convenience
   method (also PR #121's idea) binds historical lookup and checked
   conversion into one call, closing the accidental-current-rate-on-
   historical-work footgun of two separate calls.
3. **Not adopted from PR #121:** its `signed_vote_cannot_be_replayed_in_
   another_context` test. Reading it closely, it only proves ordinary
   signature field-integrity (mutating a signed vote's fields breaks
   verification), which was already trivially true before and after
   D-0085 — it does not exercise the actual cross-domain-type confusion
   D-0085's `VOTE_SIGN_DOMAIN` fix closed. No behavior gap remained to
   pull forward from it.

`ExecutionError::NonMonotonicTimestamp` is renamed to
`TimestampNotDeterministic { expected, got }` since its old name no
longer describes the check (Directive 14/"honesty over polish": an
inaccurate name for a shipped error is a bug, not a compatibility
concern, in a pre-audit, no-external-consumers workspace). Existing tests
referencing the old monotonic behavior are updated to the new
deterministic-time semantics, not merely renamed to keep passing.

**Reason:** the founder's own review-response instruction was to decide
and get `main` green rather than merge two independently-authored,
behaviorally-conflicting fixes for the same already-closed issue. PR
#121 turned out to contain one genuinely stronger design (deterministic
time) and one genuinely new, real bug fix (overflow) worth keeping; both
are adopted here under this repository's own review/test/decision
discipline rather than merging its PR as-is.

**Constitutional impact:** strengthens Directive 4 (two honest chains
enforcing an identical, now fully-deterministic timestamp rule can never
disagree because of it) and Directive 14 (the simpler, total rule
replaces a partial bound) directly. No `docs/INVARIANTS.md` row changes —
hardening within an already-decided construction, not a new invariant. No
voice/value edge: the fee fix rejects objectively-broken values (zero,
unrepresentable), it adds no authorization or governance rule.

**Implementation status:** shipped and tested. `mini-execution`'s
`a_timestamp_that_does_not_equal_the_block_height_is_rejected` replaces
the old monotonicity test; `mini-consensus` gained
`a_proposer_cannot_use_an_increasing_timestamp_to_evade_the_deterministic_check`
alongside the renamed `a_proposal_whose_timestamp_is_not_deterministic_is_
prevoted_nil`. `mini-value` gained
`fee_overflow_is_rejected_instead_of_silently_truncated`,
`a_zero_rate_is_rejected_at_quote_time_too_not_only_at_ingress`, and
`fee_at_binds_historical_lookup_and_checked_conversion_in_one_call`.
`docs/audits/issue-44-consensus-edge-cases.md` records the full review
across both decisions with attribution to PR #121. `docs/THREAT_MODEL.md`
§2's three D-0085 rows are updated in place (its "never edit merged
content" rule applies to `DECISION_LOG.md` entries specifically, not to
the living threat register, which is expected to track current reality)
plus their stray `D-0083` citations — a leftover from D-0085's own
pre-merge renumbering — corrected to `D-0085`.

**Failure point:** if `mini-consensus`'s block-building path (`build_
proposal`) is ever changed to set `timestamp_ms` to anything other than
`height`, every proposal it produces will be self-rejected as non-
deterministic by every honest validator including its own author —
loud, not silent, and this is the intended fail-closed behavior, not a
bug to work around.

**Required follow-up:** unchanged from D-0085 — a real wall-clock
consensus protocol (separate, larger, not started), a chain-id/network-id
concept for multi-deployment replay resistance, and a rate-limit/max-jump
bound plus real authorization on `PriceHistory::add_entry`, both founder/
governance-policy calls.

**Supersedes / superseded by:** partially supersedes D-0085's timestamp
mechanism (monotonic → deterministic) and extends its fee-zero-rate fix
with the overflow fix; does not touch D-0085's replay/domain-separation
fix, which stands unchanged.

---

### D-0088 — KEL freshness pin + equivocation-evidence consequence routing (founder review P0 items `kel-freshness`/`consensus-evidence`)  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** founder review
`Mininet_In_Depth_Review_20260712.md` §5.1/§5.4 and backlog items
`kel-freshness`/`consensus-evidence`, roadmap M3, audit #12 finding F4,
`crates/did-mini/src/freshness.rs`, `crates/did-mini/src/error.rs`,
`crates/did-mini/tests/recovery.rs`, `crates/mini-consensus/src/consequence.rs`,
`crates/mini-consensus/src/net.rs`, `crates/mini-consensus/src/node.rs`,
`crates/mini-consensus/tests/networked_consensus.rs`, Directive 4,
Directive 14.

**Decision:** two independent, previously-documented-but-unimplemented
gaps from the founder review are closed with real code:

1. **KEL freshness, the interim rule.** `verify_delegation`'s own doc
   comment already recommended pinning "the highest `sn` a caller has
   ever seen per SCID, refusing to go backwards" as the interim defense
   against a stale-KEL replay (audit #12's F4 finding: a revoked device
   still looks delegated in an old copy of the root's KEL). That
   recommendation was prose only. `did_mini::FreshnessPins` is it,
   implemented: `check_and_pin(&Kel) -> Result<u64>` verifies a KEL
   normally, then rejects it as `IdentityError::StaleKel` if its `sn` is
   lower than one already pinned for that SCID, and otherwise advances
   the pin to the max of what it held and the new `sn`. This closes the
   *has-already-seen-a-fresher-log* case of the gap; it does not and
   cannot close the *has-never-seen-one* case — that is exactly what
   real witness receipts and gossip-based duplicity proofs (SPEC-01 §7,
   still unbuilt) are for, and this decision does not claim otherwise.
   `crates/did-mini/tests/recovery.rs`'s existing
   `stale_root_kel_still_accepts_revoked_device_the_known_freshness_gap`
   test (which documents the raw gap) is joined by a new
   `freshness_pins_close_this_exact_gap_for_a_verifier_that_has_seen_the_fresh_kel`
   test proving the mitigation actually closes it end to end on the
   identical scenario.
2. **Equivocation evidence, no longer dropped.** `mini-consensus::net`'s
   `handle_emits` had, verbatim, `Emit::Equivocation(_) => {}` — real,
   independently-verifiable proof of double-signing reached the network
   driver and was discarded on the spot, exactly the "evidence is dropped
   by the network driver" gap the review's §5.4 names. A new
   `EquivocatorRegistry` (in a new `consequence` module, deliberately
   separate from `evidence`'s pure detection/verification role)
   independently re-verifies every emitted `EquivocationEvidence` via the
   existing `verify_equivocation` (never trusting the node's own claim)
   and records the root, deduplicated, if genuine. `run_to_height` now
   takes `&mut EquivocatorRegistry` and routes every `Emit::Equivocation`
   through it via a new `ConsensusNode::oracle()` getter. This is
   explicitly a **role-only** consequence (Directive 4's identity-root/
   personhood boundary and P2 are both untouched): it does not remove a
   root from the still-static `ValidatorSet` (dynamic validator-set
   transitions are separate, larger, later roadmap work, issues #36-#45),
   and it changes no consensus behavior today — the round driver already
   counts an equivocator's vote at most once, so safety never depended
   on this. What changes is that the evidence is no longer thrown away;
   a future exclusion-from-the-next-epoch or governance-visible-strike
   mechanism now has something real to query (`is_flagged`,
   `flagged_count`) instead of nothing.

**Reason:** both gaps were already correctly identified and described in
this tree's own code comments and test names before the founder review
named them independently — the review's value here was insisting the
documented interim mitigation actually get built rather than staying a
comment. Neither required new cryptography or new protocol design: KEL
freshness composes `Kel::verify`'s existing output (`KeyState::sn`) with
a plain per-SCID high-water mark; the evidence registry composes the
existing `verify_equivocation` with a `HashSet`. Directive 14 (simplicity)
governed both — no bespoke witness protocol, no partial slashing
mechanism invented ahead of the validator-set-transition design it would
actually need to plug into.

**Constitutional impact:** strengthens Directive 4 (two honest nodes
applying the identical freshness pin, or the identical evidence-recording
rule, can never disagree because of either) directly. No
`docs/INVARIANTS.md` row changes: `FreshnessPins` is an interim,
partial mitigation explicitly documented as such, not a new frozen
guarantee; `EquivocatorRegistry` assigns no penalty and mutates no
validator set, so P1/P2 (voice/value wall, one-root-one-vote) are
unaffected. No voice/value edge in either crate.

**Implementation status:** shipped and tested. `did-mini` gained
`FreshnessPins` (6 new unit tests in `freshness.rs`) plus the new
integration test in `recovery.rs` described above.
`mini-consensus` gained `EquivocatorRegistry`/`RecordOutcome` (4 new unit
tests in `consequence.rs`); `run_to_height`'s signature changed (a new
`&mut EquivocatorRegistry` parameter) and its 3 real-socket integration
tests were updated to pass one and assert `flagged_count() == 0` for
every honest run. `cargo fmt --all`, `cargo clippy --all-targets
--all-features --workspace -- -D warnings`, and `cargo test --workspace
--all-features` all pass (114 test binaries, 0 failures).

**Failure point:** `FreshnessPins` is in-memory only as shipped; a
verifier that restarts loses its pins and is exactly as exposed to a
stale-KEL replay as before this decision until it re-observes a fresh
KEL. Persisting pins across restarts is a caller responsibility this
decision does not solve. `EquivocatorRegistry` is likewise in-memory and
per-process; nothing yet persists or gossips flagged roots across nodes
or restarts.

**Required follow-up:** real witness receipts, witness diversity rules,
and gossip-based duplicity proofs (SPEC-01 §7) for the freshness gap's
harder case; a real validator-set-transition mechanism plus a decision on
what role-only consequence (exclusion, governance-visible strike,
economic penalty) `EquivocatorRegistry`'s record should actually trigger
once one exists (roadmap #36-#45); persistence for both registries across
restarts, a founder/governance-policy call on retention and scope.

**Supersedes / superseded by:** none. Implements interim mitigations
this tree's own prior comments (`verify_delegation`'s freshness note,
`net.rs`'s dropped-evidence comment) already called for; does not alter
D-0053's recovery mechanism, D-0204's equivocation-detection mechanism,
or D-0206's transport-confidentiality mechanism.

### D-0089 — Credential taxonomy naming/mapping doc; explicit custody-separation clause; docs-supersession non-finding (founder review P0 items `credential-separation`/`custody-separation`/`docs-supersession`)  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** `Mininet_In_Depth_Review_20260712.md`
(Value 2/Value 8/Value 9); `docs/design/credential-taxonomy.md` (new);
`docs/design/treasury-economic-model.md` §9 (amended); D-0086 (personhood-
honesty naming), D-0059/D-0060 (treasury custody committees), D-0073
(cellular treasury design)

**Decision:** ship `docs/design/credential-taxonomy.md`, a naming/mapping
document that identifies the review's four named claim classes —
`ParticipantCredential`, `HumanEvidence` (deliberately not named a
"credential"), the `RoleCredential` family, and the `ResourceCredential`
family — against mechanisms that already exist and are already tested
(`did_mini::Kel`/`Controller`; `mini_uniqueness::HumanRecord`/
`HumanStatus`; `did_mini::Capabilities`/`BaseDeviceRole` +
`mini_chain::ValidatorSet` + `mini_forge::governance` + `mini_treasury`
signer committees; `mini_storage`/`mini_reward` receipts), and states
plainly that `UniqueHumanCredential` remains entirely unbuilt Phase 2
work. Separately, amend `docs/design/treasury-economic-model.md` §9 to
state explicitly, rather than leave implied, that a bridge-specific
vault's signer committee and the general treasury's signer committee are
always disjoint sets — no individual may hold a seat on both. Also
record, as an honest non-finding, that this repository's own canonical
documents contain no live Cosmos/LoRa/reverse-liability contradiction to
mark historical — the contradictions the review names live entirely in
the founder's externally-held SPEC whitepapers, never committed here.

**Reason:** the review asks for a name-and-separate exercise, not a new
mechanism — every claim class it lists already has a working, tested
answer in this tree; what was missing was a single document a reviewer
(or a future PR-review checklist) can check a code change against before
it lets a role-level finding quietly touch personhood, or a resource
commitment quietly touch governance weight. Custody separation was
already implied by the cellular design's "separated by... custody
committee" language; making it a standalone, unmissable sentence removes
any need to infer it. The docs-supersession sweep is recorded honestly
because fabricating repository content to "resolve" a contradiction that
isn't actually present here would be a worse outcome than reporting the
non-finding.

**Constitutional impact:** Directive 16/P1 (voice/value wall) and the
one-root-one-vote rule (P2) — the taxonomy makes explicit that every
`RoleCredential` sits underneath, never beside, those rules, and calls
out the equivocation-to-personhood link (D-0088's `EquivocatorRegistry`)
as the exact reverse-liability failure (Value 9) this separation exists
to prevent. No `docs/INVARIANTS.md` row changes — this names an existing
boundary, it does not create or weaken one. Custody-separation clause
strengthens the existing cellular-treasury principle (D-0073) without
altering it.

**Implementation status:** docs-only; no crate, type, trait, or function
signature changes. `docs/design/credential-taxonomy.md` is new;
`docs/design/treasury-economic-model.md` gained one paragraph in §9.

**Failure point:** this document has no enforcement mechanism of its
own — nothing fails a build or a review if a future PR ignores the
taxonomy. It is a reference for human/AI reviewers to check against, not
a compile-time or CI-time gate. If that turns out to be insufficient, a
follow-up would need an actual lint/check (e.g. a `check_governance.py`-
style script flagging a `RoleCredential`-typed value flowing into a
personhood- or vote-weight-typed field) — not proposed or built here.

**Required follow-up:** `UniqueHumanCredential` itself (Phase 2,
nullifiers, multi-proposer challenge protocol, adversarial pilot — not
started); the `docs-supersession` item's remaining scope (superseding
language inside the founder's externally-held SPEC documents) is outside
what this repository/session can read or edit and needs either those
documents committed here or the founder handling it directly;
`constitution-registry` and `audit-program` (the two remaining founder-
review P0 items) both need founder scoping/scheduling, not unilateral
agent action.

**Supersedes / superseded by:** none. Amends D-0073's cellular-custody
design by making one already-implied rule explicit; does not alter
D-0086's `HumanStatus` rename or D-0088's `EquivocatorRegistry` scope.

### D-0090 — Canonicalize the seventeen Founder Directives; generate docs/CONSTITUTION_REGISTRY.json (founder review P0 item `constitution-registry`)  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** `Mininet_In_Depth_Review_20260712.md`
§"Number of constitutional principles" and Phase 0 backlog item 1;
`docs/FOUNDER_DIRECTIVES.md`; `docs/CONSTITUTION_REGISTRY.json` (new);
`tools/constitution_registry.py` (new); founder direction (this session,
2026-07-13: "FOUNDER_DIRECTIVES.md's 17 stand as final" / "new standalone
registry file (JSON/YAML), stable IDs per principle")

**Decision:** the review found three different constitutional-principle
counts in play with no single versioned identity — an external SPEC-00
document defining six, a later external "v2" whitepaper/README framing
defining eleven, and this repository's own committed
`docs/FOUNDER_DIRECTIVES.md` defining seventeen. The founder decided
directly: the seventeen committed directives are the one canonical
constitutional principle set going forward; SPEC-00 and the v2 framing
are both superseded as of this decision, wherever they are held (neither
is committed to this repository, so this decision can only state their
supersession here, not edit their content). A new generated file,
`docs/CONSTITUTION_REGISTRY.json`, gives each directive a stable ID
(`FD-01`…`FD-17`) and an exact SHA-256 digest of its own canonical text
block, built by a new script, `tools/constitution_registry.py build`
(with a `check` subcommand that fails if the registry drifts from the
prose it mirrors). `docs/FOUNDER_DIRECTIVES.md` gained a short
"Canonical status (D-0090)" section stating this plainly and pointing to
the registry.

**Reason:** "one machine-readable constitutional source" was the review's
exact ask, and a hand-maintained registry would reintroduce the same
drift problem it exists to solve — the founder's own choice (standalone
generated JSON, stable IDs) keeps `docs/FOUNDER_DIRECTIVES.md` itself the
single source of prose truth while giving tooling and future reviewers
something to check a claim against without re-reading and re-numbering
the document by hand every time.

**Constitutional impact:** Directive 5 (canonical truth is sacred) and
Directive 14 (simplicity/honesty) both apply directly — one canonical
principle set, mechanically kept in sync rather than asserted and left to
drift. No `docs/INVARIANTS.md` row changes: this is a naming/registry
exercise over already-existing directive text, not a new rule, and does
not touch `docs/FOUNDER_DIRECTIVES.md`'s substantive content (only adds a
short status section after "Final Words"/relations material).

**Implementation status:** shipped. `tools/constitution_registry.py`
parses the seventeen `## Directive N — Title` headings, fails loudly if
the count or ordering is ever wrong, and emits one JSON object per
directive (id, number, title, heading, digest, a hand-written one-line
faithful distillation — the digest, not the distillation, is what
actually binds an entry to canonical prose). `check` was run and passes
against the current `docs/FOUNDER_DIRECTIVES.md`.

**Failure point:** the registry is only as current as the last `build`
run — nothing in CI yet calls `constitution_registry.py check`
automatically, so a future edit to `docs/FOUNDER_DIRECTIVES.md` that
forgets to regenerate the registry would leave it silently stale until
someone runs `check` by hand. Wiring `check` into a CI workflow (parallel
to the existing `governance-policy.yml`/`governance-canonical.yml`
baseline checks) is a natural follow-up, not done here to keep this batch
docs/tooling-only and avoid touching `.github/workflows/` in the same PR
as a Tier-F-adjacent document edit.

**Required follow-up:** wire `constitution_registry.py check` into CI;
the `audit-program` P0 item remains explicitly founder-only per founder
direction this session (no further document produced) — external
reviewer engagement, budget, and scheduling stay outside repository
scope.

**Supersedes / superseded by:** supersedes SPEC-00's six-principle
framing and the external v2 whitepaper/README's eleven-principle
framing, to the extent either is still treated as authoritative anywhere
outside this repository; does not alter any directive's substantive text
or any `docs/INVARIANTS.md` row.

### D-0091 — Real mid-transfer TCP-kill resume test; local-network peer discovery over UDP multicast (founder review P1 items "resumable peer-to-peer bootstrap capsule transfer" / "Local-Wi-Fi/mDNS adapter")  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** `Mininet_In_Depth_Review_20260712.md`
P1 backlog items 2/3; `crates/mini-sync/tests/sync_over_tcp.rs`;
`crates/mini-bearer/src/discovery.rs` (new); D-0062 (live TCP bootstrap
demo, #23); `docs/gates/wifi-bearer-test-protocol.md`

**Decision:** with all ten founder-review P0 items closed (D-0086–D-0090),
this picks up the review's P1 backlog ("proves the network exists"),
scoped to what's genuinely buildable and testable in this session without
real hardware (the review's own item 1, Android/Linux BLE, and item 7,
the airplane-mode acceptance suite, both explicitly need real phones and
stay out of scope here; item 6, invitation/peer-exchange discovery, needs
real wire-message design in `mini-net` and is left for its own future
batch). Two items ship:

1. `a_connection_killed_mid_transfer_over_real_tcp_is_safely_resumed_by_
   a_fresh_connection` (`mini-sync/tests/sync_over_tcp.rs`): a
   `KillSwitchBearer` test wrapper answers a real `TcpBearer`'s first N
   `recv` calls normally, then fails every call after that as
   `BearerError::Closed`, landing strictly mid-pull (after 2 of 5
   `Objects` batches carrying 300 seeded objects). The killed attempt is
   asserted to leave the receiving store at exactly zero new objects
   (`pull()`'s ingest only runs after its whole want-round completes,
   so a mid-stream kill discards that attempt's bytes wholesale rather
   than partially corrupting the store); a second, genuinely fresh TCP
   connection is then proven to converge both stores completely. This
   closes the real gap in the crate's existing coverage: the only prior
   resume test, `sync.rs`'s `interrupted_sync_resumes_by_idempotence`,
   interrupts *before* any content crosses the wire at all.
2. `mini_bearer::{LocalAnnouncer, LocalScanner}` (`discovery.rs`, new): a
   minimal, Mininet-owned announce/query datagram over UDP multicast —
   explicitly **not** full mDNS/DNS-SD (RFC 6762/6763), just enough to
   find another peer's bearer address on the same local network with no
   central server and no prior coordination, then hand that address
   straight to `TcpBearer::connect`. Carries no identity, matching this
   crate's existing "Bearer is a dumb, identity-free pipe" design. Only
   the `LocalScanner` side binds the rendezvous port (the announcer just
   sends), so no `SO_REUSEADDR`/raw-socket tricks are needed and the
   crate's `#![forbid(unsafe_code)]` is untouched.

**Reason:** both items are precisely the kind of real, scoped,
demonstrable-without-hardware engineering the review's P1 list calls for,
and both extend already-shipped, already-tested code (`mini-sync`'s TCP
tests, `mini-bearer`'s bearer trait) rather than opening new architecture.
Picking two closely related "proves the network exists" items and
shipping them together in one batch follows the standing instruction to
batch related work into fewer, larger PRs rather than many small ones.

**Constitutional impact:** Directive 6 (design for failure, not success)
directly — both changes are about proving the network keeps working
correctly when a connection dies, exactly the class of failure Directive
6 names. No `docs/INVARIANTS.md` row changes: this is coverage/capability
work over already-decided mechanisms (`mini-sync`'s pull protocol,
`mini-bearer`'s `Bearer` trait), not a new rule. `docs/gates/
wifi-bearer-test-protocol.md`'s W1-W7 hardware-trust-scoring gate is
explicitly **not** closed by this decision — this only builds the
discovery mechanism the gate would go on to test; no trust-weight claim
is made here.

**Implementation status:** shipped and tested.
`cargo test --workspace --all-features` passes; the new
`sync_over_tcp.rs` test and all four new `discovery.rs` tests
(`a_scanner_discovers_an_announcer_on_the_same_local_network`,
`a_scanner_times_out_cleanly_when_nobody_announces`,
`discovery_hands_off_a_usable_address_to_a_real_tcp_bearer_connect`,
`a_foreign_datagram_on_the_same_group_is_ignored_not_mistaken_for_a_peer`)
were each run 5 times in isolation to rule out flakiness from real
socket/thread timing, with no failures.

**Failure point:** the discovery datagram has no replay/rate protection
and the multicast group is not a security boundary (documented explicitly
in `LocalScanner::recv_timeout`'s doc comment) — a hostile local-network
peer can flood announce traffic or spoof a source address; this is
acceptable because discovery only ever hands off to `TcpBearer::connect`,
which starts an ordinary anonymous `Channel` handshake carrying no trust
of its own, so a forged announce at worst wastes a connection attempt,
never grants any authority. The kill-switch test's timing (`remaining: 5`
sized against the crate's current 64-object/batch and 300-object seed
count) would need adjusting if `protocol.rs`'s internal batch size ever
changes.

**Required follow-up:** invitation/peer-exchange discovery (review P1
item 6) — needs real wire-message design in `mini-net`, not attempted
here; wiring `LocalAnnouncer`/`LocalScanner` into an actual bootstrap or
reference-client flow (still design-only today, this is the primitive,
not the integration); `docs/gates/wifi-bearer-test-protocol.md`'s W1-W7
hardware validation remains entirely founder/tester work needing real
routers and phones.

**Supersedes / superseded by:** none. Extends D-0062's live-TCP-bootstrap
proof and `mini-bearer`'s existing `Bearer` trait design; does not alter
`mini-sync`'s wire protocol or `mini-bearer`'s `TcpBearer`/`Channel`
behavior.

### D-0092 — Peer exchange (PEX): mini-net's first real wire-message design, over real TCP (founder review P1 item "invitation and peer-exchange discovery with no required central server")  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** `Mininet_In_Depth_Review_20260712.md`
P1 backlog item 6; `crates/mini-net/src/pex.rs` (new);
`crates/mini-net/tests/pex_over_tcp.rs` (new); D-0091 (deferred this item
as needing "real wire-message design, a larger lift than the local-
network case"); roadmap #36-#45

**Decision:** with the two smaller founder-review P1 items shipped
(D-0091), this closes the third and largest: a node can now discover a
peer's dialable address purely by asking an already-connected peer, with
no central directory server. `mini_net::pex` adds: `PeerRecord` (a
`PeerId` paired with a `SocketAddr` — `RoutingTable` alone was never
dialable, only positional); `AddressBook` (maps `PeerId` → `SocketAddr`,
first-seen-wins on insert); `PexMessage::{Request(PeerId), Response(Vec<
PeerRecord>)}` with a hand-rolled binary encode/decode (this crate's
first real wire-message design — `RoutingTable`/`GossipRouter` had no
message-type or wire-format concept before this); and two pure functions,
`build_response`/`absorb_response`, over `RoutingTable`/`AddressBook`.
`Request` carries the requester's own id but never a self-declared
address — the responder learns the requester's dialable address from the
live connection's own observed source address instead, closing one class
of return-address spoofing a self-reported address would invite.

**Reason:** D-0091 explicitly named this the largest remaining P1 lift
("needs real wire-message design in `mini-net`") and deferred it rather
than rush a design; this batch does that design properly, scoped to
exactly what "peers can find each other's addresses with no central
server" requires — nothing more (no gossip-fanout integration, no
routing-table liveness refresh, both named as follow-up, not attempted
here). Keeping `PexMessage`'s wire format hand-rolled (no serde) and the
handler functions pure (no socket I/O inside `pex.rs` itself) matches this
crate's existing "land pure, testable logic before the adapter that needs
a real socket" pattern and this workspace's broader no-new-dependency
default.

**Constitutional impact:** none directly — `PeerId` remains explicitly
non-identity (unaffected), and `PexMessage::Response` is documented as an
unauthenticated hint, never a trust or governance signal, so this
introduces no new voice/value-adjacent surface. Directive 6 (design for
failure) applies to the `AddressBook::insert` first-seen-wins choice: a
later hostile PEX response can never silently redirect who a caller
dials for an id already resolved. No `docs/INVARIANTS.md` row changes.

**Implementation status:** shipped and tested. 12 unit tests in
`pex.rs` (encode/decode round-trips, truncation/trailing-byte/oversized-
count rejection, `AddressBook` first-seen-wins, `build_response`/
`absorb_response` exclusion and never-learn-self behavior) plus two real-
TCP integration tests in `tests/pex_over_tcp.rs`:
`a_node_discovers_a_second_peers_address_purely_through_pex_over_real_tcp`
(node A, supplied only node B's address, discovers node C's dialable
address purely through one PEX round with B, then actually dials C over
a fresh socket to prove the discovered address is real, not just a data-
structure entry) and
`pex_never_hands_the_requester_back_its_own_record_over_real_tcp`. Both
new integration tests were run 5 times in isolation with no failures.
`cargo test --workspace --all-features` is green (one unrelated,
pre-existing flake in `mini-build-runner-wasmtime`'s adversarial suite
reproduced once and passed clean on immediate retry — not touched by
this change, not investigated further here).

**Failure point:** `PexMessage::Response`'s trust model is deliberately
thin — an unauthenticated hint whose only real defenses are the response-
size cap (`MAX_PEX_RECORDS`) and `AddressBook`'s first-seen-wins rule;
nothing here proves a discovered address is live, honest, or actually the
peer it claims. `RoutingTable::insert`'s own existing honest limit
(a full bucket simply refuses new peers, no liveness-based eviction) is
unchanged and still applies to peers learned via PEX. Wiring PEX into an
actual bootstrap flow, gossip fanout, or a periodic refresh loop is not
done here — this ships the mechanism, not the integration.

**Required follow-up:** wire PEX-discovered peers into `GossipRouter`
fanout and `RoutingTable` bucket refresh end to end (currently proven
independently, not together); a real address-liveness check before
trusting a PEX-learned address for anything beyond "worth dialing";
BLE/Android adapter and the airplane-mode acceptance suite (review P1
items 1/7) remain hardware-blocked; a minimal reference client (review
P1 item 4) and state sync/reconnect (review P1 item 5) remain unbuilt.

**Supersedes / superseded by:** none. Extends `mini-net`'s existing
`RoutingTable`/`GossipRouter` design (D-0034/D-0042) with the address-
carrying piece neither previously provided; does not alter either type's
existing behavior or tests.

### D-0093 — Consensus state sync / catch-up over real TCP  ·  *Accepted*
**Date:** 2026-07-13 · **Refs:** `docs/STATUS.md` §1 / `docs/design/
networked-consensus.md` (both named this gap); `crates/mini-consensus/
src/catchup.rs` (new); `crates/mini-consensus/tests/networked_consensus.rs`

**Decision:** closes the consensus crate's own named next-slice gap —
"no state-sync for a node that missed a whole height." New
`mini_consensus::{CatchupRequest, CatchupResponse, FinalizedBlock}` (a
hand-rolled wire codec, reusing `wire.rs`'s existing header/body encoders
via `pub(crate)`) plus `ConsensusNode::{history_since, catch_up}`: a node
records every block it finalizes, serves a bounded slice of that history
to a lagging peer, and the lagging peer applies each block through the
*same* `LedgerChain::apply_finalized_block` call live consensus uses —
independently re-verifying every quorum certificate, never trusting the
serving peer directly.

**Reason:** the smallest real (non-stub) increment matching what was
already named as missing, reusing existing wire-codec conventions rather
than inventing new ones.

**Constitutional impact:** none new — reuses `mini_chain::verify_finality`
unchanged; a catching-up node gets no different trust guarantee than a
live validator gets at commit time.

**Implementation status:** shipped and tested. 6 new unit tests in
`catchup.rs` (round-trip, truncation, oversized-count/vote-count
rejection) plus `a_late_joining_node_catches_up_via_real_tcp_and_matches_
the_clusters_state`: a fifth node, never a validator-set member and never
running a single Tendermint round, pulls finalized history from a live
node over a real TCP socket and reaches the exact state the four-node
cluster converged on. Also added `net::{catch_up_over_tcp,
serve_catch_up_over_tcp}` — first-class transport functions (not just a
test hand-rolling the exchange), reusing the same `Channel` handshake
every other consensus link uses so catch-up traffic is encrypted like
everything else, proven by a second real-TCP test using only the public
`net` API. Full workspace suite green.

**Failure point:** history is unbounded in-memory on the serving node —
no pruning/persistence (documented honest first-slice limit, same shape
as `mini-net`'s `RoutingTable` bucket limit). No peer-selection/retry
policy — a caller picks one peer and lives with what it returns.

**Required follow-up:** history pruning/persistence; peer selection;
folding `catch_up_over_tcp` into `run_to_height` itself so a single call
can catch up and then join live rounds.

**Supersedes / superseded by:** none.

### D-0094 — Adopt founder research V2 (cost doctrine) and its parallel-contributor phase sequencing; ship `mini-privacy-policy` as the first P1 code slice  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** `docs/research/MININET_RESEARCH_V2_20260713.md`,
`docs/research/PARALLEL_CONTRIBUTOR_PROGRAM_20260713.md` (founder-supplied,
uploaded 2026-07-14); `crates/mini-privacy-policy` (new)

**Decision:** adopt the founder's "cost doctrine" research (every privacy/
availability/integrity property is a named, priced purchase; five residual
floors F1-F5 are never removed by any spend) as forward direction, and
adopt the accompanying parallel-contributor package's phase sequencing
(`P0`-`P8`, summarized in the second ref above) as the roadmap reference
for privacy/distribution/human-evidence work going forward. Per both
documents' own stated authority order (founder research → Founder
Directives/frozen invariants → Decision Log/Failure Book → STATUS →
existing roadmap → the package's own decomposition), this decision sits
below `docs/FOUNDER_DIRECTIVES.md`/`docs/INVARIANTS.md` and changes
sequencing only — no invariant is touched. Ships the phase P1 items
`MN-101` (protection-property/resource-cost vocabulary) and `MN-102`
(privacy tier policy object) together as a new crate,
`mini-privacy-policy`: `ProtectionProperty`/`Mechanism`/`ResidualFloor`
vocabulary types (`vocabulary.rs`), `PrivacyTier` (Direct/Relayed/Mixed/
Burst), `ResourceCost` (fixed-point min/max ranges — no float, matching
the research's own honesty that these are ranges, not point estimates),
`expected_cost(tier)` reproducing the research's own §2 cost-curve
estimates, and `PrivacyRequest`/`AchievedPrivacy` with a hand-rolled wire
codec (`tier.rs`). `AchievedPrivacy::new` always attaches all five
`RESIDUAL_FLOORS`, by construction, so no caller can build a result that
silently omits one.

**Reason:** the research's own request was explicit ("bring it closer to
delivering stage 1"), and per standing founder feedback this session
prioritizes shipping real, tested code over further planning documents.
`MN-101`/`MN-102` were chosen as the first slice because they are the
package's own P1 entry point (`MN-103`/`MN-104` are declared blocked on
`MN-101`) and because a pure, dependency-free vocabulary+policy crate is
reviewable in one sitting and touches no existing crate's internals —
lower risk than starting directly on `ObjectEnvelope` v2 or a relay
protocol. The package's suggested "Rust serde types" technology was not
followed: this workspace has never taken a serde dependency (`grep -r
serde` across every `Cargo.toml` is empty) and instead hand-rolls a
domain-tagged, length-prefixed wire codec with truncation/trailing-byte/
oversized-count rejection tests everywhere a wire format exists
(`mini-net::pex`, `mini-consensus::catchup`, ...); matching that existing,
already-reviewed convention was judged higher priority than the package's
generic suggestion, and is an implementation-technology call, not a
constitutional one.

**Constitutional impact:** none. `mini-privacy-policy` is pure data plus a
codec — no crypto is implemented (Directive 14/no-new-cryptography rule:
the crate does not touch key material, AEAD, or any primitive at all yet),
no governance or value surface is touched (voice/value wall unaffected —
the new crate has zero dependencies), and every doc comment on
`HumanUniquenessSignal`/`GlobalUniquenessOfPersons` explicitly defers to
the existing Sybil-unsolved limitation in `docs/INVARIANTS.md` rather than
claiming anything new about personhood. The vocabulary's naming
deliberately does not reuse or collide with `mini_uniqueness::HumanStatus`/
`EvidenceQualifiedHuman` (D-0086); reconciling the research's own
`Unassessed → ... → ExternalUniquenessBacked` confidence-class language
with that existing shipped taxonomy is explicitly deferred (see Required
follow-up), not silently merged or renamed here.

**Implementation status:** shipped and tested. 22 unit tests in
`mini-privacy-policy` (byte round-trips for all 13 `ProtectionProperty`/17
`Mechanism`/5 `ResidualFloor` variants, unknown-byte rejection for each,
wire round-trips for `PrivacyRequest`/`AchievedPrivacy` including the
empty-list case, truncation-at-every-length rejection, trailing-byte
rejection, wrong-domain rejection, over-cap count rejection before
allocating for properties/mechanisms/floors, `AchievedPrivacy::new`'s
always-five-floors invariant, and a monotonic-cost-by-tier sanity check).
`cargo fmt`, `cargo clippy -p mini-privacy-policy --all-targets
--all-features -- -D warnings`, and `cargo test -p mini-privacy-policy`
are clean. **No transport, relay, mix, or storage mechanism is
implemented anywhere in this batch** — `expected_cost` reproduces the
research document's own estimates, not a measurement of running code; the
crate-level doc comment says this explicitly, matching this session's
"snapshot honesty" discipline.

**Failure point:** this is policy vocabulary with nothing yet enforcing
it — nothing in this workspace today reads a `PrivacyRequest` and actually
routes traffic differently, so an `AchievedPrivacy` value is only as
honest as whatever future code constructs it; there is no mechanism here
preventing a careless caller from claiming a tier it did not actually
reach. `ProtectionProperty`/`Mechanism` are marked `#[non_exhaustive]` to
allow growth, which means any downstream `match` must already handle
unknown variants — a deliberate forward-compatibility choice, not an
oversight.

**Required follow-up:** `MN-103` (`ObjectEnvelope` v2 private-metadata
boundary) and `MN-104` (capability rights/scoped pseudonym primitives),
the package's own next P1 items; reconciling the research's Human
Evidence Credential confidence-class naming against
`mini_uniqueness::HumanStatus`/`EvidenceQualifiedHuman` before any P4 work
starts; a decision on whether/when to publish the package's ~70 `MN-xxx`
items as real GitHub issues (deliberately not done this batch); wiring an
actual `TransportRequest` router (`MN-201`, P2) once a real relay exists
to consume this crate's types for something other than logging.

**Supersedes / superseded by:** none. New crate, no existing type or
behavior changed.

### D-0300 — Parallel execution plan for the privacy/cost-doctrine track: disjoint-footprint lanes, batched PRs, new `D-03xx` band  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** `docs/design/
privacy-cost-doctrine-parallel-execution-plan.md` (new); D-0094; founder
direction ("sequence PRs correctly, push as much work in 1 PR to main,
have several devs working in parallel")

**Decision:** opens the `D-03xx` decision-number band (this repo's third
track, after the main `D-00xx` line and the networking/consensus `D-02xx`
band — both already anticipated by the allocation policy's "if a third
track appears" clause) for the privacy/cost-doctrine work D-0094 adopted,
and publishes a **lane plan**: five first-wave work groupings (L1-L5),
chosen so no two lanes touch the same crate, each sized to batch multiple
`MN-xxx` items from `docs/research/PARALLEL_CONTRIBUTOR_PROGRAM_20260713.md`
into one PR rather than one PR per item. L1 (ObjectEnvelope v2 +
capability/pseudonym primitives, `mini-objects`/`did-mini`), L2
(`TransportRequest` router, new `mini-transport-policy` crate), L3
(Sphinx mix research, docs-only), L4 (resource pricing, new
`mini-resource-pricing` crate), and L5 (human-evidence taxonomy
reconciliation, `mini-uniqueness` only, flagged higher-scrutiny) are all
unblocked today — their only dependency, `MN-101`/`MN-102`, shipped in
D-0094.

**Reason:** direct founder request to make PR sequencing explicit and
enable several developers (human or AI) to work the privacy/cost-doctrine
backlog concurrently without colliding, while batching more work per PR
rather than fragmenting into ~70 single-item PRs. Disjoint file footprint
per lane is the concrete mechanism that makes "several devs in parallel"
actually collision-free rather than aspirational — two lanes touching
different crates can be reviewed and merged in either order with zero
merge conflict between them, unlike a flat issue backlog where any two
issues might land on the same file. A new hundreds-band was chosen over
reusing `D-02xx` or the main line because this track will itself run
multiple lanes concurrently and needs the same collision-avoidance
property `D-02xx` already gave the networking track relative to the main
line — see the updated allocation-policy section for the added
within-band collision rule this track specifically needs (renumber on
merge, don't pre-reserve).

**Constitutional impact:** none — this is pure process/coordination
scaffolding (a document and a decision-numbering rule), no code, no
crypto, no governance surface. L5 is flagged for extra scrutiny
specifically *because* it could create constitutional risk (a rival
personhood taxonomy) if not scoped carefully — the lane definition itself
constrains it to reconciliation only, not new claims, precisely to avoid
that risk rather than create it.

**Implementation status:** planning artifact only — no lane has started.
The lane table, footprints, and blocked-by status in the referenced
design doc are accurate as of this entry against the current repository
state (post-D-0094).

**Failure point:** the plan's collision-freedom claim holds only as long
as lane scope is respected — if a lane's PR reaches outside its declared
footprint (e.g. L2's router PR also touching `mini-objects`), the
disjointness property breaks and the next lane to merge pays the conflict
cost this plan exists to avoid. Nothing enforces footprint boundaries
mechanically today; it is a documented convention, not a CI gate.

**Required follow-up:** wave-2 lane definitions once L1/L2/L3 land and
their real public types are known (the design doc names candidates —
`L6` relay/rendezvous — but deliberately does not freeze its footprint
yet); consider a lightweight CI check that flags a PR touching files
outside its lane's declared crate list, if this scales past founder
review capacity; actually claiming/starting L1-L5 is separate follow-up
work, not done in this entry.

**Supersedes / superseded by:** none. Extends the `docs/DECISION_LOG.md`
allocation-policy section (edited in place, in accordance with that
section's own "if a third track appears, add it here" instruction — this
is the one place in this append-only log where amending existing
top-of-file policy text, not a dated entry, is the correct move).

### D-0097 — Generic Tor Pluggable Transport v1 process manager: managed-subprocess safety boundary for `mini-bridge`, no real PT dependency (post-`MN-207`)  ·  *Accepted*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md`; `docs/design/
external-bridge-adapter-integration.md` (new); D-0309 (`mini-bridge`,
which this extends); CLAUDE.md's no-new-cryptography and no-shell rules

**Decision:** adds `mini-bridge::pt_process`, implementing exactly the
research report's own recommended PR2 scope (§24): a generic Tor
Pluggable Transport v1 managed-subprocess process manager proving the
safety boundary a real circumvention adapter (Lyrebird/WebTunnel/
Snowflake) will later dial through, with zero real PT binary as a
dependency. `VerifiedExecutable` requires an absolute pinned path and a
BLAKE3 digest checked immediately before every launch — a binary that
changed since approval is refused, not executed. `PtProcessManager::
launch` spawns via `std::process::Command::new` only (never a shell, by
construction — no code path assembles a command-line string for an
interpreter), with `env_clear()` plus exactly the three PT v1 variables
the protocol requires (`TOR_PT_MANAGED_TRANSPORT_VER`,
`TOR_PT_CLIENTTRANSPORTS`, `TOR_PT_STATE_LOCATION`). Because a raw
blocking `BufRead::lines()` loop cannot be preempted by a wall-clock
deadline check between reads, the stdout handshake is read on a
background thread over an `mpsc::channel`, letting the calling thread
bound the whole startup handshake with `recv_timeout` against an
absolute `Instant` deadline — a hung or slow child is killed and
reported as `BridgeError::Timeout`, not waited on forever.
`PtProcessHandle::terminate` kills then `wait()`s the child, so success
is OS-confirmed exit. Proven end to end against a real compiled fake-PT
fixture binary (`src/bin/fake_pt_fixture.rs`) in a new integration test
file, `tests/pt_process_fixture.rs` (three tests: digest-mismatch
refusal, a full valid handshake, and termination). Adds seven new
`BridgeError` variants (reusing the existing enum per Directive 14,
rather than inventing a parallel failure-taxonomy type). Zero new
external dependencies — only `std` plus already-in-tree `mini-crypto`
for the digest check.

**Reason:** the research report's own executive conclusion is that the
strongest first PR is the generic managed PT process adapter with a
fake conformance child, proving the safety boundary before a real
circumvention binary becomes part of the release — not starting
directly on a real Lyrebird/obfs4 integration, which needs an audited
external implementation this workspace would compose, not invent, and
which is explicitly deferred to a future PR (§24 PR3+). Scoping this PR
to zero new external dependencies was a deliberate choice matching this
session's established external-dependency check-in precedent (adding a
new crate dependency to security-relevant code requires an explicit,
separate confirmation step) — nothing here needed one.

**Constitutional impact:** none. No dependency changes at all beyond
`mini-crypto` (already in-tree, path dep only, verify-only digest use —
no signing, no key material). Directive 14 (no new cryptography) is
reinforced: `VerifiedExecutable` composes `mini-crypto`'s existing
BLAKE3 hashing, nothing here invents a primitive. The trust boundary is
explicit and documented (`docs/design/
external-bridge-adapter-integration.md`): a managed PT subprocess is
trusted only to transform bytes and report a local endpoint — never to
authenticate the Mininet bridge, choose policy, or touch identity/
capability/governance state. Every PT connection is designed to
terminate into a separate, independently authenticated
`mini_bearer::Channel` handshake in a future PR, mirroring what
`DirectBridgeTransport` already does — this PR does not perform that
handshake itself.

**Implementation status:** shipped and tested. `cargo fmt`, `cargo
clippy --all-targets --all-features --workspace -- -D warnings`, and
`cargo test --workspace --all-features` are clean (full workspace, not
just `mini-bridge` in isolation).

**Failure point:** this proves the *process-management* safety boundary
only. No `PluggableTransport` implementation exists yet for any real
transport (Lyrebird/obfs4, WebTunnel, Snowflake, Tor), so nothing here
is usable for real circumvention today. No sandboxing beyond whatever
the host OS's own process isolation provides — a malicious or
compromised (but digest-matching, i.e. supply-chain-compromised at the
source) PT binary could still consume unbounded resources or attempt to
attack the parent process directly; no `ExternalAdapterManifest` or
binary-provenance/download tooling exists yet, so approving a real PT
binary's digest remains a fully manual, out-of-band step.

**Required follow-up:** a real `PluggableTransport` implementation
dialing through a launched `PtProcessHandle`'s local endpoint plus the
deferred inner `mini_bearer::Channel` handshake (Lyrebird/obfs4 first,
per the research report's own priority ordering, then WebTunnel); a Tor
SOCKS bearer kept isolated per the report's recommendation; Snowflake
via Tor's own PT management; an `ExternalAdapterManifest`/provenance
mechanism for approving and updating pinned binary digests; a platform-
packaging study (§ per report). All explicitly deferred, not started in
this PR.

**Supersedes / superseded by:** none. Extends D-0309's `mini-bridge`
additively (new module, new public types, new `BridgeError` variants);
no existing type's behavior changed.

### D-0301 — TransportRequest policy router: new `mini-transport-policy` crate, no transport wired (lane L1, `MN-201`, closes tracking issue #134)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan); D-0094
(`mini-privacy-policy`, the dependency this lane consumes);
`crates/mini-transport-policy` (new); tracking issue #134

**Decision:** ships `MN-201` as a new, dependency-light crate:
`TransportRequest` (a `mini_privacy_policy::PrivacyRequest` plus a
`PayloadSizeClass`), `route(&TransportRequest) -> Result<RouteDecision>`,
and two fixed policy tables — `mechanisms_for_tier` (what each tier
provides, monotonically increasing: `Burst` is `Mixed`'s set plus erasure
replication) and `property_min_tier` (the minimum tier at which each
`ProtectionProperty` becomes achievable, a judgment call recorded here,
not a proof). `route` fails closed with
`TransportPolicyError::UnsatisfiableProperty` when a request asks for a
property the requested tier can't provide, rather than silently routing
at the (cheaper) requested tier anyway.

**Reason:** the founder's follow-up direction ("pick a child issue,
produce code across several PRs") pointed at the lane plan D-0300 just
published; `L2` was picked first because it is fully additive (a brand
new crate, zero existing-code risk) and lowest-complexity among the
unblocked lanes, letting the parallel-lane pattern get proven with a real
PR quickly. `property_min_tier`'s conservative default for any future
`#[non_exhaustive]` `ProtectionProperty` variant this crate doesn't yet
map (routes it to the highest tier, `Burst`) follows the same
"unearned-confidence forbidden" discipline `mini-privacy-policy` itself
already applies — claiming a lower tier suffices for an unmapped property
would be exactly the kind of overclaim the cost doctrine exists to
prevent.

**Constitutional impact:** none. Pure routing-decision data and logic —
no crypto, no socket, no governance/value surface (the crate's only
dependency is `mini-privacy-policy`, itself dependency-free).

**Implementation status:** shipped and tested. 9 unit tests (tier-by-tier
mechanism coverage, the fails-closed case for both an under-provisioned
Direct-tier request and an under-provisioned Mixed-tier request against
`SuppressionResistance`, an over-provisioned request still routing
successfully, the tier-mechanism-list-is-a-superset-of-the-tier-below
monotonicity check, and payload size class passing through unchanged).
`cargo fmt`, `cargo clippy -p mini-transport-policy --all-targets
--all-features -- -D warnings`, and `cargo test -p mini-transport-policy`
are clean.

**Failure point:** `property_min_tier`'s mapping is a judgment call with
no formal backing yet — it is what this crate believes, not what any
mechanism has been proven to deliver, since no mechanism above `Tier 0`
(`AeadEncryption`) is actually implemented anywhere in this workspace. If
a future relay/mix implementation turns out weaker than this mapping
assumes, `route`'s `Ok` results would be over-claiming until the mapping
is corrected — this is the same "declared, not measured" honesty
constraint `mini-privacy-policy` already documents, inherited here.

**Required follow-up:** `MN-202` (Tier 1 relay/rendezvous protocol,
lane L6 per D-0300, blocked on this lane plus L1's capability
primitives) is the first mechanism this router's output could actually
drive; until then `RouteDecision` has no consumer beyond its own tests.

**Supersedes / superseded by:** none. New crate, no existing type or
behavior changed.

### D-0304 — ObjectEnvelope v2 private-metadata boundary + typed capability grants + scoped pseudonyms (lane L1, `MN-103`/`MN-104`, closes tracking issue #133)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan); D-0094
(`mini-privacy-policy`, the research this lane's cost-doctrine vocabulary
comes from); `crates/mini-objects/src/{envelope_v2,private_object,
capability,pseudonym}.rs` (new); `crates/mini-objects/src/{codec,error,
object}.rs` (extended); tracking issue #133

**Decision:** ships `MN-103` and `MN-104` together, per a detailed
research report concluding that a v1 `Object`'s cleartext fields (type,
author root, author device, timestamp, sequence, links, signer identity)
reconstruct a detailed behavioral/social graph even when `Payload` alone
is encrypted, and that the correct v2 boundary is architectural — an
opaque outer envelope, not a v1 object with more fields individually
marked sensitive.

`ObjectEnvelopeV2` (`envelope_v2.rs`): a public outer container carrying
only a version byte (`2`, distinct from v1's `1` — the entire
disambiguation mechanism, no separate magic bytes needed since neither
format has other ambiguous framing), an `AeadSuite` tag, a 32-byte random
`OpaqueRoute` (no semantic meaning, no deterministic HMAC-style derivation
in this version — the research's own fallback guidance was to generate
random tags rather than invent a new construction), a coarse
`RetentionClass` (`Ephemeral`/`Standard`/`Archival`, deliberately not an
exact application expiry, which could fingerprint content class the same
way exact payload length can), a nonce, and ciphertext.
`ObjectEnvelopeV2::seal`/`open` encrypt/decrypt a `PrivateObject` under an
already-established `mini_crypto::AeadKey` (key distribution is
explicitly out of scope, matching how `mini_bearer::Channel` also accepts
an already-agreed key). Every public field is bound as AEAD associated
data, so tampering with routing/suite/retention breaks decryption. A
fresh random nonce per seal means identical private objects sealed twice
produce different ciphertext and different envelope ids — closing a
confirmation-attack path a deterministic scheme would open. Content id is
computed via `ObjectId::of` (relaxed from private to `pub(crate)` in
`object.rs` so v2 reuses the exact same BLAKE3-multihash-base58btc recipe
rather than a second copy of it) over the full canonical outer bytes,
matching v1's own content-addressing convention exactly.

`PrivateObject` (`private_object.rs`): the decrypted inner form —
`object_type`, `author_human`, `author_device`, `timestamp_ms`,
`sequence`, `links`, `application_metadata`, `payload`, and signatures —
i.e. everything v1 exposes in cleartext. Signed via a typed, domain-
prefixed (`mininet/mini-objects/private-object/v1`) `signing_bytes()`
handed to `Controller::sign_message`, never a generic `sign(bytes)` call
on caller-assembled data (Directive/CLAUDE.md typed-domain rule).

`CapabilityRight`/`CapabilityScope`/`CapabilityGrant`/`CapabilityToken`
(`capability.rs`): five independent, closed rights (`Read`/`Append`/
`Reply`/`Moderate`/`Administer` — no implicit hierarchy, `Administer`
does not imply `Read`), an initially-minimal `#[non_exhaustive]` scope
(`Object(ObjectId)` only — `Collection`/`Conversation`/`Community` named
as future work, not guessed at), and exact, non-delegable,
holder-bound grants: `CapabilityGrant::validate` checks the issuer's
signature, exact scope match, exact right match, a token-commitment
match (`CapabilityToken::commit` domain-separates by scope *and* right,
so a commitment copied into a different grant never matches), the
validity window, and a holder-proof signature from the grantee's current
keys over a grant-bound (nonce + commitment), domain-separated message —
so a leaked grant (public, signed data) alone is insufficient, and so is
a leaked token alone without the grantee's signing key. No wildcard/
prefix scope, no attenuation/delegation, no Macaroon/Biscuit-style policy
language — the research explicitly evaluated and rejected those as
larger audit surface than this lane's five-right requirement needs.

`derive_scoped_pseudonym` (`pseudonym.rs`): a thin, domain-separated
wrapper over `did-mini`'s **already-existing** SPEC-01 §10
`Controller::incept_pairwise_pseudonym` (HKDF-SHA256 over the root's own
current-key seed) — no second HKDF call site, no new derivation
construction. `PseudonymPurpose` (`ObjectAuthor`/`CapabilityHolder`) is
folded into the HKDF `info` context alongside the caller's scope id, so
the same root's object-authorship pseudonym and capability-holder
pseudonym in the same scope are cryptographically unrelated — a
capability-holder pseudonym cannot double as a public authorship handle.

**Reason:** third and largest lane the founder picked from D-0300's plan
this batch, chosen because it is the most valuable and most foundational
(unblocks `MN-202`/`MN-208` per D-0300's own "sequencing after wave 1"
section) despite being the one lane touching existing crates. Scope was
deliberately bounded to exactly what the research's own PR-stage-1/2/3/4
sequencing named as fitting one reviewable PR (wire boundary + seal/open
+ scoped pseudonyms + exact capabilities), explicitly deferring HPKE
recipient encryption, MLS group key state, capability attenuation/
delegation, and deterministic route-tag derivation as separately-scoped
future work — each named in "Required follow-up" below, not silently
dropped.

**Constitutional impact:** strengthens the honesty discipline this
workspace already applies elsewhere: `PrivateObject`'s doc comments and
this entry state plainly that key distribution, traffic-analysis
resistance, and route-tag-reuse correlation are **not** solved here —
"V2 prevents private application metadata from being required in the
public object schema; it does not provide complete traffic-analysis
resistance" (research report's own framing, restated in `envelope_v2.rs`'s
module docs). No crypto primitive was invented — this composes
`mini-crypto`'s existing AEAD (ChaCha20-Poly1305), HKDF-SHA256, BLAKE3,
and Ed25519 signing exactly as already reviewed elsewhere in the
workspace (Directive 14/no-new-cryptography rule). No dependency edge to
`mini-forge`/`mini-chain` (voice/value wall unaffected — this crate's
`Cargo.toml` gained only `zeroize`, matching `did-mini`/`mini-crypto`/
`mini-treasury`'s existing `CapabilityToken`-secret-scrubbing pattern, no
new external dependency to the workspace as a whole).

**Implementation status:** shipped and tested. 39 new unit tests across
`envelope_v2.rs` (16: seal/open round-trip, wire round-trip, v1-rejected-
by-v2 and v2-rejected-by-v1 disambiguation, route/retention/ciphertext-
tamper-breaks-decryption ×3, wrong-key-fails, fresh-nonce-produces-
different-ciphertext-and-ids, a byte-scan confirming none of the private
fields appear anywhere in the outer bytes, truncation-at-every-length,
trailing-bytes, unknown-suite, unknown-retention, over-cap-ciphertext-
before-allocating), `private_object.rs` (9: round-trip, signature
verifies/fails-on-tamper/fails-against-wrong-KEL, truncation, trailing-
bytes, over-cap-links-before-allocating, unsigned-has-no-signatures,
custom-type round-trip), `capability.rs` (21: every right round-trips,
unknown right rejected, full valid-grant-and-proof success path, right-
mismatch fails ×2, scope-mismatch fails, a token-commitment-copied-into-
a-different-scope-grant fails, wrong-token fails, missing/wrong holder
proof fails ×2, expired/not-yet-valid fail, wrong-issuer fails, wire
round-trip ×2 incl. a validity-window round-trip that still enforces the
window post-decode, unknown-version rejected, truncation, trailing-
bytes), and `pseudonym.rs` (5: same-inputs-same-pseudonym, different-
scope/purpose/root all produce different pseudonyms, a derived pseudonym
can sign and independently verify). `cargo fmt`, `cargo clippy
--all-targets --all-features --workspace -- -D warnings`, and
`cargo test --workspace --all-features` are clean; all 9 pre-existing v1
integration tests in `tests/objects.rs` still pass unmodified — v1's own
wire format and behavior are untouched.

**Failure point:** `OpaqueRoute`'s randomness means a caller has no way
to compute a route tag without being told it out of band — deterministic,
scope-derived routing (so an authorized reader can find an object without
a side channel) is explicitly deferred, so this version alone does not
yet support "find this object by scope" lookups, only "open this object
I was already pointed at." `CapabilityGrant` has no revocation mechanism
— an issued grant remains valid (subject to its own expiry) until it
naturally expires; a compromised token or grantee key cannot be
invalidated early in this version. `RetentionClass` is advisory metadata
a storage node could ignore; nothing here enforces it.

**Required follow-up:** deterministic/scope-derived `OpaqueRoute`
generation; a capability revocation mechanism; `MN-202` (Tier 1 relay/
rendezvous protocol, lane L6 per D-0300, now unblocked by this lane's
capability primitives) and `MN-208` (private lookup/DHT restriction
enforcement, also now unblocked); HPKE-based recipient-key wrapping for
multi-reader key distribution (deferred — research explicitly flagged
recipient-count/identifier leakage and revocation complexity as needing
separate design); MLS for dynamic encrypted group key management
(explicitly not the right fit for a generic immutable object envelope);
capability attenuation/delegation once exact-grant semantics have been
exercised in practice; `CapabilityScope` variants beyond `Object` as
those surfaces (`mini-social` collections/communities) get their own id
types.

**Supersedes / superseded by:** none. `Object`/`Payload`/v1's wire format
are byte-for-byte unchanged; `ObjectId::of`'s visibility widened
(`fn` → `pub(crate) fn`) with no behavior change.
### D-0302 — Resource price vector and quote engine: new `mini-resource-pricing` crate, quoting only (lane L4, `MN-601`, closes tracking issue #136)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan); D-0094
(`mini-privacy-policy`, the dependency this lane consumes);
`crates/mini-resource-pricing` (new); tracking issue #136

**Decision:** ships `MN-601` as a new, dependency-light crate:
`PriceVector` (micro-MINI per unit of bandwidth/storage, the plain `u64`
"micro-MINI" convention `mini-settlement`/`mini-bounty`/`mini-reward`
already use — no new amount type, confirmed no dedicated one exists
anywhere in this workspace before choosing this), and
`quote(&PriceVector, PrivacyTier, payload_mb, storage_days) ->
Result<Quote>`: a pure function combining `mini_privacy_policy::
expected_cost`'s declared min/max multiplier range with a price vector,
producing a `Quote { min_micro_mini, max_micro_mini, requires_payment }`.
All arithmetic goes through `u128` intermediates with `checked_mul`/
`checked_add`, returning `PricingError::Overflow` rather than saturating
or panicking on an oversized input.

**Reason:** second lane picked from D-0300's plan in the same
founder-directed batch as `L2` (D-0301) — chosen because it is, like
`L2`, a brand-new dependency-light crate with zero existing-code risk,
proving the lane-parallelism pattern a second time with a genuinely
independent PR (no shared files with `L2`'s `mini-transport-policy`).
`D-0301` was already claimed by `L2` in this same session before this
entry was written, so this entry proactively takes `D-0302` rather than
wait to discover the collision on rebase — the exact scenario D-0300's
own "claim at PR-open time, renumber on merge" rule anticipates,
resolved here the cheap way since both lanes are visible to the same
author in the same sitting. Checked-arithmetic-with-explicit-overflow-
error (rather than the workspace's more common panic-free-via-fixed-
caps pattern used in wire codecs) was chosen specifically because this
is money-adjacent: silently saturating a price to `u64::MAX` would be a
wrong, not just a truncated, answer.

**Constitutional impact:** none directly, but this is the first
`D-03xx`-band crate that touches money at all, so it was checked
explicitly: `mini-resource-pricing`'s `Cargo.toml` depends only on
`mini-privacy-policy`, with a standing comment stating it must never gain
a `mini-forge`/`mini-chain` dependency (Directive 1, the voice/value
wall) — pricing a resource must never become a governance signal in
either direction. No payment executes here (no e-cash, no ledger write,
no `mini-value`/`mini-treasury` dependency at all) — that is explicitly
`MN-602`/`MN-603`, later work under the same D-0047 external-crypto-
review gate every other value-bearing prototype in this repo sits behind.

**Implementation status:** shipped and tested. 7 unit tests (Direct
tier's min-equals-max no-range case, Relayed's real range,
max-never-less-than-min across all four tiers, Burst costing at least as
much as Mixed at equal payload, a zero-payload zero-cost edge case,
overflow-is-rejected-not-truncated with `u64::MAX` inputs, and
determinism for repeated identical inputs). `cargo fmt`, `cargo clippy -p
mini-resource-pricing --all-targets --all-features -- -D warnings`, and
`cargo test -p mini-resource-pricing` are clean.

**Failure point:** `quote` is a declared price, not a cleared market
price — nothing here models supply/demand, and the underlying
`ResourceCost` multipliers it prices are themselves the research
document's estimates, not measurements (same inherited honesty
constraint as D-0301). A caller that treats a `Quote` as a binding offer
rather than an estimate would be over-trusting this crate.

**Required follow-up:** `MN-602` (blind prepaid resource credential
protocol review) and `MN-603` (anonymous resource redemption prototype),
both later lanes with their own external-review posture; `MN-604`
(privacy-pool subsidy policy) and `MN-605` (treasury/inflation/whale
simulation extension) build on this crate's `Quote` type once they start.

**Supersedes / superseded by:** none. New crate, no existing type or
behavior changed.
### D-0303 — Human-evidence taxonomy reconciliation: `HumanStatus` unchanged, no rival taxonomy (lane L5, `MN-401`, closes tracking issue #137)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan); D-0086 (the
`FullHuman`→`EvidenceQualifiedHuman` personhood-honesty rename this
reconciliation must not undo); `docs/research/
MININET_RESEARCH_V2_20260713.md` §10 (the five source classes);
`docs/design/human-evidence-taxonomy-reconciliation.md` (new);
`crates/mini-uniqueness/src/status.rs` (verified against, unmodified);
tracking issue #137

**Decision:** the founder-directed contributor picked lane L5 specifically
because it is a prerequisite for later evidence-stamp/continuity-proof/
nullifier/aggregate-proof/external-adapter work, and produced a thorough
research report (repository conventions, `mini-uniqueness`'s current
`SignalSource`/`TrustWeights`/`PromotionPolicy`/`HumanRecord` machinery,
and external credential-standard landscape — W3C VC 2.0, OpenID4VCI/VP,
SD-JWT VC, EUDI wallet pilots) concluding that the source research's five
confidence classes mix three orthogonal axes (participation, evidence
confidence, provenance) rather than forming one ordinal ladder, and that
reconciliation should therefore be **documentation-only**: `Unassessed`
maps to `Unverified`; `HumanEvidenceQualified` maps to `VouchedHuman`;
`StrongHumanEvidence` maps to `EvidenceQualifiedHuman`; `ActiveParticipant`
maps to nothing (participation is not evidence of humanity);
`ExternalUniquenessBacked` maps to nothing (an external issuer's *scoped*
assertion is one more weighted `SignalEvidence` source, already
representable via `SignalSource::External(u32)`, never a promotion to a
new status). This entry adopts that conclusion and the design doc it
produced. Independently verified against `crates/mini-uniqueness/src/
status.rs` before writing this entry: the exact three-variant
`HumanStatus` enum, `SignalSource::External(u32)`, and
`PromotionPolicy::full_required_sources`'s default inclusion of the
seed-anchored vouching graph (closing the #18 Sybil review's
farm-saturation bypass) all match the report's description precisely.

**Reason:** the "higher scrutiny" flag D-0300 placed on this lane was
specifically about not introducing a rival personhood taxonomy next to
`mini_uniqueness::HumanStatus` — the research concludes that the correct
way to honor that constraint is to *not add a type at all*, since none of
the three candidate additions (all five classes as variants; only
`ExternalUniquenessBacked`; only `ActiveParticipant`) survive scrutiny
without either conflating incomparable properties or creating a path for
an external issuer's scoped claim to outrank Mininet's own strongest,
multi-source, internally-verified state. This is the same discipline
D-0086 already established (rejecting `FullHuman`/`VerifiedHuman`-shaped
names because the system cannot distinguish one human from several
colluding identity roots) — this entry keeps applying it rather than
reopening it.

**Constitutional impact:** directly relevant to `docs/INVARIANTS.md`'s
hard-limitation section (§2) and the Sybil-unsolved limitation it states:
this reconciliation explicitly restates, not softens, that no
`HumanStatus` value — including `EvidenceQualifiedHuman` — proves global
personhood uniqueness or that one human controls only one `did:mini`
root. No governance/voice surface is touched. No new cryptography, no
credential adapter, no new type — pure documentation this batch.

**Implementation status:** documentation-only, as scoped. No Rust code
changed; `crates/mini-uniqueness` is unmodified (confirmed by the
verification pass described above). The design doc also states, for
later lanes' benefit, that `MN-406`'s external-uniqueness adapter must
require at least one live Mininet-native source alongside any external
evidence — external evidence alone must never independently promote a
record to `EvidenceQualifiedHuman` — so this workspace never silently
outsources its personhood policy to an external issuer.

**Failure point:** this is a naming/mapping decision, not a new
enforcement mechanism — nothing here changes what `mini_uniqueness`
actually does, only how the founder research's vocabulary is talked about
relative to it. The risk this guards against (an external issuer's
credential being read as stronger than it is) remains a documentation
discipline until `MN-406` actually implements the "at least one
Mininet-native source required" rule in code.

**Required follow-up:** `MN-402` (`EvidenceStamp` interface + issuer
diversity rules), `MN-403` (private continuity proof phase 1 — already
has its own design doc, `docs/design/human-continuity-proof.md`,
D-0075), `MN-404` (context nullifier + pairwise pseudonym design),
`MN-405` (aggregate proof prototype), `MN-406` (external uniqueness
credential adapter — must implement the required-native-source rule this
entry states but does not yet enforce in code), `MN-407` (Sybil-farm/
coercion simulation) — all later lanes, each producing a scoped evidence
assessment that becomes one more weighted `SignalEvidence`, never a value
that directly assigns a `HumanStatus`.

**Supersedes / superseded by:** none. Extends D-0086's naming discipline;
does not modify `mini_uniqueness::HumanStatus` or any other existing type.
### D-0305 — Sphinx-style mix network: research report and protocol specification (lane L3, `MN-204`, closes tracking issue #135)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan); D-0094
(`mini-privacy-policy::Mechanism::MixNetwork`, the named-but-unimplemented
mechanism this document specifies); D-0301 (`mini-transport-policy`'s
`PrivacyTier::Mixed` mechanism list, which already names exactly the four
mechanisms this document specifies and no others); D-0302
(`mini-resource-pricing`, this specification's cover-traffic/bandwidth
cost consumer); `docs/design/mixnet-sphinx-protocol.md` (new); D-0300's
own "Phase D gate" flag; tracking issue #135

**Decision:** ships `MN-204` as a research report and candidate protocol
specification, zero Rust code, matching L3's declared docs-only
footprint. Surveys the historical line from Chaum Mixes (1981) through
Stop-and-Go Mixes, Mixminion, Sphinx (Danezis & Goldberg 2009 — the
compact fixed-size packet format underlying every production system
since), Loopix (Piotrowska et al. 2017 — continuous-time Poisson mixing,
stratified topology, independent cover traffic), Katzenpost, Nym, and
Outfox; a comparison matrix across Tor/Loopix/Nym/Sphinx/Outfox/the
Mininet candidate on latency, adversary model, active-attack resistance,
replay handling, packet/bandwidth overhead, implementation complexity,
production maturity, audit history, and PQ readiness; a list of thirteen
simulations this repository still owes (malicious-node sweeps, AS-level
adversary, jurisdiction diversity, relay churn, mobile clients, sparse/
heavy traffic, intersection attacks, cover-traffic/battery/bandwidth
cost) before any Tier 2 multiplier in `mini-privacy-policy` stops being a
declared estimate; a fourteen-entry attack catalog (tagging, replay,
predecessor, blending/n-1, route capture, selective DoS, timing
correlation, congestion, guard compromise, flooding, statistical
disclosure, long-term intersection disclosure, Sybil relay
concentration) each with description/affected-systems/mitigation/
residual-risk; an explicit "why not just use Tor" section (Tor's own
documentation excludes global-passive-adversary resistance by design;
Tier 2 specifically targets that adversary class; the two are
complementary via `MN-207` bridging, not substitutes); a ten-section
candidate protocol specification for `MN-205` (Sphinx packet format over
`mini-crypto`'s existing X25519/HKDF-SHA256/ChaCha20-Poly1305, Loopix-
style Poisson delay and stratified topology, resource-cost-gated relay
Sybil resistance, explicit integration points into the three already-
shipped crates named above); and an eleven-item "future research beyond
MN-204" list (PQ KEMs, PIR mailboxes, decoy routing, formal verification,
UC-security proofs, anonymous-credential integration, relay reputation
without deanonymization, among others) naming what's deliberately
deferred so omissions read as decisions.

**Reason:** fourth lane the founder picked from D-0300's plan this batch,
and the natural complement to the three code lanes already shipped (L1
D-0304, L2 D-0301, L4 D-0302, L5 D-0303) — L3 depends on understanding
the surrounding privacy-doctrine work without depending on its code,
exactly the isolation D-0300's lane table already noted ("zero Rust
footprint"). Written to the depth an external cryptographer would
actually review, per explicit founder direction, rather than a survey
paragraph — because `MN-205`'s eventual external-review gate (this
entry's Constitutional impact field) needs a concrete specification to
review, not an intent statement.

**Constitutional impact:** composes a single already-published, peer-
reviewed, real-world-deployed construction (Sphinx, with a decade-plus
of production deployment history through Loopix/Katzenpost/Nym) —
CLAUDE.md's composition-of-prior-art allowance, the same class already
accepted for `mini-value`'s Bulletproofs (D-0036/D-0040) and
`mini-porep`'s SDR sealing (D-0064). No new cryptographic primitive is
proposed anywhere in the document — every primitive the candidate
specification calls for (X25519 agreement, HKDF-SHA256, ChaCha20-
Poly1305, BLAKE3) already exists in `mini-crypto`, already reviewed,
already used elsewhere in this workspace. Explicitly restates, and does
not soften, D-0094's residual floors: the document's timing-correlation
and statistical-disclosure attack entries state plainly that F2/F3 are
unremovable by construction, matching `ResidualFloor::
GlobalObserverLongSessionCorrelation`/`IntersectionOverTime` exactly.
Most importantly for constitutional posture: **this document does not
lift D-0300's Phase D gate**. `MN-205` (the actual mix-node
implementation) still requires the same external-review posture already
applied to `mini-value`/`mini-treasury` (D-0047 gate) before any
operational anonymity claim reaches a real user — stated explicitly in
§0 and §9 of the design doc, not left implicit.

**Implementation status:** research report and specification only, as
scoped. Zero lines of Rust changed; `crates/` is untouched by this batch.
`docs/design/mixnet-sphinx-protocol.md` is the sole artifact.

**Failure point:** several claims in the comparison matrix (§3) and the
historical section (§2, specifically Outfox's exact citation) are
qualitative and explicitly flagged as needing independent primary-source
verification before use in an external audit submission — this document
is honest about that gap rather than presenting unverified figures as
settled. The thirteen named simulations (§4) are not performed; every
bandwidth/latency multiplier this document references from
`mini-privacy-policy` remains a declared estimate, not a measurement,
until that work happens.

**Required follow-up:** the thirteen simulations named in §4; primary-
source verification of the Outfox citation before any audit-facing use;
`MN-205` (mix node state machine implementation) as the next lane,
explicitly gated on external review per this entry's own Constitutional
impact field; the eleven future-research items in §8, each already
scoped as separate, later work rather than folded into `MN-205`
prematurely.

**Supersedes / superseded by:** none. New document, no existing type,
crate, or decision changed.

### D-0306 — Tier 1 relay + rendezvous protocol: new `mini-relay` crate, zero changes to any existing crate (lane L6, `MN-202`, closes tracking issue #144)  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0300 (lane plan, "Sequencing after
wave 1" naming this lane once L1/L2 land); D-0304 (L1, `mini-objects`
capability/pseudonym primitives this lane's design was checked against
but does not depend on); D-0301 (L2, `mini-transport-policy`, whose
`PayloadSizeClass` this lane reuses read-only); `crates/mini-relay` (new);
tracking issue #144

**Decision:** ships `MN-202` (research §5.2, Tier 1 relay + rendezvous)
as a new, purely additive crate: `RelayRole` (`Entry`/`Rendezvous`/
`Delivery`, the three separable roles — entry relay knows the client's
address not the destination, rendezvous relay knows the destination's
mailbox capability not the client's address); `ConnectionId` (fresh
random 16 bytes, never a `did:mini` root or derived from one);
`derive_relay_identity` (a per-role, per-connection pairwise pseudonym
via `did_mini::Controller::incept_pairwise_pseudonym` called directly
with this crate's own domain-separated context — not through
`mini_objects::pseudonym`'s wrapper, so a relay operator's role identity
can never be linked to any object-authorship/capability-holder pseudonym
the same root uses elsewhere); `MailboxGrant`/`MailboxToken`/`MailboxId`
(a holder-bound, token-committed capability over an opaque mailbox,
structurally mirroring `mini_objects::CapabilityGrant`'s exact discipline
but as a fully independent typed-domain type — no shared `CapabilityScope`
variant, no dependency on `mini-objects` at all); `RelayEnvelope`
(per-hop AEAD-sealed structure over `mini_bearer::Channel`, binding
role/connection-id/size-class as associated data, reusing
`mini_transport_policy::PayloadSizeClass` read-only); and
`enforce_role_separation` (a pure function rejecting an assignment where
one relay identity holds two roles for one delivery, or a mandatory
`Entry`/`Rendezvous` role is missing).

Before designing, an API survey (Explore agent, full-crate grep across
`mini-objects`, `mini-transport-policy`, `mini-bearer`, `mini-net`,
`mini-privacy-policy`) confirmed: `mini_objects::CapabilityScope` is
hard-locked to `Object(ObjectId)` with no mailbox concept; `Mechanism`
has no dedicated rendezvous/mailbox variant; `mini_bearer::Channel`
exposes raw `seal`/`open` sufficient as a building block but has zero
relay-forwarding logic; and — most consequentially — `mini-net` has **no
PUT/GET/provider-record/value-storage DHT layer at all**, only peer-
bucket routing, gossip dedup, and peer exchange. That last finding is why
this entry ships `MN-202` alone and explicitly defers `MN-208` (DHT
lookup restriction, research §5.6): there is nothing yet in `mini-net`
for a restriction to restrict. Given that, the cleanest footprint was a
brand-new crate depending only on already-shipped read-only exports
(`did-mini`, `mini-crypto`, `mini-bearer`, `mini-transport-policy`) —
zero modification to any existing crate, matching L2/L4's cleanest
precedent rather than L1's (which had to touch `mini-objects`).

**Reason:** the founder's "well next we need to continue coding"
direction, following the four-lane consolidation (PR #143) and its
merge, pointed at whatever wave-2 work D-0300's own plan had already
named as unblocked once L1 and L2 shipped — this lane, exactly as that
document's "Sequencing after wave 1" section anticipated ("footprint
decided when L1 lands and its actual public types are known, not guessed
now"). `MailboxGrant` was deliberately built as an independent type
rather than extending `mini_objects::CapabilityScope` with a `Mailbox`
variant: `CapabilityScope`'s own doc comment states future scope variants
are added "as those surfaces get their own id types," and a mailbox
capability has a structurally different resource (a rotating queue, not
a signed object) and simpler right structure (one implicit "may collect"
right, not five independent rights) — forcing it into the object-scoped
enum would blur that distinction rather than clarify it. `MN-208` is
recorded as explicitly deferred, not silently dropped, because scoping it
into this lane would have meant designing `mini-net`'s entire DHT
value-storage layer as a side effect of a privacy lane — a much larger
decision belonging to whoever owns `mini-net`'s roadmap.

**Constitutional impact:** none. No crypto primitive invented — this
composes `mini-crypto`'s existing X25519/HKDF-SHA256/ChaCha20-Poly1305
(via `mini-bearer::Channel`) and Ed25519 signing (via `did-mini`) exactly
as already reviewed elsewhere in this workspace (Directive 14/no-new-
cryptography rule). `MailboxGrant`'s signing/commitment/holder-proof
messages are each domain-separated with their own `mininet/mini-relay/…`
tag (typed-domain rule — never a generic `sign(bytes)` call on caller-
assembled data). No dependency edge to `mini-forge`/`mini-chain` (voice/
value wall unaffected — this crate has no economic content at all).

**Implementation status:** shipped and tested. 43 new unit tests across
`role.rs` (2: tag round-trip, unknown tag rejected), `connection.rs` (7:
two generated ids differ, id round-trips through bytes, same root/role/
connection derives the same identity, different roles/connections/roots
each derive different identities, a derived identity can sign and be
independently verified), `mailbox.rs` (21: valid grant/proof validates,
mailbox-mismatch fails, wrong-token fails, a token-commitment copied into
a different-mailbox grant fails, missing/wrong holder proof fails ×2,
expired/not-yet-valid fail, wrong-issuer fails, wire round-trip ×2 incl.
a validity-window round-trip that still enforces the window post-decode,
unknown-version rejected, truncation-at-every-length, trailing-bytes,
and an explicit rotation test proving an old grant/token pair does not
satisfy a newly-issued mailbox), `envelope.rs` (10: seal/open round-trip
over two linked `Channel`s, wire round-trip, a decoded envelope still
opens, role/connection-id/size-class tamper each independently break
decryption ×3, opening with the wrong channel fails, unknown-version
rejected, truncation-at-every-length, trailing-bytes), and
`role_separation.rs` (8: three distinct relays pass, entry+rendezvous
alone pass (delivery optional), the same relay holding two or all three
roles is rejected, missing entry/rendezvous each rejected, a duplicate
entry role by different relays is rejected, an empty assignment is
rejected for missing entry). `cargo fmt`, `cargo clippy --all-targets
--all-features --workspace -- -D warnings`, and `cargo test --workspace
--all-features` are clean (123 `test result: ok` blocks workspace-wide,
zero failures).

**Failure point:** this crate has no live network wiring — `RelayEnvelope::
seal`/`open` are proven only in-process against paired `mini_bearer::
Channel`s constructed via `Initiator`/`Responder` in this crate's own
tests, the same honesty posture `mini-transport-policy` (D-0301) used for
"decisions only, no execution." A live multi-process relay demo (like
`mini-net`'s gossip demo) does not exist yet, so no delivery has actually
crossed a real socket through this protocol. `MailboxGrant` has no
revocation mechanism (same limitation D-0304's `CapabilityGrant` already
documents) — a compromised token or grantee key cannot be invalidated
early, only allowed to expire. `enforce_role_separation` checks identity
equality only; it has no way to detect Sybil relay operators presenting
different `Did`s while actually being the same party (that is the
Sybil-unsolved hard limitation `docs/INVARIANTS.md` already states, not a
new gap this crate introduces).

**Required follow-up:** a live multi-process/multi-socket relay demo
proving an actual delivery crossing entry → rendezvous → delivery hops
over real TCP; wiring `mini_transport_policy::route`'s `RouteDecision`
output to actually select and invoke this crate's relay logic (currently
two disconnected layers — decision and mechanism); `MN-207` (bridge/
pluggable transport interface, buildable against this lane's and L2's
real types per D-0300's own sequencing note); `MN-208` (DHT lookup
restriction) once `mini-net` grows a value-storage DHT layer to restrict
at all — explicitly not this lane's job; `MN-205` (mix node) remains
separately gated behind external review per D-0305.

**Supersedes / superseded by:** none. New crate, no existing type or
behavior changed.

### D-0307 — Wire `mini_transport_policy::route()` output to `mini-relay` role planning: new `plan` module, zero changes to any other existing crate  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0306 (this lane's own "Required
follow-up" field, which named this exact gap); D-0301 (`mini-transport-
policy`, the crate this module now depends on and consumes);
`crates/mini-relay/src/plan.rs` (new)

**Decision:** adds `mini_relay::roles_for_route_decision(&RouteDecision)
-> Result<Vec<RelayRole>>`, the missing link between the routing-decision
layer (`mini-transport-policy`, D-0301) and the mechanism layer
(`mini-relay`, D-0306) that D-0306's own Required-follow-up field
explicitly flagged as still disconnected. The function is deliberately
narrow rather than permissive: it accepts only a `RouteDecision` whose
`achieved.tier` is exactly `PrivacyTier::Relayed` and whose
`achieved.mechanisms` actually names `Mechanism::OnionRelay`, returning
`[RelayRole::Entry, RelayRole::Rendezvous]` — the exact mandatory pair
`enforce_role_separation` itself requires. A `Direct`-tier decision
(needs no relay) and a `Mixed`/`Burst`-tier decision (needs the
unbuilt, externally-review-gated mix network, `MN-205`) each return a
distinct, named error rather than silently returning an empty plan or
guessing — a caller cannot mistake "no error" for "this crate is
handling your Tier 2+ request."

**Reason:** shipped immediately after PR #145 (`mini-relay`) was opened,
per the founder's "continue working as suggested" direction, as
preparatory work on a branch stacked on the pending PR rather than
against `main` directly — `mini-relay` does not exist on `main` yet, so
this module cannot be built independently of it. Held for a separate PR
(not squashed into #145) so the founder's review of the base `mini-relay`
crate is not entangled with this smaller, purely-additive wiring change,
consistent with this session's practice of keeping D-0300-track PRs
narrowly scoped. `Delivery` is deliberately never planned by this
function: whether a third hop is warranted is a caller/policy decision,
not something a route-decision-to-role bridge should decide unilaterally.

**Constitutional impact:** none. No crypto, no new type beyond three
`RelayError` variants (`TierNeedsNoRelay`, `TierNotHandledByThisCrate`,
`MechanismNotRequested`), no dependency edge to `mini-forge`/`mini-chain`.
`mini-relay`'s `Cargo.toml` gains one new dependency,
`mini-privacy-policy` (for `Mechanism`/`PrivacyTier`), already a
dependency of `mini-transport-policy` itself — no new external crate
enters the workspace.

**Implementation status:** shipped and tested. 6 new unit tests in
`plan.rs`: a real `mini_transport_policy::route()` call at `Relayed`
tier plans exactly `[Entry, Rendezvous]`; `Direct` tier is rejected as
needing no relay; `Mixed` and `Burst` tiers are each rejected as not
handled by this crate; a decision with `Mechanism::OnionRelay` stripped
out is rejected; and the planned roles, given three distinct relay
identities, satisfy `enforce_role_separation` unmodified — proving the
two modules actually compose, not just type-check. `cargo fmt`, `cargo
clippy --all-targets --all-features --workspace -- -D warnings`, and
`cargo test --workspace --all-features` are clean.

**Failure point:** this function only plans *which roles* a delivery
needs — it does not select *which relay operators* fill those roles,
issue mailbox grants, or establish `mini_bearer::Channel`s. A caller
still has to do all of that itself; `roles_for_route_decision` is a
necessary but far from sufficient step toward an actual live relay.

**Required follow-up:** relay-operator selection/discovery (unbuilt —
`mini-net`'s peer routing table exists but nothing selects relay
operators from it yet); a live multi-process demo actually exercising
`route()` → `roles_for_route_decision` → `MailboxGrant`/`RelayEnvelope`
end to end over real sockets, still D-0306's largest open item.

**Supersedes / superseded by:** none. Extends `mini-relay` additively;
no existing type or behavior in any other crate changed.

### D-0308 — Live two-hop relay demo over real TCP sockets: closes D-0306/D-0307's "no live demo" honest limit  ·  *Accepted*
**Date:** 2026-07-14 · **Refs:** D-0306 (named this as the largest open
item in its Required follow-up); D-0307 (named it again);
`crates/mini-relay/tests/live_relay_over_tcp.rs` (new)

**Decision:** adds one automated integration test, run by `cargo test`
like every other test in this workspace (no manual multi-terminal
invocation, unlike `mini-net`'s `gossip_live_demo` example), proving a
message crosses two independently-established **real TCP sockets** —
client→entry and entry→rendezvous — each with its own genuine
`mini_bearer::Channel` handshake (`Initiator::start`/`Responder::respond`
carried over a real `TcpBearer`, the exact pattern `mini-cli`'s `sync
connect`/`listen` already uses), and arrives at the rendezvous relay
byte-for-byte. `RelayRole`/`ConnectionId`/`derive_relay_identity`/
`enforce_role_separation` are all exercised on genuinely independent
generated identities, not placeholders. Scoped narrowly and deliberately:
mailbox pickup (`MailboxGrant`/`MailboxToken`/holder-proof) is **not**
re-proven over a third socket here, because it is pure local logic that
doesn't care whether its inputs arrived over a wire or a function call —
already exercised 21 times in `mailbox.rs`'s own unit tests — and adding
ad hoc wire-format code to serialize it for this demo would cost real
lines without adding real truth-value.

**Reason:** `RelayEnvelope`'s own doc comment already states it is "a
**one-hop** AEAD-sealed relay message," so this demo does not invent any
new multi-hop protocol structure — it wires the crate's existing,
already-tested primitives together exactly as designed: unwrap at one
hop, reseal fresh for the next. This resolves an explicit design question
raised while planning the demo (nested "onion" layering vs. hop-by-hop
re-sealing) in favor of what the crate's own architecture already
committed to, rather than inventing a stronger cryptographic property
(end-to-end payload secrecy across relays) that was never actually built
and would have required new, undiscussed protocol design — exactly the
kind of scope creep this session's practice has been to flag, not
smuggle in via a "demo."

**Constitutional impact:** none. No new type, no new crypto, no
dependency changes — the test file depends only on `mini-relay`,
`mini-bearer`, `did-mini`, and `mini-transport-policy`, all already
in-tree. Directive 14 (no new cryptography) is reinforced, not
weakened: this demo proves existing composition works over a real
transport, it does not add any.

**Implementation status:** shipped and tested. 1 new integration test,
`a_message_crosses_entry_and_rendezvous_hops_over_real_tcp_sockets`,
passing. `cargo fmt`, `cargo clippy --all-targets --all-features
--workspace -- -D warnings`, and `cargo test --workspace --all-features`
are clean (124 `test result: ok` blocks workspace-wide — the new
integration-test binary is its own block — zero failures).

**Failure point:** this is a **hop-by-hop store-and-forward model, not
onion routing** — the entry relay necessarily decrypts and sees the
plaintext it forwards to the rendezvous relay. That is Tier 1's actual
research-doc scope (§5.2), correctly weaker than Tier 2's layered-mix
property (§5.3, `MN-205`, gated behind external review) — but a reader
skimming "relay demo" without this entry's honest-limits section could
mistake it for stronger metadata protection than it provides. The demo
uses two threads in one process on loopback, not genuinely separate OS
processes or machines (unlike `mini-net`'s `gossip_live_demo` example) —
real socket I/O is still exercised, but process/network isolation is
not.

**Required follow-up:** relay-operator selection/discovery; a mailbox-
pickup-over-a-real-socket demo if that slice's own protocol (KEL exchange
alongside the grant/token/holder-proof) is ever designed as a first-class
feature rather than added just to round out a demo; a genuinely
multi-process version of this demo (separate `cargo run` invocations
like `gossip_live_demo`) if that becomes independently useful.

**Supersedes / superseded by:** none. New test file only; no existing
type or behavior changed.

### D-0309 — `mini-bridge`: pluggable entry-transport interface, one real direct implementation (`MN-207`)  ·  *Accepted*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
MN207_BRIDGE_PLUGGABLE_TRANSPORT_RESEARCH_20260714.md`; `docs/design/
bridge-pluggable-transport.md` (new); D-0306 (role-separated relay this
crate's `DirectBridgeTransport` connects toward); CLAUDE.md's no-new-
cryptography rule

**Decision:** adds `mini-bridge`, a new, additive-only crate (zero
changes to any existing crate) implementing exactly the research
report's own recommended first slice: a typed, `#[non_exhaustive]`
`TransportId` naming nine transport kinds; `TransportCapabilities`/
`capabilities_for` declaring policy facts (probe resistance, address
agility, domain/broker dependency, overhead class) for every named
transport, including ones with no adapter yet; `BridgeDescriptor`, a
self-signed one-party reachability claim with a mandatory (non-`Option`)
`expires_at_ms` enforcing "short-lived where practical" at the type
level; a synchronous `PluggableTransport` trait (this workspace has no
async runtime anywhere — `mini_bearer::Bearer` is the existing
sync-trait precedent, diverging deliberately from the research report's
own illustrative `async fn connect` pseudocode); and one real, tested
implementation, `DirectBridgeTransport`, dialing a real TCP socket via
`TcpStream::connect_timeout` and performing a genuine `mini_bearer::
Channel` handshake, verifying the descriptor's signature and validity
window strictly before the socket is touched. 24 new tests, including a
real-socket test proving a sealed/opened message round-trips end to end.

**Reason:** the research report's executive conclusion is explicit that
MN-207 should not become "invent a Mininet obfuscation protocol" — it
should be a small, typed interface plus adapters to already-proven
external systems added over time. Implementing exactly the report's own
Phase 0/1/one-real-Phase-2 recommendation (rather than attempting obfs4/
WebTunnel/Snowflake/Tor-PT adapters, which need audited external
implementations this workspace would compose, not invent) keeps this
batch inside what a single PR can honestly claim to have built and
tested.

**Constitutional impact:** none. No dependency changes beyond
`mini-crypto`/`did-mini`/`mini-bearer` (all already in-tree, path deps
only). Directive 14 (no new cryptography) is reinforced: `BridgeDescriptor`
composes `did-mini`'s existing KEL/signature machinery and
`DirectBridgeTransport` composes `mini-bearer`'s existing `Channel` —
nothing here invents a primitive. `TransportId::DirectTlsV1`'s `Tls` is a
wire-tag label carried over from the research report's own taxonomy, not
a claim of real TLS — see the crate's `direct.rs` module docs and the
design doc's honesty section.

**Implementation status:** shipped and tested. `cargo fmt`, `cargo
clippy --all-targets --all-features --workspace -- -D warnings`, and
`cargo test --workspace --all-features` are clean.

**Failure point:** `TransportCapabilities` are **declared** policy facts,
not measured ones — a transport's real-world probe resistance depends on
deployment and the current adversary, and nothing in this crate verifies
a declared capability against live network behavior. `DirectBridgeTransport`
provides no address agility, no obfuscation, and no probe resistance
beyond `mini-bearer`'s existing channel properties — it is a real
transport, not a censorship-resistant one on its own.

**Required follow-up:** obfs4/Lyrebird, WebTunnel, Snowflake, and Tor
pluggable-transport subprocess adapters (research report Phases 3-8),
each gated on an audited external implementation; bridge-distribution
channels; measurement/active-probing-detection tooling; local BLE/Wi-Fi
bridge transports (gated on hardware this environment cannot exercise,
mirroring `mini-presence`'s existing honest limits); wiring
`mini-transport-policy`'s routing decisions to transport selection.

**Supersedes / superseded by:** none. New crate only; no existing type
or behavior in any other crate changed.

### D-0310 — `mini-private-index`: capability-derived lookup labels + local signed index, doctrine for the public-DHT boundary (`MN-208`)  ·  *Accepted*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
MN208_PRIVATE_LOOKUP_DHT_RESEARCH_20260714.md`; `docs/design/
private-lookup-and-dht-boundary.md` (new); D-0306 (independently
confirmed `mini-net` has no value-storage DHT layer to restrict, the
premise this decision starts from); tracking issue #144;
CLAUDE.md's no-new-cryptography rule, D-0047 (external crypto review gate)

**Decision:** adds `mini-private-index`, a new, additive-only crate
(zero changes to any existing crate) implementing exactly the research
report's own recommended Phase 0 (doctrine) and Phase 1 (local-only
primitive): `LookupPrivacyClass`, a frozen, ordered, `#[non_exhaustive]`
five-tier taxonomy (`Public` → `CapabilityScoped` → `PrivateProxied` →
`PrivateBundled` → `PrivatePIR`); `derive_lookup_label` deriving
capability-scoped rotating `LookupLabel`s via HKDF-SHA256 across nine
disjoint `LookupPurpose` domains; `PrivateIndexRecord`/`RecordSizeClass`,
a signed, fixed-size-class record whose opaque encrypted payload this
crate never interprets; and `LocalIndex`, a local, in-memory store
enforcing signature validity, writer-cannot-hijack-another's-label, and
strictly-increasing sequence (rollback rejection), with `lookup()`
returning `None` indistinguishably for both a missing and an expired
record. 27 new tests, including rollback-rejection, writer-hijack-
rejection, and cross-label-non-collision cases. Only
`LookupPrivacyClass::CapabilityScoped`'s primitive is implemented; the
other four tiers are named as a stable vocabulary for future work, not
built.

**Reason:** the research report's executive conclusion is explicit that
MN-208 should not begin by building a general-purpose value DHT and
adding privacy around it afterward — `mini-net` has no provider-record
or value-storage DHT to restrict today, independently confirmed while
scoping `mini-relay` (D-0306, #144). The correct first slice is a design
doctrine plus a narrowly scoped local primitive proving the
signature/rollback/label discipline a networked private index would need
to enforce per-replica — not a network protocol, which the report itself
sequences later (role-separated queries, batching/decoys, PIR).

**Constitutional impact:** none. No dependency changes beyond
`mini-crypto`/`did-mini`/`zeroize` (all already in-tree elsewhere).
Directive 14 (no new cryptography) is reinforced: `derive_lookup_label`
is HKDF-SHA256 via `mini_crypto::KdfSuite`, the same already-reviewed
primitive `mini-bearer`'s channel and `mini-treasury`'s key derivation
use. `LookupPrivacyClass::PrivatePIR` is explicitly named but not
implemented, and this decision records that no future PR may claim that
tier without the external cryptographic review D-0047 requires.

**Implementation status:** shipped and tested. `cargo fmt`, `cargo
clippy --all-targets --all-features --workspace -- -D warnings`, and
`cargo test --workspace --all-features` are clean.

**Failure point:** `LocalIndex` is genuinely local-only — "No network
yet" is not a hedge, it is the actual scope. Nothing in this crate hides
a client's network address from an index service (needs the deferred
relay-based role-separation layer), nothing pads or batches query
traffic, and the encrypted payload's confidentiality is entirely the
caller's responsibility — this crate stores and forwards opaque bytes
and authenticates only the writer and the label, not the payload's
plaintext meaning.

**Required follow-up:** a real wire protocol and replicated private-index
service (Phase 2+); relay-based OHTTP-style query role separation (Phase
3, likely composing `mini-relay`); query batching/decoys and
caching/prefetch (Phase 4-5); PIR research and external cryptographic
review before `PrivatePIR` may be implemented (Phase 9, gated on D-0047);
wiring a real content-encryption scheme behind `LookupPurpose::
RecordEncryption`/`RecordAuthentication`, currently reserved domains with
no wiring.

**Supersedes / superseded by:** none. New crate only; no existing type
or behavior in any other crate changed.

### D-0096 — Adopt KEL witness receipts + duplicity gossip as the design direction for the "never seen a fresher log" gap (audit #12 F4, M3)  ·  *Accepted (design only)*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
KEL_WITNESS_RECEIPTS_DUPLICITY_GOSSIP_RESEARCH_20260715.md`; `docs/design/
kel-witness-receipts-and-duplicity-gossip.md` (new); SPEC-01 §7; invariant
M3; audit #12 finding F4; D-0088 (`FreshnessPins`, the interim pin-based
fix this extends, not replaces)

**Decision:** adopts the research report's recommended architecture for
the harder half of M3 — a first-contact verifier with no prior pinned
state has no protection against a valid-looking, controller-signed
private fork. The adopted direction: KERI-inspired asynchronous witness
receipts (typed `WitnessReceiptStatement`/`WitnessReceipt` binding
identity, sequence, event digest, prior digest, event kind, witness-
policy generation, and a coarse epoch — never generic `sign(bytes)`),
threshold `WitnessedEventCertificate`s, first-seen monotonic witness
state producing compact `ControllerDuplicityProof`/
`WitnessEquivocationProof` evidence on conflict, and proof-carrying
gossip of compact `KelHeadSummary`s piggybacked on ordinary peer
interactions rather than a standalone global service. Explicitly rejects
a global identity ledger, mandatory public transparency for private
identities, witness consensus/BFT, and interactive signature aggregation
for the first version. **This decision is design-only** — no code
changes. The research report's own closing recommendation states the
correct sequencing is a design-only PR first, then a small receipt/proof-
type PR, then an in-memory witness state machine, and only after that
network gossip — "the dangerous mistake would be starting with a witness
daemon or Merkle log before freezing exactly what a receipt means."

**Reason:** `did_mini::FreshnessPins` (D-0088) closes the case where a
verifier already holds prior state for an identity; it structurally
cannot help a verifier meeting an identity for the first time, since
"no fresher event in local storage" proves nothing about a personalized
fork the attacker never showed this verifier. Witness receipts convert
observation into transferable, offline-verifiable evidence; gossip is
what turns two isolated witness views into detectable duplicity. Neither
alone is sufficient — the report's own analysis of why receipts without
gossip only catch equivocation by accident, and gossip without signed
receipts is unresolvable claims, both apply directly to Mininet's KEL.

**Constitutional impact:** none yet — no code exists to have impact.
When implemented, the design explicitly preserves: self-certifying
identity ownership (witnesses attest, never author or rotate); offline
verifiability (receipts and duplicity proofs are self-contained bytes);
no central registry (witness sets are controller-chosen, gossip is
peer-to-peer); and graduated assurance rather than a false global-
freshness claim (`KelAssurance::WitnessedRecentAndGossiped` is honestly
scoped to "within this verifier's gossip horizon," never "globally
freshest").

**Implementation status:** design only. No Rust code in this PR. Phase 1
(receipt types) is the next scoped deliverable, not started.

**Failure point:** this decision has no code failure point yet by
construction (nothing implemented). The design's own named failure
conditions, to watch for once Phase 1+ lands: witnesses signing
conflicting receipts without producing detectable evidence; a receipt
from a retired witness-policy generation being miscounted under a newer
policy; private-identity gossip becoming globally enumerable; recovery
silently bypassing witness consistency or erasing prior duplicity
evidence; and "threshold witnessed" ever being described as proof no
conflicting branch exists anywhere.

**Required follow-up:** Phase 1 (`WitnessPolicy`/`WitnessReceiptStatement`/
`WitnessReceipt`/`WitnessedEventCertificate` types, canonical encoding,
no network); Phase 2 (in-memory witness state machine); Phase 3 (KEL
verification integration, `KelAssurance` output); Phases 4-8 (collection
protocol, gossip, persistent service, rotation/recovery, public-authority
transparency logs); Phase 9 (adversarial network simulation); Phase 10
(external cryptographic/protocol review before any high-value authority
decision depends on this layer, per D-0047).

**Supersedes / superseded by:** none. `FreshnessPins`/D-0088 is extended,
not superseded — pinning remains the baseline for returning verifiers;
this decision adds the first-contact layer pinning cannot provide.

### D-0095 — `mini-crypto`: `SignatureSuite::MlDsa65` verify-only support, Phase 1 of the post-quantum identity migration (issue #15)  ·  *Accepted*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
PQ15_POST_QUANTUM_MIGRATION_RESEARCH_20260715.md`; `docs/design/
post-quantum-identity-migration.md` (new); SPEC-01 §13 (the crypto-agility
frozen invariant this makes real); CLAUDE.md's no-new-cryptography rule;
D-0047 (external crypto review gate)

**Decision:** adds `SignatureSuite::MlDsa65` (FIPS 204, wire tag `0x02`,
already reserved in `suite.rs`'s own comments) to `mini-crypto`, composing
the externally-maintained `fips204` crate (v0.4, MIT/Apache-2.0, pure
Rust, no unsafe, feature-scoped to just `ml-dsa-65`) rather than
implementing ML-DSA's lattice math in-house. Scoped to exactly the
research report's own recommended Phase 1: `VerifyingKey`/`Signature` can
parse and verify real ML-DSA-65 material (`public_key_len()`/
`signature_len()` sourced from `fips204`'s own constants: 1952/3309
bytes); `SigningKey` stays Ed25519-only, with no production key-generation
or signing path for the PQ suite. `to_bytes()` on `VerifyingKey`/
`Signature` now returns `Vec<u8>` instead of a fixed-size array, to
accommodate ML-DSA-65's much larger sizes — audited against every call
site in the workspace before merging; all were already slice-based, so
the change compiled cleanly workspace-wide except for one unrelated
regression (`did-mini::Controller`'s `SigningKey: Clone` bound, dropped
during the rewrite, restored same PR after the full-workspace build caught
it). 11 new unit tests including a round-trip against a real
`fips204`-generated keypair/signature (produced via a
`dev-dependencies`-only `fips204`/`default-rng` feature, so production
builds never link OS-RNG PQ keygen), tamper/wrong-key/wrong-length
rejection, and cross-suite mismatch rejection.

**Reason:** the research report's executive conclusion is explicit that
simply adding the enum variant and flipping the default is not a
migration — it names Phase 1 (verify-only, no generation, no KEL
activation) as the correct first slice, with the actual identity-migration
protocol (dual-authorised hybrid KEL rotation, downgrade prevention,
legacy-client handling) sequenced as separate, later work belonging to
`did-mini`, not `mini-crypto`. Implementing exactly that first slice keeps
this PR inside what can be honestly tested and reviewed in one batch,
mirroring the discipline already used for `mini-bridge`/`mini-private-index`
(D-0309/D-0310) on founder-supplied research this same session.

**Constitutional impact:** none negative; SPEC-01 §13's crypto-agility
invariant is exercised, not weakened — `SignatureSuite::DEFAULT` remains
`Ed25519`, unchanged by this decision. Directive 14 (no new cryptography)
is honored: `fips204` implements the already-standardized FIPS 204
construction; this crate composes it behind the existing suite-tagged
API, adding no novel cryptographic design. The `fips204` dependency
addition was explicitly confirmed with the founder before being built or
tested, given `mini-crypto`'s security-critical, workspace-wide blast
radius — this was not treated as a routine additive-crate decision.

**Implementation status:** shipped and tested in `mini-crypto` only.
`cargo fmt`, `cargo clippy --all-targets --all-features --workspace -- -D
warnings`, and `cargo build --workspace --all-features` are clean;
`cargo test --workspace --all-features` run recorded in this PR.

**Failure point:** this is a **primitive, not a migration** — no KEL can
actually rotate to ML-DSA-65 yet, since `did-mini` is untouched. FIPS
204's public-key encoding has no structural validity check beyond byte
length (unlike Ed25519's compressed-curve-point check): an all-zero
"public key" of the correct length parses successfully and simply never
verifies a real signature — documented and tested
(`an_all_zero_ml_dsa_65_key_parses_but_never_verifies_a_real_signature`)
rather than silently assumed to behave like Ed25519. No external
cryptographic review of this suite wrapper or the `fips204` implementation
has occurred; no production identity may depend on `MlDsa65` authority
until that review happens (D-0047).

**Required follow-up:** Phase 2 (key generation, benchmarks, mobile/WASM
testing); Phase 3 (the actual `did-mini` KEL hybrid-migration protocol —
PQ pre-commitment, dual-authorised activation rotation, downgrade
prevention, legacy-client stale-head handling); Phase 4 (recovery,
delegated-device, witness migration); ML-KEM-768 hybrid session
establishment for `mini-bearer` (a separate track); external cryptographic
review before any of the above reaches production identity authority.

**Supersedes / superseded by:** none. Additive to `mini-crypto`'s existing
suite-tagged API; no existing type's behavior changed for the `Ed25519`
suite.

### D-0098 — PIR research and external-review preparation for `mini-private-index`: frozen workload, candidate portfolio, no code (`MN-208` Phase 9)  ·  *Accepted (research only)*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
MN208_PIR_RESEARCH_AND_REVIEW_PREPARATION_20260715.md`; `docs/design/
mn208-pir-research-and-review-preparation.md` (new); D-0310
(`mini-private-index`, whose `LookupPrivacyClass::PrivatePIR` this
prepares the review path for); D-0047 (external crypto review gate);
CLAUDE.md's no-new-cryptography rule

**Decision:** adopts the research report's own recommended Phase 9 scope
exactly: Mininet does not select or implement a production Private
Information Retrieval (PIR) protocol for `mini-private-index` yet. This
decision freezes the first PIR workload the network will ever benchmark
against — exact retrieval of one fixed-size encrypted mailbox or
provider descriptor from one immutable, epoch-versioned, equal-length-
record database, mapping directly onto `mini-private-index`'s existing
`PrivateIndexRecord`/`RecordSizeClass` vocabulary (D-0310) rather than a
new database shape invented for PIR — and names a four-candidate research
portfolio: whole-index download (mandatory baseline), two-server
information-theoretic PIR (preferred first true-PIR candidate), one
mature single-server lattice scheme (Spiral or a SimplePIR-family
successor, benchmarked not chosen; SealPIR stays a compatibility
baseline), and ZipPIR on the watchlist only (2026 publication,
insufficient independent review history for the shortlist). No Rust
code, no PIR crate, and no new dependency are added by this PR.
`LookupPrivacyClass::PrivatePIR` remains exactly as unimplemented and
gated behind D-0047 as it was before this decision.

**Reason:** the research report's own executive conclusion is explicit
that selecting a PIR scheme before freezing the database model, record
layout, and trust assumptions would reverse the correct design order —
a protocol optimized for a million fixed 256-byte records may be
unsuitable for a database with continuous updates and variable records,
and a scheme efficient on a cloud server may be unusable on a volunteer
node or old phone. The report's own closing recommendation states the
strongest immediate deliverable is "a research-only PR containing the
fixed workload, benchmark methodology, candidate shortlist, and
external-review questions" — this PR is exactly that, mirroring the
same narrowly-scoped-first-deliverable discipline already used for
D-0096 (KEL witness receipts, design-only) and D-0097 (bridge adapters,
doctrine-then-safety-boundary) earlier this session.

**Constitutional impact:** none. No dependency changes at all — this PR
adds only Markdown. Directive 14 (no new cryptography) is unaffected
since nothing here composes or invents a primitive; it names research
targets and release gates for a future, separately-reviewed decision.
No voice/value dependency is possible (this track has zero relationship
to `mini-value`/`mini-bounty`/`mini-treasury`). `mini-private-index`'s
STATUS.md claim that `PrivatePIR` is unimplemented and gated behind
D-0047 remains exactly true after this decision — this document only
makes that gate's opening criteria concrete (freeze workload → benchmark
whole-index and two-server baselines → benchmark one single-server
scheme → simulate updates/mobile clients → define replica-independence
and malicious-server threat models → external cryptographic review →
only then select one candidate for an out-of-process experimental
implementation).

**Implementation status:** research and review preparation only. No
Rust code in this PR. The next scoped deliverable is the whole-index-
download and two-server-PIR benchmark programme named in the design
doc's "Required before any PIR code PR" section — not started.

**Failure point:** this decision has no code failure point yet by
construction (nothing implemented). The design's own named failure
conditions to watch for once benchmarking starts: nominally independent
two-server replicas that actually share an operator, hosting provider,
or logs; a database namespace choice that itself identifies a user's
interest even though PIR hid the row; a PIR lookup immediately
correlated with a direct object fetch moments later; malformed queries
that exhaust server resources; and any future PR marketing an
unreviewed experimental implementation as production-private.

**Required follow-up:** build the whole-index-download baseline;
benchmark two-server information-theoretic PIR against real,
operator-diverse replica infrastructure; benchmark one mature
single-server scheme (not more); simulate database-update and
mobile-client costs, not just server throughput; write a replica-
independence policy and a malicious-server/malicious-client threat
model; commission the external cryptographic review named in D-0047;
only then select one candidate for an experimental, out-of-process
implementation (mirroring `mini-build-runner-wasmtime`'s sandboxing
precedent, D-0069).

**Supersedes / superseded by:** none. Extends D-0310's `mini-private-
index` doctrine additively — no existing type or behavior in any crate
changed.

### D-0099 — Anonymous resource payment and redemption preparation: online-spend blind-token doctrine, no code (`MN-602`/`MN-603`)  ·  *Accepted (research/doctrine only)*
**Date:** 2026-07-15 · **Refs:** founder-supplied `docs/research/
MN602_MN603_ANONYMOUS_RESOURCE_PAYMENT_RESEARCH_20260715.md`; `docs/design/
mn602-mn603-anonymous-resource-payment-preparation.md` (new);
`mini-resource-pricing` (D-0302, `MN-601`, unmodified); D-0098 (this
session's other research-only preparation decision); CLAUDE.md's
voice/value wall and no-new-cryptography rules

**Decision:** adopts the research report's own recommended doctrine
scope exactly: Mininet's priced privacy/resource services (relay, bridge,
mix, storage, cover traffic, private-index queries) must not be paid for
via an ordinary identity-linked MINI transfer per request, since that
turns the payment graph into a second metadata graph correlating payer,
privacy tier, provider, and timing. The adopted first-protocol shape is
online-spend, issuer-backed, fixed-denomination blind-signature resource
tokens with atomic spent-token checking and batched provider redemption
— not offline anonymous cash, not a new general currency, and not an
embedding of Privacy Pass/GNU Taler/Coconut, each of which is named as a
reference point for a specific later phase rather than adopted wholesale
(Privacy Pass's issuance/redemption separation for the first non-monetary
test-credit prototype; GNU Taler evaluated later as a possible external
monetary rail; Coconut reserved for later threshold-mint research,
MN-603B). Freezes five separable roles (funding source, token issuer,
client wallet, service provider, redemption service) and the hard rule
that subsidised and paid tokens must be indistinguishable at spend time.
Names, but does not create, three future crates
(`mini-resource-token`/`mini-resource-redemption`/`mini-resource-wallet`).
**No Rust code, no new crate, no blind-signature dependency.**
`mini-resource-pricing` (D-0302) is completely unmodified — it remains
pure quoting logic with no keys, no issuance, no transfers.

**Reason:** the research report's own executive conclusion states this
architecture prevents privacy-tier fingerprinting, timing linkage between
payment and service use, and provider-graph reconstruction that a direct
transfer would create — while online (rather than offline) redemption
avoids the identity-escrow double-spend-tracing complexity of classic
e-cash research, which this workspace is not positioned to design or
review safely today. Adopting the doctrine now, before any token type
exists, mirrors the same narrowly-scoped-first-deliverable discipline
already used for D-0096 (KEL witness receipts), D-0097 (bridge adapters),
and D-0098 (PIR research prep) earlier this session — freeze the
constraints a future implementation must satisfy before any code can
violate them by omission.

**Constitutional impact:** none negative; strengthens the voice/value
wall's applicability. Directive 16 (voice/value wall) is explicitly
extended to this future track: resource-token balances must never enter
vote calculations, review quorum, validator weight, personhood score,
witness selection, merge authority, or constitutional amendment, and no
crate that will eventually provide anonymous payment may be imported by
governance-counting code. Directive 14 (no new cryptography) is
reinforced — blind-signature/anonymous-credential schemes are named as
research targets requiring an external, already-reviewed implementation
(Phase 2+) and a separate external cryptographic review (Phase 6) before
any implementation proceeds past valueless test credits, and real MINI
may never back a token before that review plus separate accounting and
legal review (Phase 8-9) all complete. Personhood remains unsolved (see
`docs/INVARIANTS.md`'s hard-limitation section), so no subsidy mechanism
adopted under this doctrine may ever be represented as one-human-one-
share.

**Implementation status:** doctrine and research preparation only. No
Rust code in this PR. The next scoped deliverable is Phase 1 (non-
monetary test token types, no cryptographic blindness claim) — not
started.

**Failure point:** this decision has no code failure point yet by
construction (nothing implemented). The design's own named failure
conditions to watch for once implementation starts: withdrawal timing or
denomination patterns that uniquely identify a spend despite blind
issuance; an issuer that logs unblinded tokens; a provider that redeems
before the redemption service's atomic spent-check completes; a
subsidised token distinguishable from a paid one at presentation; a
wallet backup that enables an accepted duplicate redemption; and any
future PR letting resource-credit balances leak into a governance,
personhood, or validator-weight calculation.

**Required follow-up:** Phase 1 (test token types, denomination metadata,
mock issuance, wallet state, spent-set semantics — no blindness claim);
Phase 2 (real blind-issuance prototype behind one reviewed external
implementation, still valueless); Phase 3 (integration with one low-risk
resource — private-index query or a fixed relay byte bucket — still no
real settlement); Phase 4 (provider batch redemption); Phase 5
(adversarial simulation: timing correlation, replay races, provider/
issuer fraud, wallet rollback, subsidy farming); Phase 6 (external
cryptographic review); Phase 7 (closed valueless pilot); Phase 8
(economic/legal classification of what a credit actually is); Phase 9
(limited MINI-backed pilot, only after all of the above); Phase 10
(threshold-mint research, Coconut-style or otherwise).

**Supersedes / superseded by:** none. Extends D-0302's `mini-resource-
pricing` doctrine additively — no existing type or behavior in any crate
changed.
### D-0311 — Free public commons; payment purchases scarce protection, never speech (`MN-6xx`, public commons + protected publishing doctrine)  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
(Parts II, VII); D-0094 (cost doctrine this extends); `mini-privacy-
policy`; `mini-resource-pricing` (D-0302); `mini-social`; `mini-objects`;
`mini-relay` (D-0306); `mini-private-index` (D-0310); D-0099 (anonymous
resource-token doctrine, same session); money-never-buys-voice invariant
(Directive 16, voice/value wall)

**Decision:** public profiles may be viewed without payment. A person may
create and maintain a public profile and may publish, reply, comment,
react, search, and participate in ordinary public discussion without
paying the network merely for permission to speak, read, or be
discovered. Public operation is the commons path: a person choosing
public publication accepts that the content is intentionally disclosed
and may be served through ordinary peer-to-peer storage, caching,
indexing, replication, and bandwidth voluntarily contributed by
participants. Mininet charges only for additional resource-consuming
protection or service supplied by other participants — relay capacity,
metadata protection, source-hiding transport, mix routing, cover
traffic, private capability resolution, delayed/batched delivery,
geographically diverse storage, erasure-coded replication, prolonged
availability, suppression resistance, private retrieval, and other
measurable security/privacy/durability/availability mechanisms. Payment
purchases resources and a declared protection attempt; it never
purchases permission to speak, greater governance power, privileged
organic ranking, personhood, moderation authority, ownership of another
person's identity/data, or the right to discover a protected source.
Public users may voluntarily contribute bounded local resources — free
participation must never require unlimited storage, bandwidth, battery,
CPU, mobile data, or continuous availability, and contribution limits
must be visible, configurable, and revocable. For high-risk material, a
publisher may purchase a protection profile intended to make suppression
difficult and source attribution resistant; the system must minimize
source knowledge structurally (storage providers need not know the
author, transport participants must not learn the complete path, index
providers need not learn the searcher, and payment settlement must not
create a direct public link between payer, publisher, query, and
protected object). No tier may be described as guaranteeing absolute
anonymity or impossibility of suppression — every achieved result must
state the mechanisms used, resources purchased, duration/service bounds,
and residual risks, matching the "no absolute anonymous badge" rule
already established for `mini-privacy-policy` and D-0099's resource
tokens.

**Reason:** speech, reading, public discovery, and ordinary social
participation should not be paywalled — this is the founder's explicit
direction and matches Directive 16's money-never-buys-voice principle
applied to *reading and ordinary posting*, not just governance votes.
Strong privacy, anonymous transport, durable replication, and
suppression resistance consume measurable resources (bandwidth, storage
byte-time, mixing delay, jurisdictional diversity — the same cost
doctrine D-0094 already established) and should support an open
provider economy rather than being bundled into a mandatory access fee
that would exclude ordinary users from the commons.

**Constitutional impact:** strengthens equal participation and the
separation between money and voice (Directive 16). Money may purchase
measurable service capacity but never governance weight, legitimacy,
speech rights, personhood, or control over another person. This decision
does not touch any Tier-F frozen invariant — it constrains future
pricing/policy code to never introduce a paywall on frozen free-speech
protocol rights, which is a *new* constraint, not a weakening of an
existing one.

**Implementation status:** policy accepted, design only. No Rust code in
this decision. Existing Tier 0 direct-transport policy already models
unpaid ordinary operation; higher tiers already model paid relay/mix/
replication as policy data (`mini-privacy-policy`, `mini-resource-
pricing`, D-0302). `PublicCommonsPolicy`, wallet-independent public
entitlements, bounded opt-in contribution budgets, provider settlement,
anonymous payment separation (see D-0099), and production transport
remain to be implemented (Tracks C/D of the source document).

**Failure point:** this decision fails if free operation becomes a
covert mandatory resource tax on ordinary users; if paid placement
becomes political or social power; if payment metadata identifies
protected publishers or searchers; if one provider can correlate source,
destination, content, and payment; or if Mininet markets bounded
protection as guaranteed anonymity.

**Required follow-up:** define `PublicCommonsPolicy` (free public
actions independent of wallet balance); define opt-in bounded resource-
contribution budgets; price only incremental external resources beyond
the free commons path; add protected-publication and private-search
receipts; prove balances cannot alter governance or ordinary public
rights (adversarial tests); threat-model timing/payment/entry/storage/
search/retrieval correlation across the full path; add clear UI language
distinguishing public, private, anonymous, and suppression-resistant
modes. Tracked as Track C (`PublicCommonsPolicy` + contribution budgets)
and Track D (protected publishing: publication-profile dimensions,
protection quotes, source-hiding path, mixed transport, suppression-
resistant replication, unlinkable settlement) of the source document.

**Supersedes / superseded by:** none. Clarifies that no earlier wording
implies all publishing, storage, or social activity necessarily requires
payment. Does not supersede the privacy-tier model (D-0094's cost
doctrine, `mini-privacy-policy`) — it defines Tier 0 as the free public
commons and higher tiers as incremental paid service, which is
consistent with, not a change to, the existing tier definitions.

### D-0312 — Independent, transparent, pluralistic open-web search (`MiniSearch` doctrine, `MN-7xx`)  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
(Part III); D-0311 (public commons — search is a free commons
entitlement); D-0310 (`mini-private-index`, explicitly a different
system from the public web index this decision creates); Directive 16
(voice/value wall, extended here to search-ranking authority)

**Decision:** Mininet builds and operates its own independent web
crawler, index, and search protocol suite, working name **MiniSearch** —
not a reimplementation of any proprietary search engine, but restoring
broad discovery, direct links to independent sites, visible relevance,
minimal manipulation, explicit query operators, and source diversity that
concentrated commercial search has eroded. "Unfiltered and uncensored" is
adopted as a precise, bounded claim, not a marketing absolute: no secret
political allow/block list, no payment-based organic ranking, no hidden
demotion for commercial advantage, no forced ideological personalization,
no single authority controlling the global index, no silent removal
without a reason code, no pretending an incomplete index is complete —
paired with an explicit acknowledgment that indexing every byte on the
internet, ignoring applicable law, serving malware unwarned, or
displaying unlawful harmful material is never in scope. Retrieval
relevance, spam/manipulation assessment, malware/technical-risk
assessment, user-selected content filters, jurisdictional availability,
and local device policy are kept as five independently inspectable
layers — a restricted result must never be silently folded into a lower
relevance score; it must carry an explicit `AvailabilityState` reason.
Ranking is a versioned, declared-weight `RankingProfile` (lexical/
phrase/link/freshness/originality/diversity, `PersonalizationPolicy::
None` by default) rather than an unreviewable opaque model as sole
authority. The system is architected for plurality by construction:
multiple independently built index segments from shared crawl
observations, multiple selectable/forkable ranking profiles, federated
query merging across independent providers, and local client re-ranking
— so MiniSearch cannot itself become a second search monopoly. Ordinary
public search is a free commons entitlement (D-0311); users may
additionally pay for query relay, mix-routed queries, private
information retrieval, or other privacy-preserving retrieval transport,
but search providers must never receive a receipt publicly linking
identity, query, and result selection, and paid protection must never
alter organic ranking. `mini-private-index` (D-0310) remains a distinct
system for private capability resolution and is explicitly not to be
merged with or reused as the general public web index.

**Reason:** the founder's explicit direction is that search infrastructure
concentrated in one company's hands became an invisible governor of what
society can discover, and that restoring pre-concentration search
properties (broad crawling, high recall, visible relevance, minimal
manipulation) requires an index nobody can unilaterally control. Keeping
ranking, safety, legality, and personalization as separable, inspectable,
versioned layers is the only way to make "uncensored" an honest,
falsifiable claim rather than a slogan — the same honesty discipline this
repository already applies to privacy tiers (`mini-privacy-policy`) and
personhood (`HumanStatus`, D-0086).

**Constitutional impact:** extends Directive 16's voice/value wall into a
new domain: search-ranking authority must never be purchasable, and
provider identity/hardware spend must never grant ranking authority
(reinforced explicitly for crawler/index providers in the source
document's Track F). No Tier-F frozen invariant is touched; this decision
adds a new constraint on all future search-ranking code, symmetrical to
the existing constraint that MINI balances never buy governance weight.

**Implementation status:** policy accepted, design only. No Rust code in
this decision. `mini-web-types`, `mini-crawler`, `mini-web-extract`,
`mini-index`, `mini-ranker`, `mini-query`, `mini-search-service`,
`mini-search-ui`, and the federated/distributed search layer (Track F)
are all future work — none exist yet.

**Failure point:** this decision fails if any ranking signal becomes
purchasable; if restriction notices are silently converted into
relevance penalties instead of an explicit `AvailabilityState`; if
personalization becomes mandatory or opt-out rather than opt-in and
local-by-default; if one index or one ranker becomes a de facto protocol
requirement; if crawl-observation rewards can be claimed without
verifiable work, letting a wealthy operator buy ranking influence via
raw hardware spend; or if "uncensored" is claimed without the explicit,
narrow scope this decision defines.

**Required follow-up:** Track E (search doctrine/threat-model docs;
`mini-web-types`; a minimal single-host crawler with strict limits, no
JavaScript; sandboxed static-page extraction; a deterministic lexical
index with immutable signed segments; a transparent versioned ranker;
a query CLI with exact/site/date/language/type operators; result
provenance and `RankingExplanation` types) and Track F (signed crawl-
observation exchange, content-addressed index segments, federated query
merging, local re-ranking, verifiable-work provider payments, private
query transport wired to `mini-relay`/mix tiers, historical snapshot
retention) — a substantial, multi-PR body of original engineering, none
started by this decision.

**Supersedes / superseded by:** none. New doctrine only; touches no
existing crate. Explicitly clarifies that `mini-private-index` (D-0310)
is not to be repurposed as the general public web index this decision
describes.
### D-0313 — `mini-intake-types`: shared Mininet Intake vocabulary, Track B1 of the native-intake direction  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
Part V (Track B PR sequence); D-0311/D-0312 (the adjacent public-commons
and open-web-search doctrine this track feeds); CLAUDE.md's typed-domains
rule; Directive 14 (no new cryptography)

**Decision:** adds a new crate, `mini-intake-types`, holding only the
shared vocabulary for Mininet Intake: `IntakeId` (wrapping the existing
`mini_crypto::Multihash` — no new digest type), `SourceRecord`,
`MediaType`, `DerivedRepresentation`/`DerivationRecord`/
`GeneratorIdentity`/`RepresentationKind`, the ordered six-tier
`AuthorityClass` taxonomy (`UntrustedExternal` → `CanonicalProjectMaterial`),
the `ReviewState` lifecycle state machine (`allows_transition_to` names
every legal transition; `Rejected`/`Superseded` are terminal), `IntakeLink`,
`IntakeWarning`, and the top-level `IntakeEnvelope` tying them together
with a deterministic, length-prefixed wire codec matching `mini-relay`/
`mini-bridge`/`mini-private-index`'s existing discipline. The research
report's core rule — imported material receives no project authority
merely because Mininet can parse it — is enforced structurally, not just
documented: `IntakeEnvelope::new` always starts at `ReviewState::Unreviewed`
and `AuthorityClass::UntrustedExternal` by construction (private fields,
no alternate constructor), and `promote_authority` rejects reaching
`AuthorityClass::ReviewedEvidence` or higher unless `review_state` is
already `ReviewState::Accepted`. Designed clean-room, independent of and
with no dependency on any external licensed intake tool, per the research
report's own non-negotiable §2.1 rule. 35 unit tests, including exhaustive
review-transition and authority-promotion coverage and a truncation-fuzz
test looping over every possible truncated byte length asserting `Err`
rather than a panic.

**Reason:** the research report's own Track B sequencing names
`mini-intake-types` (pure types, no parser/filesystem/network/AI) as the
correct first slice — the same narrowly-scoped-first-deliverable
discipline already used for `mini-bridge`/`mini-private-index`
(D-0309/D-0310) and the ML-DSA-65 verify-only slice (D-0095) this
session. Shipping the vocabulary first, with the "no automatic authority
promotion" rule enforced in the type system rather than left to caller
discipline, lets every later Track B/C/D crate (`mini-intake` the trusted
coordinator, the extractor protocol, publication linking) build on a
boundary that cannot silently be bypassed by a future caller forgetting a
check.

**Constitutional impact:** none negative. No new cryptography (Directive
14) — `IntakeId`/`SourceRecord` carry a `mini_crypto::Multihash` a caller
already computed; this crate performs no hashing itself. No voice/value
wall implications — this crate has no dependency on `mini-value`/
`mini-bounty`/`mini-treasury` or on `mini-forge`/`mini-chain` voting.
Typed-domains rule honored: `IntakeEnvelope`'s only mutation paths are
`advance_review_state`/`promote_authority`, both taking specific typed
arguments and both fallible against a named rule, not a generic
`sign(bytes)`/`finalize(state)` shape.

**Implementation status:** shipped in `mini-intake-types` only, added to
the workspace `members` list. `cargo fmt --all`, `cargo clippy
--all-targets --all-features --workspace -- -D warnings`, and `cargo test
--workspace --all-features` all clean, including this new crate's 35
tests.

**Failure point:** vocabulary only — there is no working intake pipeline
yet. No hashing, no filesystem watcher, no extractor, no AI model, no
storage of represented bytes, no way to actually construct an
`IntakeEnvelope` from a real external document today. `AuthorityClass`
and `ReviewState` are honor-system inputs from whatever caller eventually
drives them (`mini-intake`, Track B2) — this crate only guarantees that
*given* a caller correctly reporting review outcomes, authority cannot be
promoted out of order; it cannot stop a Track B2 coordinator from lying
about a review outcome it never actually ran.

**Required follow-up:** Track B2 `mini-intake` (trusted intake
coordinator: hashing, immutable storage, dedup, local text/Markdown
intake, atomic object creation); Track B3 (extractor protocol + isolated
host, mirroring `mini-build-runner-wasmtime`'s sandboxing discipline);
Track B4 (PDF/HTML extraction backends); Track B5 (intake publication
linking). See `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
Part V for the full sequence.

**Supersedes / superseded by:** none. New crate, no existing crate
touched.

### D-0314 — Split the SPEC-11 reproducibility CI check into its own path-scoped workflow; add `merge=union` for the append-only decision/status logs  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** SPEC-11 (verified-reproducible releases
\[FREEZE\]), D-0001/D-0006 (reproducibility hygiene), D-0044 (#69, the
original reproducibility CI check); `.github/workflows/ci.yml`,
`.github/workflows/reproducibility.yml` (new), `.gitattributes` (new)

**Decision:** two changes to reduce PR friction without touching any
frozen guarantee. (1) The `reproducibility` job (two full clean `cargo
build --release` passes + a hash diff, 10+ minutes) moves out of
`ci.yml` into its own `.github/workflows/reproducibility.yml`. Its
`push: branches: [main]` trigger is unchanged and unfiltered — every
commit reaching `main` is still checked, matching SPEC-11's actual scope
("verified-reproducible *releases*"). Its `pull_request` trigger gains a
narrow `paths-ignore: [docs/**, **/*.md]`: a PR that touches *only*
documentation cannot change a build artifact, so skipping the check for
it loses no coverage; a PR touching any other path (source, `Cargo.toml`/
`Cargo.lock`, `rust-toolchain.toml`, build scripts, or the workflow file
itself) still runs the real check before merge, since `paths-ignore`
only skips when *every* changed path matches. The job keeps its exact
name (`reproducibility`) so any existing branch-protection required-check
rule keyed on that name still matches. (2) `.gitattributes` adds
`merge=union` (a built-in Git driver, no external tool) for
`docs/DECISION_LOG.md` and `docs/STATUS.md`: both files are append-heavy
by their own documented convention (DECISION_LOG.md entries are never
edited, only superseded; STATUS.md is "updated far more often than any
individual decision entry"), so two branches adding different, adjacent
entries is a routine, non-semantic conflict that a line-union resolves
correctly, rather than forcing manual conflict-marker resolution on every
rebase of a stacked PR.

**Reason:** this session hit the same two costs repeatedly on legitimate,
narrowly-scoped doctrine/research PRs (#150-#153): a 10+ minute
reproducibility rebuild on PRs that touched zero build-relevant files,
and a manual `<<<<<<< HEAD` resolution on `docs/DECISION_LOG.md`/
`docs/STATUS.md` on nearly every rebase past a just-merged sibling PR,
purely because both sides appended near the same location. Neither cost
buys any additional assurance — a docs-only diff cannot regress build
reproducibility, and an append-only log's insertion order across two
independently-authored entries is not itself meaningful. Fixing the
underlying friction is preferable to disabling the check (which was
raised and rejected: SPEC-11 reproducibility is Tier-F frozen, and
weakening its enforcement needs the formal unfreezing process, not a
chat-direction disable).

**Constitutional impact:** none negative. The SPEC-11 frozen requirement
is enforced exactly as strictly as before for every commit that can
possibly matter (all of `main`, and every PR touching non-doc paths) —
this is a *scheduling* change, not a weakening of what gets checked or
what passing means. `merge=union` never suppresses a real same-line edit
conflict (there are none possible in an append-only log by construction)
and never touches any other file's merge behavior.

**Implementation status:** shipped. `.github/workflows/reproducibility.yml`
carries the exact job body that previously lived in `ci.yml` unchanged;
`ci.yml` retains `check`/`dependency-audit`/`dependency-deny` exactly as
before. `.gitattributes` is new.

**Failure point:** `paths-ignore` is a blunt, path-based proxy — a PR
that edits only `docs/**`/`*.md` cannot regress reproducibility by
construction, so there's no known false-negative case, but if a future
non-Rust file *were* to ever affect the release build (unlikely given
this workspace's structure) it would need adding to the ignore list's
complement, i.e. the ignore list should stay narrow, not grow. `merge=union`
resolution is not reviewed by a human before the merge commit forms — a
malformed union (e.g. two entries claiming the same D-number, which the
project's own collision-avoidance discipline is supposed to prevent
independently) would still need to be caught by review, same as any
other merge.

**Required follow-up:** none required; watch whether `paths-ignore`'s
two patterns need widening (e.g. to cover a future non-code,
non-doc path) as the repo's shape changes.

**Supersedes / superseded by:** none. Additive CI/repo-config change;
`reproducibility`'s own build/hash logic is byte-for-byte unchanged from
D-0044.
### D-0100 — Bootstrap work-claim registry for parallel AI contributors  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** #122; `governance/work-claims.json`;
`docs/governance/51_BOOTSTRAP_WORK_CLAIMS.md`; D-0083; D-0084; Directive 12

**Decision:** add a machine-readable bootstrap work-claim registry and validator
checks for active issue leases, Decision identifier reservations, path-claim
overlap, and lease expiry. The GitHub Project remains the dashboard, but
`governance/work-claims.json` is the CI-enforced branch artifact.

**Reason:** the previous banding rule reduced cross-track Decision-number
collisions but did not prevent same-band or same-issue collisions between
parallel AI contributors. A JSON registry plus CI makes the coordination state
reviewable and testable instead of relying on private chat or model memory.

**Constitutional impact:** strengthens Directive 12 and AI1 by keeping AI
collaboration attributable and non-authorizing. No new authority is granted:
a claim is coordination evidence only, not approval, review, merge,
canonicalization, or legitimacy.

**Implementation status:** implemented in `tools/check_governance.py`,
`tools/work_claims.py`, `governance/work-claims.json`, and
`governance/work-claims.schema.json`; adversarial validator tests cover duplicate
active Decision IDs, duplicate active issue claims, expired active leases, and
overlapping active path claims.

**Failure point:** the registry is Git-based bootstrap coordination, not a
distributed lock across unmerged branches. It catches conflicts before merge and
in CI; a future Forge-native allocator should replace it with a signed proposal
object and atomic allocation state.

**Required follow-up:** wire PR metadata enforcement so every implementation PR
names its claim automatically, and replace this GitHub-era registry with a
Forge-native proposal allocator during the Forge transition.

**Supersedes / superseded by:** supplements the Decision-number banding rule in
the Decision Log header; superseded by a future Forge-native allocator.
### D-0316 — `mini-web-types`: shared MiniSearch vocabulary, Track E foundation  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** D-0312 (MiniSearch doctrine);
D-0311 (free public commons); founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
Part III and §14.1 (`mini-web-types`); Directive 16 (voice/value wall)

**Decision:** adds `mini-web-types`, a pure shared-vocabulary crate for
MiniSearch. It defines content-addressed identifiers (`UrlId`,
`CrawlObservationId`, `IndexSegmentId`, `RankingProfileId`), validated
`CanonicalUrl`/`NormalizedHost` records, `CrawlObservation` and
`FetchStatus`, `AvailabilityState` with explicit `UnavailabilityReason`
and `RestrictionReason`, deterministic integer `WeightBps`, versioned
`RankingProfile`, `PersonalizationPolicy` whose public default is
`None`, `SearchResult`, and `RankingExplanation`.

The crate deliberately has no crawler, fetcher, parser, index, ranker,
query service, network client, payment logic, governance dependency, or
value dependency. It is the typed boundary future MiniSearch crates must
share before any one crawler/index/ranker implementation exists.

**Reason:** D-0312's central engineering rule is separability:
discovery, relevance ranking, availability restrictions, user filters,
and personalization must not collapse into one opaque score. Shipping the
vocabulary first makes that rule concrete. A restricted result is carried
as an explicit `AvailabilityState::Restricted(RestrictionReason)` rather
than silently hidden as a lower relevance score; public ranking profiles
default to `PersonalizationPolicy::None`; ranking weights are declared as
bounded integer basis points, not opaque floating-point model output or
provider/payment authority.

**Constitutional impact:** strengthens Directive 16 in the search domain:
there is no balance, stake, payment, provider-revenue, or governance
weight field anywhere in `RankingProfile` or `SearchResult`. Search
ranking authority remains declared, inspectable, forkable, and separate
from payment. No new cryptography is introduced; identifiers wrap the
existing `mini_crypto::Multihash` type only.

**Implementation status:** shipped in `mini-web-types` and added to the
workspace. Focused local validation on Windows: `cargo fmt --all
-- --check`, `cargo test -p mini-web-types --all-features`, and
`cargo clippy -p mini-web-types --all-targets --all-features --
-D warnings` pass. The broader workspace check still hits the known
Windows-only `mini-installer` Unix symlink compile gap and is not evidence
against this crate.

**Failure point:** vocabulary only. No URL parser from arbitrary text, no
robots fetcher, no HTTP client, no crawler frontier, no extractor, no
index segment format, no ranker implementation, no query CLI, no
federated merge protocol, no private-query transport, and no provider
reward/receipt system exist here. `CanonicalUrl::new` validates already
separated URL parts; it is not a full browser-compatible URL parser.

**Required follow-up:** minimal single-host crawler with strict limits and
no JavaScript; sandboxed static-page extraction; deterministic lexical
index segments; transparent versioned ranker; query CLI; result
provenance; federated query merging; local re-ranking; and signed crawl
observation exchange as separate PRs.

**Supersedes / superseded by:** implements the first code slice named by
D-0312's Track E follow-up. Does not supersede `mini-private-index`
(D-0310), which remains private capability lookup, not public web search.
### D-0315 — `mini-intake`: trusted intake coordinator, Track B2 of the native-intake direction  ·  *Accepted*
**Date:** 2026-07-18 · **Refs:** founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
Part V (Track B PR sequence); D-0313 (`mini-intake-types`, the vocabulary
this crate drives); `mini-store` (the `Backend` trait this crate
composes)

**Decision:** adds a new crate, `mini-intake`, that actually drives
`mini-intake-types`' vocabulary: `intake_local_file` reads one local
text/Markdown file, computes its `BLAKE3` digest (`mini_crypto`, no new
cryptography), stores the immutable source bytes, and creates a fresh
`Unreviewed`/`UntrustedExternal` `IntakeEnvelope` — or, on a dedup hit
(byte-identical content already intaken, detected by content digest, not
path), returns the *existing* envelope completely untouched. `load_envelope`/
`read_source_bytes` read back what's stored; `save_envelope` persists a
caller-mutated envelope (e.g. after a separate, later
`advance_review_state`/`promote_authority` call) — this crate never calls
either on a caller's behalf. Storage composes `mini_store::Backend`
(`MemoryBackend`/`FsBackend`) — the plain content-addressed blob/meta
abstraction — rather than `mini_store::Store`/`mini_objects::Object`,
since intake material has no `did:mini` signature at ingest time and
`Store` assumes self-certifying signed objects. Media-type detection is
extension-based and deliberately narrow: `.txt`/extensionless → plain
text, `.md`/`.markdown` → Markdown, anything else is a hard
`UnsupportedMediaType` error rather than a guess (PDF/HTML/etc. is Track
B3/B4); bytes that fail UTF-8 validation are rejected regardless of
extension. 13 tests, including a dedup test proving re-intaking
identical content after a caller advanced its review state and authority
class returns the *advanced* envelope, not a fresh reset one, and a real
`FsBackend` round-trip test proving persistence survives closing and
reopening the backend.

**Reason:** the research report's own Track B sequencing names PR B2 as
hashing, immutable storage, deduplication, local text/Markdown intake,
atomic object creation, and — explicitly — no automatic authority
promotion. Composing `mini-store::Backend` rather than writing a parallel
blob-storage primitive from scratch avoids duplicating already-solved,
already-tested atomic-write/content-addressing plumbing (`FsBackend`'s
tmp-file-then-rename discipline), while deliberately not depending on
`mini-store::Store`/`mini_objects` object semantics, which are the wrong
trust model for bytes with no Mininet identity behind them yet.

**Constitutional impact:** none negative. No new cryptography (Directive
14) — reuses `mini_crypto::Multihash`/`HashAlgorithm` and the existing
multibase encoding helper, exactly as `mini_objects::ObjectId` already
does, for backend key derivation. No voice/value wall implications — no
dependency on `mini-value`/`mini-bounty`/`mini-treasury` or on
`mini-forge`/`mini-chain` voting. Extends D-0313's structural
no-automatic-authority-promotion guarantee with its practical
consequence discovered while implementing this crate: a dedup hit must
also never *demote* an already-advanced envelope back to `Unreviewed`/
`UntrustedExternal` — tested explicitly
(`a_dedup_hit_never_resets_an_already_advanced_review_state`).

**Implementation status:** shipped in `mini-intake` only, added to the
workspace `members` list. `cargo fmt --all`, `cargo clippy --all-targets
--all-features --workspace -- -D warnings`, and `cargo test --workspace
--all-features` all clean, including this new crate's 13 tests.

**Failure point:** local text/Markdown only — no PDF/HTML/binary support,
no extractor, no AI model, no network client, no publication linking. No
cross-process locking: concurrent intake calls against the same `FsBackend`
directory from two processes are not coordinated (same documented
limitation `mini-store::FsBackend` itself already carries). Atomicity is
per-write, not cross-file-transactional — a crash between writing the
source blob and writing the envelope leaves a resumable, not corrupted,
state (documented in the crate's own doc comments), but this has not been
tested under an actual simulated crash/kill, only reasoned about.
`AuthorityClass`/`ReviewState` remain honor-system beyond this crate's own
guarantees: nothing here verifies that a caller's `advance_review_state`
call reflects a review that actually happened.

**Required follow-up:** Track B3 (extractor protocol + isolated host,
mirroring `mini-build-runner-wasmtime`'s sandboxing discipline); Track B4
(PDF/HTML extraction backends, after license/security review); Track B5
(intake publication linking). A crash-recovery test (kill mid-write,
verify resumability) is a reasonable next hardening step before Track B2
is relied on for anything beyond local single-process use.

**Supersedes / superseded by:** none. New crate; composes `mini-store`
and `mini-intake-types` additively, no existing crate's behavior changed.

### D-0317 — `mini-crawler`: deterministic MiniSearch crawler planning and URL admission, Track E2  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** D-0312 (MiniSearch doctrine);
D-0316 (`mini-web-types`); issue #161; founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
§14.2 and Track E PR sequence; Directive 16 (voice/value wall)

**Decision:** adds `mini-crawler`, the second MiniSearch code slice after
`mini-web-types`. The crate implements deterministic crawler planning and
URL admission policy only: bounded `CrawlLimits`, explicit
`CrawlExclusions`, `CrawlRequest`, `CrawlAdmission`, named
`CrawlRejectReason` values, and a FIFO `CrawlPlan` that tracks canonical
URL identity with the `CanonicalUrl::canonical_string()` representation
from `mini-web-types`. The default plan is single-host and HTTPS-only,
rejects cross-host discoveries, depth overrun, queue/seen exhaustion,
overlong canonical URLs, duplicate canonical URLs, and caller-supplied
robots exclusions before any fetch can occur.

The crate deliberately has no network client, DNS lookup, robots fetcher,
JavaScript execution, HTML parser, storage, indexing, ranking, payment,
provider reward, governance, or value dependency. It is an admission core
for a later runtime, not a crawler runtime.

**Reason:** D-0312 names a minimal crawler as the next Track E slice, but
fetching the public web before deterministic admission rules would bake
abuse risk and platform-shaped behavior into the wrong layer. Shipping
the policy core first makes the future runtime testable: the decision to
fetch is bounded and explainable before any network side effect happens.
Explicit rejection reasons also preserve D-0312's separation between
discovery, availability, and ranking — a robots or policy exclusion is a
crawler-layer fact, not a hidden relevance penalty.

**Constitutional impact:** strengthens the search-domain extension of
Directive 16 without adding authority. `mini-crawler` contains no
payment, stake, balance, governance-weight, or provider-entitlement
field; crawler admission cannot buy ranking authority and cannot approve
or canonicalize content. No new cryptography is introduced.

**Implementation status:** shipped in `mini-crawler` and added to the
workspace. Focused local validation on Windows: `cargo fmt --all
-- --check` and `cargo test -p mini-crawler --all-features` pass, with
tests covering empty/invalid plans, deterministic seed order and
canonical duplicate handling, HTTPS-only default behavior, cross-host
rejection, depth and queue limits, robots exclusions, canonical URL byte
limits, and explicit HTTP opt-in.

**Failure point:** planning only. No page is fetched, parsed, stored,
indexed, ranked, queried, federated, paid for, or published by this
crate. Robots exclusions are caller-supplied policy inputs; this crate
does not download or interpret `robots.txt`. The same-host default is
deliberate for the first runtime slice and is not a full web-scale
frontier.

**Required follow-up:** a runtime that uses this policy before fetch;
robots.txt retrieval/parsing as a separate bounded module; static HTML/
text extraction in a sandboxed process; immutable crawl observations;
content-addressed index segments; transparent ranker; query CLI; and
federated/distributed query merging as later Track E/F PRs.

**Supersedes / superseded by:** implements the minimal crawler-planning
slice named by D-0312 after D-0316. Does not supersede `mini-web-types`
or `mini-private-index`.

### D-0318 — `mini-installer`: gate `std::os::unix::fs::symlink` behind `#[cfg(unix)]` so the crate compiles on non-Unix hosts  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** issue #167; `crates/mini-installer/src/lib.rs`
(`swap_current`); D-0071/D-0106-D-0108 (`mini-installer`'s original
type-state pipeline and its already-documented "Unix-only" honest
limit)

**Decision:** `Installer::swap_current`'s atomic pointer-flip called
`std::os::unix::fs::symlink` with no platform guard, which does not
exist outside `#[cfg(unix)]` targets -- a hard *compile* error for the
whole crate, and everything depending on it (`mini-cli`), on any
non-Unix host. Extracts the one platform-specific line into a small
`create_symlink` helper: `#[cfg(unix)]` calls the real
`std::os::unix::fs::symlink` unchanged; `#[cfg(not(unix))]` returns a
`std::io::Error` with `ErrorKind::Unsupported` and a clear message,
propagated through the crate's existing `InstallerError::Io` variant --
no new error variant needed. The module doc's already-honest "Unix-only"
limitation is reworded to distinguish *compiling* (now every platform)
from *activating* (still Unix-only, unchanged).

**Reason:** two independent open PRs (#165, #166) both hit this exact
wall from a Windows host and had to note their full `mini-cli` test run
was blocked by it, unable to exercise their own changes locally even
though the change itself had nothing to do with `mini-installer`.
Workspace CI itself only runs `ubuntu-latest`
(`.github/workflows/ci.yml`), so this never blocked the merge gate
directly, but it silently blocked every non-Unix-hosted contributor
(human or AI agent) from building or testing the workspace at all --
a real production-readiness gap nothing in `docs/STATUS.md` or the
crate's own docs previously named as a workspace-wide limitation (only
`mini-installer`'s *activation* being Unix-only was ever claimed, not
the crate failing to build elsewhere).

**Constitutional impact:** none. This is a compile-target fix only --
`mini-installer`'s actual capability is completely unchanged: activation
was, and remains, Unix-only; no new platform support, no new capability,
no touched frozen invariant. Directive 14 (no new cryptography) and the
typed-domains rule are both untouched -- `InstallerError::Io` already
existed and already carried arbitrary `std::io::Error` values.

**Implementation status:** shipped in `mini-installer` only.
`cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features
--workspace -- -D warnings`, and `cargo test --workspace --all-features`
all clean on this (Linux) host, which continues to exercise the real
`#[cfg(unix)]` path exactly as before -- this PR could not itself run
the `#[cfg(not(unix))]` branch to prove it compiles cleanly on Windows,
since no Windows toolchain exists in this environment; the branch is
a five-line, trivially-inspectable `std::io::Error` construction with no
platform-specific API surface, and the fix is scoped exactly to the line
the two blocked PRs both named.

**Failure point:** does not add real Windows (or any non-Unix) install
support -- `Installer::activate` still cannot succeed there, it now just
fails at runtime with a clear error instead of preventing the crate from
being built at all. No Windows CI runner exists in this workspace to
continuously verify the `#[cfg(not(unix))]` branch keeps compiling as
the crate evolves; a future edit to `swap_current` could silently
reintroduce a Unix-only construct elsewhere in the same function without
any check catching it before a non-Unix contributor hits it again.

**Required follow-up:** none required for this fix's own scope. Adding a
`windows-latest` compile-only (not full-test) CI leg would catch
regressions of this exact class going forward; not done here since it's
outside this issue's named scope (#167 is about the one existing compile
failure, not about establishing new CI infrastructure).

**Supersedes / superseded by:** none. Narrow compile-target fix; no
existing crate behavior changed on the Unix targets this crate was
already built and tested against.
### D-0319 — `mini-extract-protocol` + `mini-extract-host`: isolated extractor protocol and process host, Track B3 of the native-intake direction  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** founder-supplied `docs/research/
MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
Part V (Track B PR sequence); D-0313 (`mini-intake-types`, Track B1);
D-0315 (`mini-intake`, Track B2); D-0069 (`mini-pipeline-protocol`/
`mini-build-runner-wasmtime`, the isolated-runner discipline this PR
mirrors); issue #169

**Decision:** adds two new crates. `mini-extract-protocol` is a pure
wire-format crate -- no I/O, no process-spawning, no filesystem access --
defining a length-delimited, size-bounded request/result protocol
between an intake coordinator and an isolated extractor worker:
`ExtractionRequest` (an `ExtractorKind`, raw `source_bytes`, and
`ResourceLimits`), `ExtractionOutcome` (either `ExtractionSuccess` or a
specific, structured `ExtractionError` -- `Timeout`/`OutputTooLarge`/
`MalformedInput`/`ExtractorCrashed`/`Io`/`UnsupportedExtractorKind`,
never a generic failure), and `read_framed`/`write_framed` framing.
Hand-rolled rather than depending on `mini-pipeline-protocol` despite an
identical wire shape for the framing helpers -- the same choice
`mini-intake-types`/`mini-relay`/`mini-bridge`/`mini-private-index`
already made, keeping each isolated-process protocol crate free of a
cross-domain dependency edge to an unrelated subsystem.

`mini-extract-host` is the isolated host: `run_worker` spawns the
compiled `mini-extract-worker` binary (this crate's own `[[bin]]`
target) as a genuine child process, writes a framed request to its
stdin from a writer thread, and reads a framed result from its stdout
on a reader thread bounded by `mpsc::channel::recv_timeout` against
`ResourceLimits::max_wall_clock_ms` -- exactly the spawn/thread/
mpsc-timeout pattern `mini-cli build run` already uses to talk to
`mini-build-runner-wasmtime` (`crates/mini-cli/src/build.rs`). A missed
deadline kills the child and reports `ExtractionError::Timeout`; a
worker that exits without writing a result frame reports
`ExtractorCrashed { exit_code }`; a result frame declaring more than the
request's own `max_output_bytes` reports `OutputTooLarge` before the
host allocates to read it. `HostError` is reserved for failures meaning
the exchange itself never happened (the binary is missing, an unrelated
I/O failure) -- a worker that behaved badly but still spoke the protocol
always comes back `Ok(ExtractionOutcome::Err(..))`, never `Err`.

The one simple extractor Track B3 asks for: `ExtractorKind::
PlainTextNormalize` lossy-UTF-8-decodes `source_bytes`, strips control
characters other than tab/newline, and collapses horizontal whitespace
runs -- deliberately trivial, proving the isolation host works
end-to-end before Track B4's real PDF/HTML parsers (a far larger,
historically-exploited attack surface) are wired in behind their own
licence/security review. `ExtractorKind` is `#[non_exhaustive]`; the
worker's own dispatch match has an explicit wildcard arm returning
`UnsupportedExtractorKind` rather than a compile error or silent
fallthrough, so a request naming a kind this binary predates fails
cleanly and specifically. 30 tests total: 17 protocol round-trip/
truncation/oversize-rejection tests (`mini-extract-protocol`), 5 unit
tests for the plain-text extractor itself, and 8 adversarial/integration
tests spawning the real compiled worker binary (successful extraction,
empty input, invalid UTF-8 lossy-decoded not rejected, output-too-large,
a zero-millisecond deadline reported as timeout, a missing binary
reported as `HostError::Spawn` not a panic, a process that exits without
a result frame reported as `ExtractorCrashed` via the real `true`
binary, and two concurrent extractions not interfering).

**Reason:** the research report's own Track B sequencing names PR B3 as
"isolated worker protocol; resource limits; structured errors; one
simple extractor; adversarial tests" -- exactly this PR's scope, no
more. Mirroring `mini-pipeline-protocol`/`mini-build-runner-wasmtime`'s
already-reviewed process-spawn/framing/timeout discipline rather than
inventing a new isolation mechanism is the same "prefer the smaller,
well-trodden construction" reasoning (Directive 14) already applied to
D-0069 itself; the self-hosted forge spine's own CLI (`mini build run`)
is real, working, adversarially-tested proof this exact pattern holds up
under a hostile/misbehaving child process.

**Constitutional impact:** none negative. No new cryptography (Directive
14). No voice/value wall implications -- no dependency on `mini-value`/
`mini-bounty`/`mini-treasury` or on `mini-forge`/`mini-chain` voting.
Typed-domains rule honored: the worker's only externally-driven action
(running an extractor over bytes) is dispatched through
`ExtractorKind`, a closed-but-extensible named enum, never a generic
`run(bytes)` the caller could redirect to arbitrary logic.
`mini-build-runner-wasmtime` remains the only crate in this tree
permitted to link Wasmtime -- `mini-extract-host` has no Wasmtime
dependency and never will; its isolation is OS-process-boundary only,
honestly scoped as weaker than Wasmtime's deny-by-default capability
sandbox in the crate's own "Honest limits" doc section.

**Implementation status:** shipped in `mini-extract-protocol` and
`mini-extract-host` only, both added to the workspace `members` list.
`cargo fmt --all`, `cargo clippy --all-targets --all-features
--workspace -- -D warnings`, and `cargo test --workspace --all-features`
all clean, including these two new crates' 30 tests.

**Failure point:** process-boundary isolation only, not Wasmtime-grade
sandboxing -- no seccomp-bpf, no restricted syscalls, no network or
filesystem denial beyond what the OS grants any ordinary child process
of the same user; a malicious "extractor" logic given enough CPU time
before its wall-clock kill could still, in principle, attempt anything
an unprivileged process on the host can. One built-in extractor only,
and it never rejects input as malformed (lossy UTF-8 decoding always
succeeds), so `ExtractionError::MalformedInput` is defined but currently
unreachable by any built-in extractor -- Track B4's real parsers are
expected to be the first to actually return it. No wiring yet: `mini-
intake`'s coordinator (Track B2) does not call `run_worker` -- that
integration, and choosing where the compiled `mini-extract-worker`
binary is expected to live relative to its caller (mirroring `mini-cli
build run`'s next-to-the-executable-then-PATH resolution), is left to a
later PR. No cross-process resource accounting beyond wall-clock and
declared-output-size -- no memory ulimit, no CPU-time ulimit, no disk-
write prevention (the worker process is never given a path to write to,
but nothing in this crate enforces that at the OS level the way a
seccomp filter would).

**Required follow-up:** wire `mini-intake`'s coordinator to
`mini-extract-host::run_worker` for `.txt`/`.md` files currently handled
in-process, and for any future format once Track B4 lands; Track B4
(PDF/HTML extraction backends, after licence/security review, each as
its own new `ExtractorKind` variant); Track B5 (intake publication
linking); consider real OS resource limits (`setrlimit` on Unix, a job
object on Windows) as a follow-up hardening pass once a real extractor
with a larger attack surface than lossy UTF-8 decoding exists.

**Supersedes / superseded by:** none. Two new crates; composes no
existing crate's runtime behavior, only its own process-spawn/framing
pattern mirrored from `mini-pipeline-protocol`/`mini-build-runner-
wasmtime` (D-0069) without a dependency edge to either.
---

### D-0320 — `reproducibility` CI: build only the artifacts the job actually hashes  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** D-0314 (the split of this job out of
`ci.yml`); SPEC-11 §8 (verified-reproducible releases, roadmap #69);
`docs/INVARIANTS.md` (frozen invariant); issue #173

**Decision:** the `reproducibility` workflow's two passes build
`--examples` for the four crates that have an `examples/` directory —
`mini-bootstrap`, `mini-keystone`, `mini-net`, `mini-treasury` — and their
dependency closure, instead of `--workspace --all-targets`. The
two-clean-build discipline, the `rm -rf target` between passes, and the
`find`/`sha256sum`/`diff` comparison are unchanged.

**Reason:** the job built all 44 crates and all 50 integration-test
binaries twice, then hashed exactly four example binaries. Every test
binary was compiled twice and never compared, so the extra work proved
nothing the job asserts while costing roughly twenty minutes of wall clock
on a *required* status check — making it the merge bottleneck for every PR
that touches source.

This narrows no claim. The assertion surface was already those four
binaries; only the build scope changes, to match it. D-0314 narrowed this
job's *triggers* for the same reason — cost that buys no additional
assurance — and this narrows its *scope* on the same principle.

The honest limits are recorded in the job's own comment rather than left
implicit: the check remains same-machine, same-toolchain (not the
K-independent-builder, cross-machine check SPEC-11 §8 ultimately wants),
and its assertion surface is four example binaries by choice. If SPEC-11's
intent is later read as "every release artifact is reproducible" rather
than "these example binaries are", the correct fix is to widen the
*hashing* step to cover the additional artifacts — not to restore a
blanket `--all-targets` build whose extra output is never compared. Build
scope and assertion scope should stay equal in either direction.

**Constitutional impact:** none. This is CI scope, not protocol. It adds
no authority, changes no invariant text, and weakens no reproducibility
claim the repository makes — the set of artifacts asserted byte-identical
across two clean builds is unchanged. SPEC-11 remains a frozen invariant
and is not amended by this entry.

**Implementation status:** shipped in
`.github/workflows/reproducibility.yml`. Verified locally: `cargo build
--release --locked --examples -p mini-bootstrap -p mini-keystone -p
mini-net -p mini-treasury` completes and produces exactly
`bootstrap_live_demo`, `frost_live_demo`, `gossip_live_demo`, and
`keystone` — the same set the hashing step globs, cross-checked against
`cargo metadata`, which shows one example target per crate. The
before/after CI ratio is deliberately not quantified here; it is
measurable from this PR's own run.

**Required follow-up:** a larger CI runner (2-core `ubuntu-latest` is the
current constraint; a paid 4/8-core runner would cut this again with no
semantic change); caching `~/.cargo/registry`/`~/.cargo/git`, which is
symmetric across both passes and safe — unlike caching `target/`, which
would make the passes asymmetric and could mask the very nondeterminism
this job exists to catch; and verifying whether this job's `paths-ignore`,
combined with its status as a *required* context, can leave a docs-only PR
permanently unmergeable.

**Supersedes / superseded by:** refines D-0314's narrowing of this job.
Supersedes nothing; SPEC-11 is untouched.
### D-0322 — `mini-crypto`: ML-DSA-65 key generation + isolated signing, Phase 2 of the post-quantum identity migration (D-0095, issue #15)  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** D-0095 (Phase 1: verify-only support);
`docs/design/post-quantum-identity-migration.md`; `docs/research/
PQ15_POST_QUANTUM_MIGRATION_RESEARCH_20260715.md` §24 (rollout phases);
issue #15; issue #175

**Decision:** `SigningKey` gains `generate_ml_dsa_65()` and
`sign_ml_dsa_65(message)`, composing `fips204`'s own
`try_keygen_with_rng`/`try_sign_with_rng` with `rand_core::OsRng` for
entropy — the same canonical OS-backed RNG Phase 1's own tests already
validated real `fips204` keygen/signing against. These are new,
explicitly-named, suite-specific methods alongside the existing
Ed25519-only `generate()`/`sign()`, which are completely unchanged in
behavior. Internally, `SigningKey`'s private field becomes a
`SecretKeyMaterial` enum (`Ed25519(Box<DalekSigningKey>)` /
`MlDsa65 { public, secret }`, both `fips204` structs boxed together
since FIPS 204's exposed API gives no way to recover a `PublicKey` from
a `PrivateKey` alone, unlike Ed25519 where the verifying key is cheaply
re-derived from the signing key) — mirroring `KeyMaterial`'s existing
suite-tagged-enum pattern on the `VerifyingKey` side from Phase 1.
`verifying_key()` now handles both suites. Secret zeroization on drop is
structural, not reimplemented — `fips204::ml_dsa_65::PublicKey`/
`PrivateKey` both derive `ZeroizeOnDrop` already.

`sign()` and `to_seed_bytes()` stay infallible (unchanged return types,
since every existing call site across the workspace already assumes
that and holds only Ed25519 keys) but now **panic** with a clear,
specific message if called on an `MlDsa65` key — a genuinely reachable
caller bug now that `generate_ml_dsa_65()` exists, honestly documented
as such rather than mislabeled `unreachable!()`. `sign_ml_dsa_65()`,
being a brand-new method with no legacy infallible contract to preserve,
instead returns `Err(CryptoError::SignatureSuiteMismatch)` (a new error
variant, mirroring the existing `KeyAgreementSuiteMismatch`) when called
on an Ed25519 key — the `Result`-returning, non-panicking discipline
`VerifyingKey::verify` already uses for its own suite-mismatch case.

**Reason:** the research report's own rollout phasing (§24) names Phase
2 as "generate ML-DSA keys; sign typed test messages; secure RNG; secret
zeroisation; benchmarks; mobile tests; cross-implementation vectors" —
explicitly listed *before* Phase 5's external-cryptographic-review gate,
confirming key generation/isolated signing is self-contained crate work
the review gate does not block (the gate applies to network opt-in and
KEL activation, Phase 5 onward). Composing `fips204`'s own
`try_keygen_with_rng`/`try_sign_with_rng` plus the canonical
`rand_core::OsRng` (already a proven quantity from Phase 1's tests)
rather than hand-rolling an RNG wrapper follows Directive 14 (prefer the
smaller, well-trodden construction) — `rand_core::OsRng`'s own
entropy-failure posture (panic inside `RngCore::fill_bytes`, since that
trait method's signature has no `Result` to propagate through) is
industry-standard behavior this crate reuses rather than reinvents.

**Constitutional impact:** none negative. No new cryptography (Directive
14) — composes only `fips204`'s and `rand_core`'s already-reviewed
primitives, identical discipline to Phase 1's `VerifyingKey`/`Signature`
work. No voice/value wall implications. Typed-domains rule honored:
`sign_ml_dsa_65`/`generate_ml_dsa_65` are specific, narrowly-named
methods, not a generic suite-parameterized `sign(suite, bytes)` a caller
could misuse. Does **not** touch `SignatureSuite::DEFAULT`, `did-mini`,
or any KEL/identity logic — Phase 3 onward remains completely
unstarted, exactly as D-0095 already scoped.

**Implementation status:** shipped in `mini-crypto` only. `rand_core`
promoted from this crate's `[dev-dependencies]` to `[dependencies]`
(version-pinned to match what `fips204` itself resolves to, so there is
exactly one `rand_core` in the dependency graph — the same discipline
`mini-build-runner-wasmtime`'s Cargo.toml already documents for its own
`rand_core` pin). `cargo fmt --all`, `cargo clippy --all-targets
--all-features --workspace -- -D warnings`, and `cargo test --workspace
--all-features` all clean, including 8 new tests: a real generate →
sign → verify round-trip entirely through `mini_crypto`'s own public API
(stronger than Phase 1's tests, which exercised `fips204` directly for
keygen/signing and `mini_crypto` only for verification); wrong-message
and wrong-key rejection; two independently generated keys are distinct
(proving real RNG-backed generation); `sign_ml_dsa_65` on an Ed25519 key
returns the new mismatch error rather than panicking; `sign`/
`to_seed_bytes` on an `MlDsa65` key panic with the documented message;
`Debug` output on a generated `MlDsa65` `SigningKey` still redacts
secret material.

**Failure point:** benchmarks and mobile/WASM testing (both explicitly
named in Phase 2's own scope) are not done — no benchmarking harness or
mobile toolchain exists in this environment; named honestly here rather
than silently skipped. No official FIPS 204 known-answer cross-
implementation test vectors are exercised (this PR's tests check
internal self-consistency through `mini_crypto`'s own API, the same
honesty posture Phase 1's tests already had). No secret-key storage
export/import for `MlDsa65` — `to_seed_bytes`'s Ed25519-only 32-byte-
seed model has no FIPS 204 equivalent exposed by `fips204`'s API (no
way to derive a `PublicKey` back from a raw seed the way Ed25519 does),
and building one (concatenated public+private key export) was judged
out of Phase 2's named scope; a generated `MlDsa65` key today only lives
for the process's lifetime. `sign`/`to_seed_bytes` panicking on the
wrong suite is a real footgun for any future caller who forgets to check
`SigningKey::suite()` first — acceptable for now because nothing in the
workspace outside this PR's own tests constructs an `MlDsa65` `SigningKey`
yet, but this must be kept in mind by whichever future PR (Phase 3,
`did-mini` wiring) starts handing suite-mixed keys to shared code paths.

**Required follow-up:** Phase 3 (`did-mini`'s KEL hybrid migration
protocol: pre-commitment, dual-authorised rotation, downgrade
prevention, legacy-client handling) — not started, and per D-0095's own
hard rule, may not land before the external cryptographic review gate
that specifically covers *migration* (not this PR's self-contained
keygen/signing primitive). Benchmarks and mobile/WASM testing as a
fast-follow once appropriate tooling exists. Consider whether
`sign`/`to_seed_bytes`'s panic-on-wrong-suite posture should become
`Result`-returning workspace-wide once Phase 3 actually starts handing
`SigningKey` values across suite-mixed call sites.

**Supersedes / superseded by:** extends D-0095 (Phase 1). Does not
supersede it — `VerifyingKey`/`Signature`'s Phase 1 behavior is
completely unchanged.

### D-0321 — `did-mini`: KEL witness receipt types, Phase 1 of witness receipts + duplicity gossip (audit #12 F4, invariant M3)  ·  *Accepted*
**Date:** 2026-07-19 · **Refs:** D-0096 (`docs/design/
kel-witness-receipts-and-duplicity-gossip.md`, the Phase 0 design-only
predecessor this PR implements Phase 1 of); `docs/research/
KEL_WITNESS_RECEIPTS_DUPLICITY_GOSSIP_RESEARCH_20260715.md` (founder-
supplied, 2026-07-15); audit #12 finding F4; invariant M3
(`docs/INVARIANTS.md`); issue #177

**Decision:** adds `WitnessId`, `KeyEventKind`, `WitnessReceiptVersion`/
`WitnessCertificateVersion`, `WitnessPolicy`, `WitnessReceiptStatement`,
`WitnessReceipt`, and `WitnessedEventCertificate` to `did-mini`, plus
`sign_witness_receipt(WitnessReceiptStatement)` — the one typed function
a witness ever calls to produce a receipt, never a generic
`sign(bytes)`, per CLAUDE.md's typed-domain rule. Implements exactly
Phase 1 of the design doc's committed phased plan ("receipt types;
canonical encoding; signature verification; no network service") using
`event.rs`'s existing hand-rolled `Writer`/`Reader` codec discipline
rather than a new format.

`WitnessPolicy::new` validates `1 <= threshold <= witnesses.len()` and
rejects duplicate witness identifiers at construction, mirroring
`event::validate_establishment`'s existing reject-early discipline for
key sets. `WitnessedEventCertificate::assemble` rejects any receipt that
does not exactly match the certificate's own claimed identity/sequence/
event-digest/generation before admitting it, then canonically sorts
receipts by witness DID for deterministic encoding (research report
§10.1). `WitnessedEventCertificate::verify(&policy, resolve_witness_key)`
checks: the certificate's claimed generation matches the given policy;
every receipt matches the certificate's own event; every witness
belongs to the policy; no witness counts twice toward the threshold;
every signature verifies via the caller-supplied resolver (fails closed
— `UnresolvedWitnessKey`, not a silent skip, if the resolver can't find
a key); the threshold is met.

**Reason:** the design doc's own recommended sequencing (echoing the
research report's closing recommendation) is "a small receipt/proof
type PR, then an in-memory witness state-machine PR, and only
afterward network gossip" — exactly the phase boundary this PR
implements. This closes the harder half of `FreshnessPins` (D-0088)
does not solve: a verifier meeting an identity for the first time has
no prior head to pin against, and two internally-valid, controller-
signed branches can both pass ordinary KEL verification in isolation.
Composing `did-mini`'s existing typed-signature machinery (`SigningKey`/
`VerifyingKey`/`Signature` from `mini-crypto`) rather than any new
cryptographic construction follows both Directive 14 and the design
doc's own hard rule: "ordinary independent signatures first... composing
`did-mini`'s existing typed-signature machinery is sufficient for
Phase 1-3."

**Constitutional impact:** none negative. No new cryptography (Directive
14) — every signature is an ordinary `mini_crypto::Signature` over a
typed statement, no aggregation, no BLS, no bespoke consensus. No voice/
value wall implications. Typed-domains rule honored throughout:
`WitnessId` is a distinct type from a bare `Did` even though structurally
identical, `WitnessReceiptVersion`/`WitnessCertificateVersion` are
distinct types so one can never be substituted for the other, and
`sign_witness_receipt` takes the one specific statement type rather than
raw bytes. Witnesses gain no authority by this PR or by design — a
witness attests observation only; nothing here lets a witness create,
rotate, or override an identity event (the design doc's own hard rule,
carried forward structurally: `WitnessedEventCertificate::verify` never
returns anything resembling "this event is now authoritative," only
whether a threshold of witnesses observed it).

**Implementation status:** shipped in `did-mini` only (`witness.rs`, new
module). `cargo fmt --all`, `cargo clippy --all-targets --all-features
--workspace -- -D warnings`, and `cargo test --workspace --all-features`
all clean, including this module's 24 new tests (policy construction/
validation/round-trip; statement and receipt round-trips including the
inception no-prior-digest case; signature verification and rejection
under a wrong key; certificate assembly rejecting a mismatched receipt;
certificate verification succeeding at threshold, rejecting below
threshold, rejecting a witness outside the policy, rejecting a stale
policy generation, and failing closed on an unresolvable witness key;
trailing-bytes rejection on every decoder). Every pre-existing
`did-mini` test (identity, delegation, recovery, pairwise, identity
modes) still passes unchanged — this PR touches no existing type or
function.

**Failure point:** exactly what the design doc's own scope line names as
not-yet-done: no in-memory witness state machine (so nothing in this
repo actually issues a receipt in response to a real observed event
yet), no `ControllerDuplicityProof`/`WitnessEquivocationProof` (Phase
2), no `KelAssurance`/KEL-verification integration (Phase 3 — a
`WitnessedEventCertificate` cannot yet be checked against a live
`did_mini::Kel`), no receipt-freshness-policy evaluation against
`observed_epoch` (also Phase 3), no receipt collection protocol, no
gossip, no persistent witness service, no witness rotation, no public
transparency logs, no adversarial network simulation. `WitnessPolicy`
is not yet carried by `Establishment` events (`event.rs`'s existing
`witnesses: Vec<Vec<u8>>` field remains its own pre-existing, differently-
shaped placeholder, explicitly reserved and unused) — wiring a real
`WitnessPolicy` into inception/rotation events is Phase 3's job, not
this one's.

**Required follow-up:** Phase 2 (in-memory witness state machine:
first-seen acceptance, direct-successor verification, duplicate
idempotence, stale rejection, conflict detection, receipt issuance,
`ControllerDuplicityProof`, `WitnessEquivocationProof`) — not started.
Phase 3 (`KelAssurance` output alongside ordinary KEL validity, wiring
`WitnessPolicy` into real establishment events) — not started, and per
the design doc's own hard rule, no high-value authority decision may
depend on this layer before Phase 10's external cryptographic review.

**Supersedes / superseded by:** none. New module, additive only —
`event.rs`'s existing `witnesses` field, `FreshnessPins`, and every
other existing `did-mini` type/function are unchanged.
### D-0324 — `mini-extract-host`: make a zero-millisecond deadline a deterministic timeout instead of a race  ·  *Accepted*
**Date:** 2026-07-20 · **Refs:** D-0319 (`mini-extract-host` itself);
`crates/mini-extract-host/src/lib.rs` (`run_worker`); `crates/mini-extract-host/tests/host.rs`
(`a_zero_millisecond_deadline_is_reported_as_timeout`)

**Decision:** in `run_worker`, when the caller's `ResourceLimits::max_wall_clock_ms`
is `0`, kill the child and return `ExtractionOutcome::Err(ExtractionError::Timeout)`
immediately, without waiting on the stdout-reader channel at all. Every other
timeout value is unchanged: the existing `rx.recv_timeout(timeout)` path still races
the worker against a real deadline.

**Reason:** the test asserting this behavior was intermittently failing in CI (most
recently blocking PR #181, an unrelated docs-only PR, on the required `check` job)
while passing reliably in isolation. The root cause is a genuine race, not flakiness
in the test's assertion: `Duration::from_millis(0)` is `Duration::ZERO`, and
`mpsc::Receiver::recv_timeout(Duration::ZERO)` does not guarantee "always already
expired" — under favorable scheduling, the worker's round trip for the test's
trivial 8-byte input can complete and reach the channel before or exactly as the
zero-duration wait is evaluated, so the call sometimes observes `Ok(frame)` instead
of the expected `Err(RecvTimeoutError::Timeout)`. A zero-millisecond wall-clock
budget can never be honestly satisfied by a real child-process round trip in any
case — spawning, writing the request, the worker doing any work at all, and writing
the response back all take non-zero wall-clock time — so racing it at all was never
buying a meaningful check; special-casing it to a deterministic immediate timeout is
both a correctness fix (the outcome no longer depends on scheduler luck) and the more
honest semantics (a zero budget deterministically cannot be met).

**Constitutional impact:** none. This is an isolation-host timeout-accounting detail,
not protocol. It adds no authority, changes no invariant, and does not alter
`mini-extract-host`'s documented isolation limits (`docs/mobile/../lib.rs`'s own
"Honest limits" section is unchanged) — a zero-millisecond deadline still always
yields `ExtractionError::Timeout`, exactly as before; only the path by which it does
so is now deterministic instead of racy.

**Implementation status:** shipped in `crates/mini-extract-host/src/lib.rs`.
Verified: `cargo test -p mini-extract-host` passes; the previously-flaky test run
30 times in a loop with `--exact`, all 30 green (versus intermittent failure before
the fix); full-workspace `cargo fmt --all -- --check`, `cargo clippy --all-targets
--all-features --workspace -- -D warnings`, and `cargo test --workspace
--all-features` all clean.

**Failure point:** this only fixes the `max_wall_clock_ms == 0` case. Non-zero
deadlines still rely on `mpsc::Receiver::recv_timeout` racing a real child process,
which remains correct (a non-zero timeout has a real window in which "not yet
received" is a true statement throughout) but is not covered by this entry's
reasoning — if similar intermittent failures are ever observed at very small
non-zero deadlines under heavy CI load, that would be a scheduling-latency issue,
not this same race, and would need its own diagnosis.

**Required follow-up:** none identified. This is a narrow, self-contained fix.

**Supersedes / superseded by:** none. `run_worker`'s documented behavior for the
zero-deadline case (always `ExtractionError::Timeout`) is unchanged; only how
reliably it is observed changes.

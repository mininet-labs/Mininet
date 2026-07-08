# Mininet

> A population, not an organization. Fork it, build on it, run it — own it,
> together.

Mininet is a peer-to-peer network whose rules sit above its protocol: money buys
reach and storage but never a vote; governance is one verified human, one equal
vote; there is no owner, no institution, no foundation, no admin key, no off
switch, no law-enforcement backdoor, and no party that can unmask a user. The
software is public domain, built Rust-first and in-house — proven designs are
adapted into our own tree, never taken as a live external dependency.

This repository is the **self-contained Rust core**: ~22 crates, no owner, no
external dependency on any single company's infrastructure to keep running.

> **Read this first, before anything else in this repository:**
> [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md) — *MININET
> Founder Directives: The Principles Behind Every Engineering Decision.*
> It is not the Constitution, the Whitepaper, or a Specification — it is
> the *why* underneath all three, written so that a century from now,
> someone facing a problem no document anticipated can still reason the
> way the founders would have. Every contributor, human or AI, reads this
> before opening the codebase.

## New here? Start with these things

1. **Read `docs/FOUNDER_DIRECTIVES.md`.** Seventeen directives, five
   minutes, and every engineering judgment call in this repository — down
   to the code review comments — is expected to trace back to them.
2. **Build it.** `cargo fmt --all && cargo clippy --all-targets --all-features
   --workspace -- -D warnings && cargo test --all --all-features` — all clean
   on this tree, `Cargo.lock` committed. See [Build & test](#build--test) below.
3. **See it run.** Three runnable demos exist today — see
   [Status at a glance](#status-at-a-glance):
   - `cargo run -p mini-keystone --example keystone` — two devices exchange
     identities, prove co-presence, and accrue reward, in-process.
   - `cargo run -p mini-treasury --example frost_live_demo` — five threads
     each holding one key share jointly sign a treasury payout live, then a
     second session shows a tampered share getting caught before it produces
     a bad signature.
   - `cargo run -p mini-net --example gossip_live_demo` — three genuinely
     separate OS processes gossiping a message over real TCP sockets (not
     simulated in one process — see `crates/mini-net/README.md` for the
     three-terminal walkthrough).
4. **Find your way around.** `python3 tools/mininet_nav.py map` builds an
   offline, searchable index of every crate, doc, and symbol in the tree — see
   `docs/NAVIGATION.md`. No GitHub search or IDE required.
5. **Read before you touch a FREEZE domain.** `docs/DECISION_LOG.md` (every
   architectural and policy decision, numbered `D-0001`–`D-0042` so far) and
   `docs/INVARIANTS.md` (the frozen-vs-tunable register mapped to code) are
   the two documents that outrank any comment or README, including this one.
   `CONTRIBUTING.md` has the PR/review checklist (two-approval floor, D-0033).
   `docs/TESTING.md` has copy-pasteable verification steps and a reviewer
   checklist, including how to review the cryptography prototypes below.

## Status at a glance

**What's real and running:** identity, presence, storage, sync, the social
layer, and the forge/release-governance logic are all working, tested Rust —
see the repository map below for the per-crate breakdown. **What's a
cryptography prototype, not a finished product:** stealth addresses, ring
signatures, and Bulletproofs confidential amounts (`mini-value`); FROST
threshold custody (`mini-treasury`); Merkle/PDP storage proofs
(`mini-spacetime`) — all AI-authored under an explicit founder policy
(`D-0037`: AI may write this code, a human must review it, and it still needs
a specialized external cryptography audit before any real value depends on
it). **What has a real network transport, for the first time (D-0042):**
`mini-bearer::TcpBearer` moves frames over an actual TCP socket, and
`mini-net`'s live demo gossips a message across three separate processes
over it — real IP connectivity, not yet BLE (needs platform-native radio
code no library crate can provide) and not yet wired into every crate that
needs it (`mini-bootstrap`, `mini-sync`, the keystone demo are all still
in-process only). **What doesn't exist yet, at all:** BLE/local-radio
transport, a mobile or desktop client app, and a solved construction for
the personhood behavioral/location ZK proof (the whitepaper itself calls
this open research, not engineering debt — see `mini-uniqueness`'s honest
limit). None of this should be read as "ready for real people or real
value" — see
[Path to a global launch](#path-to-a-global-launch-what-is-still-missing) for
the full list.

## Repository map

```
mininet/
├── Cargo.toml              workspace for the Rust core
├── rust-toolchain.toml     pinned toolchain for reproducible-build hygiene
├── tools/mininet_nav.py    offline repo index/search (docs/NAVIGATION.md)
├── crates/                 22 crates, see the table below
├── docs/
│   ├── FOUNDER_DIRECTIVES.md    read this first — the why beneath every other document
│   ├── DECISION_LOG.md          every stack and freeze choice, with rationale (D-0001..)
│   ├── INVARIANTS.md            frozen/tunable register mapped to code
│   ├── ROADMAP.md               pack order from two-phone demo to full network
│   ├── BETA_STATUS.md           near-term target: the two-phone keystone beta
│   ├── NAVIGATION.md            how to use tools/mininet_nav.py
│   ├── BOOTSTRAP_AND_UPDATE.md  self-contained update + Bluetooth bootstrap spec
│   └── UI_BETA_PLAN.md          the eventual product/UI layer, not yet built
├── CONTRIBUTING.md          PR checklist, review floor, scope-of-a-batch rule
└── .github/workflows/ci.yml  fmt + clippy + test on every PR (temporary mirror CI)
```

Every crate below is a **library**, not a running binary, unless noted.
Status tags: ✅ logic complete and tested · 🧪 real AI-authored crypto
prototype, founder-reviewed, pending external audit (D-0036/D-0037) · 🚧
partial/structural piece, real transport or a further layer still pending ·
🔬 deliberately blocked on unsolved research, not an engineering gap.

| Crate | What it does | Status |
|---|---|---|
| `mini-crypto` | Crypto-agile primitives: signatures, X25519, ChaCha20-Poly1305, HKDF, strong multihash | ✅ |
| `did-mini` | KERI-style self-certifying identity: KEL, pre-rotation, device delegation | ✅ |
| `mini-bearer` | Bearer trait + anonymous encrypted channel + real `TcpBearer` | 🚧 real TCP transport (D-0042); BLE/Wi-Fi radio adapters still pending |
| `mini-presence` | Mutually-signed, range-bound co-presence attestation | 🚧 alpha; active RTT challenge-response pending |
| `mini-reward` | Deterministic, non-spendable local reward accrual | 🚧 alpha; demo stub, not money |
| `mini-keystone` | The two-device demo harness (`cargo run --example keystone`) | 🚧 alpha; still in-process only, not yet ported to `TcpBearer` |
| `mini-objects` | Unified signed, content-addressed object envelope (SPEC-09) | ✅ |
| `mini-store` | Local content-addressed store: blobs, indexes, head pointers | ✅ |
| `mini-crdt` | Op-log CRDT for threads/docs, offline-first merge | ✅ |
| `mini-sync` | Bucketed reconciliation + verified ingest over any bearer | ✅ |
| `mini-social` | Profiles, follow graph, explainable locally-computed feeds, public walls | ✅ |
| `mini-media` | Chunked content-addressed media, progressive assembly | ✅ |
| `mini-forge` | Repos, branches, releases + attestations, governed merge | ✅ logic complete; git SHA-256 interop pending |
| `mini-bootstrap` | Self-certifying genesis/update capsule, chunked exchange | 🚧 protocol logic done; real transport is `mini-bearer`'s job |
| `mini-update` | Local update-adoption state machine (no forced update, no kill path) | ✅ |
| `mini-net` | Kademlia-style routing table + gossip broadcast | 🚧 gossip proven live over real TCP (D-0042, `examples/gossip_live_demo.rs`); peer discovery/mesh routing still logic-only |
| `mini-storage` | Mutually-signed storage-served receipts | ✅ |
| `mini-chain` | BFT finality-verification core (`ValidatorSet`, quorum certs) | 🚧 finality core done; networked consensus + state machine pending |
| `mini-spacetime` | Proof-of-space-time storage weight for block production | 🧪 Merkle/PDP proves continuous possession, not replication uniqueness (D-0039) |
| `mini-uniqueness` | Personhood/uniqueness: open-ended multi-signal fusion + status | 🧪 fusion logic real (D-0038); the behavioral/location ZK signal itself is 🔬 unsolved research |
| `mini-treasury` | Contribution bookkeeping + FROST threshold custody | 🧪 FROST + live multi-device demo (D-0041); trusted-dealer keygen, no DKG yet |
| `mini-value` | MINI fee bookkeeping + transaction-privacy primitives | 🧪 stealth addresses, ring signatures, Bulletproofs confidential amounts (D-0036/D-0040) |

See `docs/DECISION_LOG.md` for the reasoning and honest limits behind every
🧪/🔬 entry, and each crate's own `README.md`/top-of-file doc comment for the
full detail — those are written to be the first thing a reviewer opens.

## What we've built so far, grouped by theme

- **Identity & presence.** Self-sovereign `did:mini` identity with no
  central registry, device delegation, and physically-verified co-presence
  between two nearby devices — the foundation everything else builds on.
- **Content & social fabric.** A signed, content-addressed object model
  underneath profiles, feeds, forums/CRDT docs, and chunked media — all
  locally computed and explainable, no hidden ranking algorithm.
- **Chain & release governance.** A BFT finality-verification core with
  equal vote weight per verified identity root (never stake), plus a
  governed release/update path with no forced-update or kill-switch path.
- **Personhood & Sybil resistance.** Redesigned per founder direction
  (D-0038) from a fixed three-signal proof into an open-ended, weighted
  multi-signal accumulator — `VouchedHuman` as a fast onboarding path,
  `FullHuman` reachable only automatically once several independent, live,
  currently-valid signals and a minimum age all agree. This sidesteps rather
  than solves the whitepaper's hardest open research problem (see below).
- **Storage & space-time.** A Merkle/PDP challenge-response proof that a
  device is still holding data it claims to store, feeding block-production
  weight — explicitly proving continuous possession, not yet replication
  uniqueness.
- **Treasury & value privacy.** Real (prototype-grade) cryptography for both
  of the whitepaper's highest-stakes domains: stealth addresses + linkable
  ring signatures + Bulletproofs confidential amounts for the one MINI
  ledger, and FROST threshold signatures (with a live multi-device demo) for
  treasury custody.

All of the cryptography above was authored under **D-0037**: the founder
cohort's explicit, recorded decision to let AI draft this code as long as a
human reviews it, rather than requiring human authorship from the start.
That policy does not, and cannot, substitute for the external cryptography
audit every 🧪 item above still needs before real value or real custody
depends on it.

## Path to a global launch — what is still missing

This is the honest gap list between "the protocol logic works and is
tested" and "people anywhere in the world can actually run and trust this
network." Nothing below is secret or silently dropped — each is also
documented at the crate level.

1. **External cryptography audit.** Every 🧪 item in the table above
   (stealth addresses, ring signatures, Bulletproofs, FROST, Merkle/PDP
   storage proofs) is founder-reviewed AI-authored work, not
   audit-equivalent. This is the single largest gate before any real value
   or custody touches this code.
2. **A real network transport.** Partially closed (D-0042):
   `mini-bearer::TcpBearer` moves frames over a real TCP socket, and
   `mini-net`'s gossip demonstrably works across three separate processes
   over it — proof the design holds under real message-passing, not just
   in-process simulation. Still missing: BLE (needs platform-native radio
   code, out of reach for a library workspace with no phone hardware),
   `mini-bootstrap`'s capsule exchange and `mini-sync`'s replication aren't
   wired to `TcpBearer` yet, and `mini-net`'s peer *discovery* (as opposed
   to gossip) is still logic-only. This is the difference between "three
   demo processes on loopback" and "phones in different countries" — closer
   than before, not there yet.
3. **A client people can actually install.** There is no mobile, desktop, or
   web application anywhere in this repository yet — `docs/UI_BETA_PLAN.md`
   is a plan, not code. Global launch needs an installable app, not a
   library workspace.
4. **The personhood ZK proof (signal (b)).** The whitepaper itself describes
   on-device behavioral/location entropy proved in zero-knowledge as
   unsolved research. D-0038's multi-signal redesign makes the *system* not
   depend on this one signal, but it does not solve the underlying research
   problem — that remains open.
5. **FROST distributed key generation.** The treasury custody prototype
   uses trusted-dealer keygen, where one party briefly holds the whole
   secret. A real deployment needs DKG, so no party ever holds it, even
   briefly.
6. **Consensus and chain networking.** `mini-chain` verifies finality given
   valid votes; the networked BFT protocol (proposing, voting, gossiping
   blocks across real peers) and the full state machine are not built yet.
7. **Security posture at scale.** No dependency-vulnerability scanning is
   wired into CI yet (`cargo audit` or equivalent), and the reproducible-
   build CI job is present but disabled pending real infrastructure.
8. **Abuse/moderation tooling at the edges.** Content rules are explicitly
   meant to live in user/community filters (constitution principle 10), but
   almost none of that tooling exists yet beyond `mini-social`'s follow
   graph and feed computation.
9. **Load and adversarial testing at real scale.** Everything so far is
   unit- and demo-tested on one machine. Sybil-cost economics, gossip
   behavior under churn, and storage-proof behavior under real network
   partitions are all untested against anything resembling global scale.

None of these are quick fixes, and several (1, 2, 3, 6) are each
substantial, multi-month efforts on their own. They are listed in roughly
the order a global launch would need them resolved, not necessarily the
order they'll be worked in — see `docs/ROADMAP.md` for the actual pack
sequencing, which currently targets the much nearer two-phone keystone beta
(`docs/BETA_STATUS.md`) before any of the above.

## Suggested improvements (not yet decided, worth raising with the founder cohort)

- Wire `cargo audit` (or `cargo deny`) into CI now, while the dependency tree
  is still small — cheap today, much more valuable once it's not.
- Consider standing up the DKG variant of FROST keygen before the trusted-
  dealer version ever gets used with anything of real value, even in a
  testnet — it's a smaller lift now than a migration later.
- Start the mobile/desktop client track in parallel with the remaining
  transport work (BLE, item 2 above), since neither blocks the other and
  both gate global launch equally.
- Wire `TcpBearer` into `mini-bootstrap` and `mini-sync` next, so genesis/
  update capsule exchange and store-and-forward replication get the same
  real-network proof `mini-net`'s gossip demo just got — the transport now
  exists, only two of its intended consumers use it so far.

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

## Identity, public walls, and base devices

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
  `did-mini::IdentityMode` for the full taxonomy.
- **One base/static device is recommended** per human — for hosting, storage,
  seeding, and participation (`did-mini::BaseDeviceRole`). It is operational
  infrastructure, not political power: it cannot buy governance weight.
- **Watching helps seed it.** Opening public content naturally helps seed it
  to the network, unless the user disables that or content policy forbids it
  — see `mini-store::CacheTier` and `Store::note_view`.
- **Money, storage, and reach never buy a vote.** Storage/seeding commitment
  earns value (`mini-reward`) and reach, never voice (P1).

## Stack at a glance

- **Language:** one Rust stack for on-device core and chain.
- **Chain:** custom Rust chain adapting proven BFT finality; equal validator vote
  weight per verified human, never stake.
- **Identity:** KERI-style did:mini autonomic identifiers.
- **Networking core:** BLE + local Wi-Fi/hotspot/mDNS + optional relay;
  store-and-forward/delay-tolerant by default. Radio/LoRa is **permanently
  out of scope** (D-0033).
- **Forge/update:** internal content-addressed forge and on-chain release registry;
  GitHub/GitLab/etc. are temporary mirrors only.

## Build & test

```sh
cargo fmt --all
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo test --all --all-features
```

All three are clean on this tree and `Cargo.lock` is committed for
reproducible builds (D-0006).

## Contributing & review

See `CONTRIBUTING.md` for the PR checklist and the two-approval floor
(D-0033). If your change touches a FREEZE domain (`docs/INVARIANTS.md`),
it needs a `docs/DECISION_LOG.md` entry — look at the existing `D-0036`
through `D-0041` entries for the expected shape (what was built, why, what
it does *not* prove, and what's still pending).

## License

Public domain via [CC0 1.0](./LICENSE).

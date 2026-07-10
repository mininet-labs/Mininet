# Start here — for a developer

Everything you need to build, run, and navigate the Mininet core. For *why*
any of it is shaped the way it is, the canonical documents (linked from the
[README](../README.md)) outrank this page and every code comment.

## Build & test

```sh
cargo fmt --all \
  && cargo clippy --all-targets --all-features --workspace -- -D warnings \
  && cargo test --workspace --all-features
```

All clean on this tree, `Cargo.lock` committed. Toolchain is pinned
(`rust-toolchain.toml`) for reproducible-build hygiene.

## See it run — three demos that exist today

- `cargo run -p mini-keystone --example keystone` — two devices exchange
  identities, prove co-presence, and accrue reward, in-process.
- `cargo run -p mini-treasury --example frost_live_demo` — five threads, each
  holding one key share, jointly sign a treasury payout live; a second session
  shows a tampered share getting caught before it produces a bad signature.
- `cargo run -p mini-net --example gossip_live_demo` — three genuinely
  separate OS processes gossiping a message over real TCP sockets (not
  simulated in one process — see `crates/mini-net/README.md` for the
  three-terminal walkthrough).

## Find your way around

`python3 tools/mininet_nav.py map` builds an offline, searchable index of every
crate, doc, and symbol in the tree — see [`NAVIGATION.md`](NAVIGATION.md). No
GitHub search or IDE required.

## Before you touch a FREEZE domain

[`DECISION_LOG.md`](DECISION_LOG.md) (every architectural/policy decision,
`D-0001`–`D-0065`, policy only) and [`INVARIANTS.md`](INVARIANTS.md) (the
frozen-vs-tunable register, by domain, with a hard-limitations section at the
top) outrank any comment or README. [`STATUS.md`](STATUS.md) is the living
account of what's actually built. [`../CONTRIBUTING.md`](../CONTRIBUTING.md)
has the PR/review checklist (two-approval floor, D-0033). [`TESTING.md`](TESTING.md)
has copy-pasteable verification steps, including how to review the cryptography
prototypes. [`FAILURE_BOOK.md`](FAILURE_BOOK.md) records rejected designs —
read it before re-proposing something.

**One structural rule worth internalizing:** any function that exercises real
authority takes a specific, named request type
(`sign_release_attestation(ReleaseAttestation)`), never a generic
`sign(&[u8])` — so the set of things an authority *can* do is fixed at compile
time. Reject a generic authority-shaped signature in review the same way a
money→governance dependency edge gets rejected.

## Repository map

```
mininet/
├── Cargo.toml              workspace for the Rust core
├── rust-toolchain.toml     pinned toolchain for reproducible-build hygiene
├── tools/mininet_nav.py    offline repo index/search (docs/NAVIGATION.md)
├── crates/                 27 crates, see the table below
├── docs/
│   ├── FOUNDER_DIRECTIVES.md    read this first — the why beneath every other document
│   ├── INVARIANTS.md            frozen/tunable register mapped to code, with a Directive-traceability column
│   ├── DECISION_LOG.md          every stack and freeze choice, with rationale (D-0001..)
│   ├── FAILURE_BOOK.md          every rejected design and abandoned approach, and why
│   ├── THREAT_MODEL.md          civilization-scale threat catalog
│   ├── STATUS.md                living implementation-status account, by domain
│   ├── HUMAN_START.md           the curious-person door
│   ├── DEVELOPER_START.md       this file
│   ├── AUDITOR_START.md         the auditor/skeptic door
│   ├── gates/                   external legitimacy gates — audit/legal/hardware/research handoff packages
│   ├── design/                  design notes that close roadmap design issues
│   ├── audits/                  written audit deliverables for roadmap review issues
│   ├── ADDRESSING.md            no-DNS universal addressing design (petnames, not domains)
│   ├── ROADMAP.md               pack order from two-phone demo to full network
│   ├── BETA_STATUS.md           near-term target: the two-phone keystone beta
│   ├── BOOTSTRAP_AND_UPDATE.md  self-contained update + Bluetooth bootstrap spec
│   ├── NAVIGATION.md            how to use tools/mininet_nav.py
│   └── UI_BETA_PLAN.md          the eventual product/UI layer, not yet built
├── CONTRIBUTING.md          PR checklist, review floor, scope-of-a-batch rule
└── .github/workflows/ci.yml  fmt + clippy + test on every PR (temporary mirror CI)
```

## The crates

Every crate is a **library**, not a running binary, unless noted. Status tags:
✅ logic complete and tested · 🧪 real AI-authored crypto prototype,
founder-reviewed, pending external audit (D-0036/D-0037) · 🚧
partial/structural piece, real transport or a further layer still pending ·
🔬 deliberately blocked on unsolved research, not an engineering gap.

| Crate | What it does | Status |
|---|---|---|
| `mini-crypto` | Crypto-agile primitives: signatures, X25519, ChaCha20-Poly1305, HKDF, strong multihash | ✅ |
| `did-mini` | KERI-style self-certifying identity: KEL, pre-rotation, device delegation, recovery | ✅ |
| `mini-bearer` | Bearer trait + anonymous encrypted channel + real `TcpBearer` | 🚧 real TCP transport (D-0042); BLE/Wi-Fi radio adapters still pending |
| `mini-presence` | Mutually-signed, range-bound co-presence attestation | 🚧 alpha; active RTT challenge-response pending |
| `mini-reward` | Deterministic, non-spendable local reward accrual | 🚧 alpha; demo stub, not money |
| `mini-keystone` | The two-device demo harness (`cargo run --example keystone`) | 🚧 alpha; still in-process only, not yet ported to `TcpBearer` |
| `mini-objects` | Unified signed, content-addressed object envelope (SPEC-09) | ✅ |
| `mini-store` | Local content-addressed store: blobs, indexes, head pointers | ✅ |
| `mini-crdt` | Op-log CRDT for threads/docs, offline-first merge | ✅ |
| `mini-sync` | Bucketed reconciliation + verified ingest over any bearer | ✅ proven live over real TCP (D-0062), not just in-process |
| `mini-social` | Profiles, follow graph, explainable locally-computed feeds, public walls | ✅ |
| `mini-media` | Chunked content-addressed media, progressive assembly | ✅ |
| `mini-forge` | Repos, branches, releases + attestations, governed merge | ✅ logic complete; git SHA-256 interop pending |
| `mini-bootstrap` | Self-certifying genesis/update capsule, chunked exchange | 🚧 live TCP bootstrap demo proves real-transport interop (D-0062); real BLE/Wi-Fi radio adapters need phone hardware (#22) |
| `mini-update` | Local update-adoption state machine (no forced update, no kill path) | ✅ |
| `mini-net` | Kademlia-style routing table + gossip broadcast | 🚧 gossip proven live over real TCP (D-0042); peer discovery/mesh routing still logic-only |
| `mini-storage` | Mutually-signed storage-served receipts | ✅ |
| `mini-chain` | BFT finality-verification core (`ValidatorSet`, quorum certs) | 🚧 finality core done; networked consensus + state machine pending |
| `mini-spacetime` | Proof-of-space-time storage weight for block production | 🧪 Merkle/PDP proves continuous possession, not replication uniqueness (D-0039) |
| `mini-uniqueness` | Personhood/uniqueness: open-ended multi-signal fusion + status | 🧪 fusion logic real (D-0038); the behavioral/location ZK signal itself is 🔬 unsolved research |
| `mini-treasury` | Contribution bookkeeping + FROST threshold custody | 🧪 FROST + live multi-device demo (D-0041); real DKG + resharing (D-0060), unaudited |
| `mini-value` | MINI fee bookkeeping + transaction-privacy primitives | 🧪 stealth addresses, ring signatures, Bulletproofs confidential amounts (D-0036/D-0040) |
| `mini-bounty` | Anonymous developer-bounty claims (ring signature + stealth address reuse) | 🧪 real, tested (D-0049); no GitHub integration, no minimum ring-size policy yet |
| `mini-settlement` | Offline transaction settlement: signed pending claims, wallet state machine, double-spend reconciliation (M1/M2/M3) | 🧪 real, tested (D-0055); `CanonicalLedgerView` now has a real chain-backed impl, see `mini-execution` |
| `mini-execution` | Chain-backed `CanonicalLedgerView`: state only advances behind a verified quorum certificate | 🧪 real, tested (D-0061, closes #40); not networked consensus — that's `mini-chain`/`mini-net`'s job |
| `mini-porep` | Real proof-of-replication: Stacked Depth-Robust Graph sealing + registration audit + ongoing challenge-response | 🧪 real, tested (D-0064, closes #31); simplified DRG, probabilistic (non-SNARK) audit, unaudited |
| `mini-erasure` | Systematic Reed-Solomon erasure coding over GF(2^8) + self-healing shard repair | 🧪 real, tested (D-0065, closes #30/#32); coding/repair logic only, not wired to real network distribution |

See [`DECISION_LOG.md`](DECISION_LOG.md) for the reasoning and honest limits
behind every 🧪/🔬 entry, and each crate's own `README.md`/top-of-file doc
comment for the full detail.

## Path to a global launch — what is still missing

The honest gap list between "the protocol logic works and is tested" and
"people anywhere can actually run and trust this network." Each is also
documented at the crate level. The items that **more code cannot close** are
tracked as external legitimacy gates ([`gates/`](gates/), issue [#99](../../issues/99)).

1. **External cryptography audit.** Every 🧪 item above is founder-reviewed
   AI-authored work, not audit-equivalent — the single largest gate before any
   real value ([`gates/crypto-audit-scope.md`](gates/crypto-audit-scope.md), [#72](../../issues/72)).
2. **A real network transport.** `TcpBearer` + live three-process gossip
   (D-0042); `mini-bootstrap`/`mini-sync` now proven live over real TCP too
   (D-0062, closes #23). Missing: real BLE/Wi-Fi radio adapters (need phone
   hardware, [#97](../../issues/97)/[#98](../../issues/98)) and `mini-net`
   peer *discovery* (`RoutingTable` isn't wired to real lookups yet).
3. **A client people can actually install.** No mobile/desktop/web app exists
   yet — `UI_BETA_PLAN.md` is a plan, not code.
4. **The personhood ZK proof (signal b).** On-device behavioral/location
   entropy proved in zero-knowledge is unsolved research; D-0038 makes the
   system not depend on it, but doesn't solve it
   ([`gates/personhood-signal-b-decision.md`](gates/personhood-signal-b-decision.md), [#21](../../issues/21)).
5. **FROST distributed key generation — P0 (D-0048), implementation done, audit open.**
   Real Pedersen DKG and committee resharing now exist and are tested
   (D-0060); `trusted_dealer_keygen` remains only for tests/demos, gated
   behind an explicit acknowledgment type either way. What's still missing
   is the external review itself
   ([`gates/dkg-audit-scope.md`](gates/dkg-audit-scope.md), [#93](../../issues/93)).
6. **Consensus and chain networking — state machine done, networking open.**
   `mini-chain` verifies finality given votes; `mini-execution` (D-0061,
   closes #40) is the real state machine `mini-settlement`'s
   `CanonicalLedgerView` needed, proving double-spend resolution and
   cross-node convergence given a finalized block. What's still missing is
   the *networked* BFT protocol itself — proposer rotation, vote gossip,
   round timeouts/view-change — that produces those finalized blocks in the
   first place (#36-#45).
7. **Security posture at scale.** Closed (D-0044): dependency scanning +
   same-machine reproducible-build check in CI. Open: cross-machine
   K-independent-builder reproducibility (SPEC-11 §8), and CodeQL-alert triage.
8. **Abuse/moderation tooling at the edges.** Content rules live in
   user/community filters (constitution principle 10); little of that tooling
   exists yet beyond `mini-social`.
9. **Treasury economics calibration + legal review** — mechanism-design and
   counsel, not code ([`gates/economic-simulation-spec.md`](gates/economic-simulation-spec.md),
   [`gates/legal-review-brief.md`](gates/legal-review-brief.md)).

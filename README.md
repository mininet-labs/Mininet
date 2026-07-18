# Mininet

**A free peer-to-peer internet owned by its users, not by a company.**

Mininet is building a network where people own their identity, data, money,
voice, and infrastructure. Money can buy storage, reach, and the funding of
work — but never political power. Governance is one verified human, one equal
vote. There is no owner, no foundation, no admin key, no forced-update path,
no off switch, no law-enforcement backdoor, and no party that can unmask a
user.

> **This GitHub repository is only a temporary public mirror.** The long-term
> code forge is content-addressed, self-governed, reproducibly built, and
> owned by the network itself — GitHub is where the work is shown while it is
> built, never where it ultimately lives (see [`docs/ADDRESSING.md`](docs/ADDRESSING.md)
> and `mini-forge`).

## What no one can change

These are not promises of good behavior — they are structural, enforced in
code, and frozen. A full, code-mapped register is in
[`docs/INVARIANTS.md`](docs/INVARIANTS.md); the short version:

- **Money never buys a vote.** No balance maps to governance or validator
  weight, in either direction — a wall enforced by the dependency graph
  itself, not by policy ([`docs/design/bounty-and-review.md`](docs/design/bounty-and-review.md)).
- **One verified human, one equal vote.** Early arrival, wealth, and hardware
  buy nothing extra. *(Today the system counts verified identity **roots**,
  not yet verified humans — the honest gap is stated plainly below and at the
  top of `docs/INVARIANTS.md`.)*
- **No owner, no admin key, no kill switch, no forced update.** Nobody can
  seize the network, freeze it, unmask a user, or push software you didn't
  choose to run.
- **Offline money is a signed promise, never final ownership** until canonical
  consensus accepts it — so a network partition can never manufacture a
  double-spend ([`crates/mini-settlement`](crates/mini-settlement)).
- **Forking is always free; legitimacy is earned by continuity,** never owned
  by a repository or a trademark ([`docs/design/fork-legitimacy.md`](docs/design/fork-legitimacy.md)).

## What exists today — honestly

This repository is the **self-contained Rust core**: ~40 crates, no external
dependency on any single company's infrastructure to keep running. Nothing
here is ready for real people, real money, or real custody yet — and it says
so, everywhere, on purpose.

**Working, tested Rust:**
- `did:mini` self-sovereign identity + device delegation + lost-device
  recovery
- signed, content-addressed objects; local storage; social feeds; public
  walls; an opt-in `ObjectEnvelope` v2 private-metadata boundary
  (D-0304) — type, author, timestamp, sequence, and links all move
  inside AEAD ciphertext instead of the v1 cleartext schema — plus
  typed, non-delegable capability grants and scoped pseudonyms
  (`mini-objects`)
- BFT finality-verification core; governed release/update path (no forced
  update, no kill switch)
- networked BFT consensus (`mini-consensus`, D-0200–D-0206): a real
  multi-round Tendermint protocol (Buchman/Kwon/Milosevic Algorithm 1) run
  over a real, non-blocking TCP socket mesh — **signed** proposals, signed
  votes (incl. `nil`), locking, quorum certificate, and `mini-execution`
  application all crossing a wire. Independent ledgers converge to
  bit-identical state; a cluster survives a **crashed proposer** by
  round-timeout **view-change**; and messages are **re-gossiped**, so
  consensus is live over any *connected* graph, not just a full mesh (proven
  by a four-node line topology). Safety is complete, proposals are
  authenticated (front-running closed), a wedged peer cannot back-pressure
  honest nodes, and **double-signing is detected** as verifiable evidence;
  every link is now **confidential and tamper-evident**
  (`mini_bearer::Channel`, D-0206) — no consensus byte crosses the wire in
  cleartext; remaining gaps are liveness/DoS and deployment (no state-sync
  for a node that missed a whole height, no slashing layer yet, peers
  supplied not discovered, `Channel`'s handshake is anonymous so it proves
  nothing about *which* validator is on the other end)
- a real TCP transport with a live three-process gossip demo
- `mini`, a real command-line developer tool (`mini-cli`): three
  independent identities on a shared store path can propose, review, and
  governed-merge a commit with no GitHub involved (D-0067)
- an isolated Wasmtime sandbox (`mini-build-runner-wasmtime`) for
  untrusted build-pipeline components: deny-by-default filesystem/network,
  fuel/epoch/memory limits, a 12-point adversarial test suite driving the
  real compiled binary (D-0069). Only this one crate ever links Wasmtime;
  raw shell/native-tool build steps stay unsandboxed and are never
  trusted-provenance-eligible
- TUF-adapted release verification (D-0070): rollback protection, a
  release transparency log with equivocation detection, a device-local
  freshness/staleness bound, and an optional independent
  build-provenance quorum layered in front of `mini-forge`'s existing
  timelocked release/attestation gate
- `mini-installer` (D-0071): real local staging/preflight/owner-approved
  activation/health-check/rollback over an already-verified release —
  atomic symlink-based activation, automatic rollback on a failed health
  check, still no forced update (activation always requires an explicit,
  typed `OwnerApproval` naming the exact release id)
- `mini-privacy-policy` (D-0094): typed cost-doctrine vocabulary
  (protection properties, mechanisms, the five un-removable residual
  floors) and a Tier 0-3 privacy request/achieved-result policy object —
  pure policy data; no relay/mix/erasure mechanism is wired to it yet
- `mini-transport-policy` (D-0301): a `TransportRequest` policy router —
  maps a privacy request to the mechanisms its tier requires, failing
  closed rather than silently downgrading; routing decisions only, not
  wired to `mini-relay` yet
- `mini-resource-pricing` (D-0302): a `PriceVector`/quote engine over
  `mini-privacy-policy`'s declared tier costs, in the workspace's plain
  micro-MINI convention — quoting only, no payment execution, no
  dependency on `mini-value`/`mini-treasury`/`mini-forge`/`mini-chain`
- `mini-relay` (D-0306/D-0307/D-0308): Tier 1 relay + rendezvous
  protocol — role-separated entry/rendezvous/delivery relaying, rotating
  mailbox capabilities (`MailboxGrant`, holder-bound and token-committed
  like `mini-objects`' capability grants but a separate typed domain),
  connection-scoped ephemeral identities never tied to a `did:mini` root,
  role-separation enforcement so no single relay operator holds two roles
  for one delivery, `mini_transport_policy::route()` decisions wired to
  role planning, and an automated `cargo test` proving a message crosses
  two independently-established real TCP+`Channel` hops byte-for-byte —
  hop-by-hop store-and-forward, not onion routing (`MN-205` mix routing
  is a separate, still-gated tier)
- `mini-bridge` (D-0309, `MN-207`): a pluggable entry-transport
  interface — `TransportId`/`TransportCapabilities` naming nine
  transport kinds with declared policy facts, a self-signed
  `BridgeDescriptor` reachability claim with a mandatory (non-optional)
  expiry, a synchronous `PluggableTransport` trait, and one real
  implementation (`DirectBridgeTransport`) dialing a real TCP socket
  through a genuine `mini-bearer` channel handshake — obfs4/WebTunnel/
  Snowflake/Tor-PT adapters are named but not implemented, pending
  audited external implementations
- `mini-private-index` (D-0310, `MN-208`): the privacy boundary between
  public DHT routing and private capability resolution — a frozen
  `LookupPrivacyClass` taxonomy, capability-derived rotating
  `LookupLabel`s via HKDF-SHA256, a signed fixed-size-class
  `PrivateIndexRecord`, and a local `LocalIndex` enforcing signature/
  writer/rollback discipline; doctrine plus one local-only primitive —
  no network, no PIR, no replicated index service yet
- `mini-web-types` (D-0316): the first MiniSearch code slice — pure
  shared vocabulary for canonical public-web URLs, crawl observations,
  explicit availability/restriction states, declared ranking profiles,
  default-no-personalization public search, and result explanations; no
  crawler, index, ranker, query service, network client, or payment logic

**Prototype cryptography — real code, founder-reviewed, NOT yet audited:**
- stealth addresses, linkable ring signatures, Bulletproofs confidential
  amounts (`mini-value`)
- FROST threshold custody (`mini-treasury`)
- Merkle/PDP storage proofs (`mini-spacetime`); real proof-of-replication,
  Stacked Depth-Robust Graph sealing (`mini-porep`); Reed-Solomon erasure
  coding + self-healing shard repair (`mini-erasure`)
- anonymous developer-bounty claims (`mini-bounty`); offline settlement
  protocol (`mini-settlement`)

**Not ready yet, and openly tracked:**
- a mobile or desktop app anyone can install
- BLE / local-radio transport (needs real phone hardware)
- full networked consensus and a live chain
- external cryptography audit — the single largest gate before any real value
- FROST distributed key generation is implemented and tested (Pedersen DKG
  + committee resharing) but not yet externally audited
- a solved, privacy-preserving personhood/liveness proof (open research, not
  engineering debt)
- adversarial testing at real-world scale

The work that **more code cannot finish** — external audits, legal review,
real-hardware testing, and open research decisions — is named explicitly, so
a finished-looking GitHub repo is never mistaken for a launch-ready network:
[`docs/gates/`](docs/gates/) and tracking issue [#99](../../issues/99).

## Start here

Pick the door that fits you:

| You are… | Start with |
|---|---|
| **A curious person** — what is this, and why should it exist? | [`docs/HUMAN_START.md`](docs/HUMAN_START.md) |
| **A developer** — build it, run the demos, find your way around | [`docs/DEVELOPER_START.md`](docs/DEVELOPER_START.md) |
| **An auditor or skeptic** — where are the invariants, threats, and honest gaps? | [`docs/AUDITOR_START.md`](docs/AUDITOR_START.md) |
| **A contributor** — how work is reviewed and merged | [`CONTRIBUTING.md`](CONTRIBUTING.md) |

Beneath everything else in this repository — read before opening the code —
is [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md): the seventeen
principles the whole project is filtered through, written so that a century
from now, someone facing a problem no document anticipated can still reason
the way the founders would have.

## The canonical documents

Mininet preserves its *reasoning* as first-class infrastructure, not just its
code — because a network meant to outlive its creators has to explain itself
to people who will never meet them:

1. [`docs/FOUNDER_DIRECTIVES.md`](docs/FOUNDER_DIRECTIVES.md) — *why the
   project exists and what it values* — the one canonical seventeen-directive
   set (D-0090); machine-readable as
   [`docs/CONSTITUTION_REGISTRY.json`](docs/CONSTITUTION_REGISTRY.json)
   (`FD-01`–`FD-17`, generated by `tools/constitution_registry.py`).
2. [`docs/INVARIANTS.md`](docs/INVARIANTS.md) — *what can never be broken*,
   each row traced Directive → Invariant → Source → enforcing code + test.
3. [`docs/DECISION_LOG.md`](docs/DECISION_LOG.md) — *why each choice was made,
   and when it was superseded* (append-only; main sequence `D-0001`–`D-0094`,
   plus the networking/consensus track's reserved `D-0200`–`D-0206` and the
   privacy/cost-doctrine track's `D-0300`– — see the log's "Decision-number
   allocation across parallel tracks").
4. [`docs/FAILURE_BOOK.md`](docs/FAILURE_BOOK.md) — *what was tried and
   rejected, and why* — read before re-proposing something.
5. [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) — *what could kill the
   project at civilization scale*, and which invariant, if any, is the
   defense.

Living detail: [`docs/STATUS.md`](docs/STATUS.md) (what's actually built, by
domain), [`docs/gates/`](docs/gates/) (external legitimacy gates),
[`docs/audits/`](docs/audits/) (review deliverables),
[`docs/design/`](docs/design/) (design notes),
[`docs/governance/`](docs/governance/) (the founder-supplied Governance
Pack — normative process/specification material, subordinate to the five
documents above; see
[`docs/GOVERNANCE_PACK_INTEGRATION.md`](docs/GOVERNANCE_PACK_INTEGRATION.md)
for what's activated, staged, or founder-only),
[`docs/LEGAL_DISCLAIMER.md`](docs/LEGAL_DISCLAIMER.md) (the project's
constitutional legal position — voluntary participation, no universal
representative, individual legal responsibility, no protocol ownership).
Find anything offline: `python3 tools/mininet_nav.py map` (see
[`docs/NAVIGATION.md`](docs/NAVIGATION.md)).

## License

Public domain (CC0-1.0). Fork it, build on it, run it — own it, together. A
population, not an organization.

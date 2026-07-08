# Implementation status

This is the living account of *what's actually built*, kept deliberately
separate from `docs/DECISION_LOG.md` (which records policy decisions and
their rationale, not ongoing status) per the founder's own review of these
two documents. If a decision log entry's "Implementation status" field and
this file ever disagree, **this file wins** — it's updated far more often
than any individual decision entry is revisited.

Organized by the same nine domains as `docs/INVARIANTS.md`, so a reviewer
can check "is this invariant enforced" and "how far along is the thing
that would enforce it" in one pass across both files.

Status legend: **shipped** (real code, tested) · **partial** (real code
for part of the claim, gap documented) · **prototype** (real code, but
explicitly founder-reviewed only, pending external audit) · **design-only**
(written design exists, no code yet) · **not started**.

## 1. Voice / value

- **shipped** — `ValidatorSet`/governance quorum counting has no weight
  field anywhere (`mini-chain`, `mini-forge::governance`).
- **shipped** — storage/seeding reward accrual (`mini-storage`,
  `mini-reward`), public walls (`mini-social::PublicWall`), base devices
  (`did-mini::BaseDeviceRole`) all confirmed to create no governance
  weight.
- **partial** — BFT finality *verification* is shipped
  (`mini-chain::verify_finality`); the networked consensus protocol that
  produces the votes it verifies is **not started** (roadmap Phase 5,
  [#36](https://github.com/britak420/Mininet/issues/36)-[#45](https://github.com/britak420/Mininet/issues/45)).

## 2. Personhood

- **prototype** — `mini-uniqueness::status` (D-0038): open-ended
  multi-signal `HumanRecord`/`TrustWeights`/`PromotionPolicy` accumulator.
  Real, tested code.
- **design-only / research-blocked** — signal (b), on-device behavioral/
  location entropy proved in zero-knowledge: the whitepaper itself calls
  this unsolved research. Not a code gap; a research gap
  ([#21](https://github.com/britak420/Mininet/issues/21)).
- **HARD LIMITATION, not partial** — every "verified identity" counted
  anywhere in this tree today is a verified `did:mini` root, not a
  verified human. See `docs/INVARIANTS.md`'s hard-limitation section.
  Sybil-resistance at real-world scale is unproven
  ([#18](https://github.com/britak420/Mininet/issues/18)).
- **partial** — co-presence attestation (`mini-presence`) is shipped;
  the software RTT bound has no hardware ranging backing it yet in
  production use (UWB trait scaffolded, not wired to real hardware).

## 3. Identity & key custody

- **shipped** — `did-mini`: KEL, pre-rotation, device delegation,
  detached signing, decoder hardening. Logic-complete, hardened, tested.
- **not started** — post-quantum migration path
  ([#15](https://github.com/britak420/Mininet/issues/15)), device
  hierarchy beyond current single-tier delegation
  ([#14](https://github.com/britak420/Mininet/issues/14)), on-chain
  pre-rotation anchoring (needs the chain).

## 4. Money & finality

- **prototype** — `mini-value`: stealth addresses, linkable ring
  signatures, Bulletproofs confidential amounts (D-0036/D-0040). Real,
  tested, founder-reviewed, **pending external audit** — see `docs/
  audits/issue-8-constitutional-audit.md`'s A1 row.
- **prototype** — `mini-treasury`: FROST threshold signing (D-0041), live
  multi-process demo. **Trusted-dealer keygen, not DKG** — flagged P0 in
  D-0048; see that entry before treating this as production-viable at any
  value level.
- **not started** — the M1/M2/M3 invariants added to `docs/INVARIANTS.md`
  this pass (money never CRDT-merges, offline payment is pending-not-final,
  canonical ordering alone resolves double-spends) have no implementing
  code yet. `mini-reward`'s accrual bookkeeping is ordinary, non-spendable
  value and is unaffected by this gap; anything that becomes real,
  spendable MINI is.

## 5. Updates & forks

- **shipped** — `mini-update::AdoptionState` (local adoption state
  machine, no forced update, no kill path).
- **partial** — `mini-bootstrap` (genesis/capsule protocol logic) is
  shipped; real transport underneath it is not (see §8).
- **not started** — the release registry (on-chain), and therefore
  everything that depends on it: governed release finality, the
  emergency-update-path question ([#53](https://github.com/britak420/Mininet/issues/53)),
  and fork-legitimacy criteria (F1, `docs/INVARIANTS.md` §5) beyond the
  frozen statement of the requirement itself.

## 6. Privacy

- **shipped** — `mini-bearer::Channel` (anonymous, forward-secret,
  handshake carries no identity); `mini-store`'s seed-on-view policy
  gating.
- **not started** — the storage fabric's P6 guarantees (no forced
  replication, no compelled decryption) have no owning subsystem yet.

## 7. Storage

- **prototype** — `mini-spacetime::storage_proof` (D-0039): Merkle/PDP
  challenge-response. Real, tested. **Proves possession, not replication
  uniqueness — see `docs/INVARIANTS.md`'s hard-limitation section.**
  Real proof-of-replication is **not started**
  ([#31](https://github.com/britak420/Mininet/issues/31)).
- **not started** — erasure coding, self-healing replication, cold/
  owner-only storage tiers, huge-file handling at scale (roadmap Phase 4).

## 8. Networking

- **shipped** — `mini_bearer::TcpBearer` (D-0042): real TCP transport,
  tested, proven live via `mini-net`'s three-process gossip demo.
- **partial** — `mini-net`'s gossip logic is proven live over real
  sockets; peer *discovery* (`RoutingTable`) is unexercised over a real
  transport; not a mesh.
- **not started** — BLE/local-Wi-Fi radio adapters (needs real phone
  hardware, [#22](https://github.com/britak420/Mininet/issues/22));
  `mini-bootstrap`/`mini-sync` are not yet wired to `TcpBearer` or any
  real transport; NAT traversal; local mesh routing.

## 9. AI & audit gates

- **shipped (as policy)** — D-0037's AI-authorship-with-human-review
  policy; `mini-forge::governance::PROTOCOL_MIN_APPROVALS`'s 2-approval
  floor.
- **tightened this pass** — D-0047 makes external audit a hard
  *production* gate (not "desirable") for value privacy, treasury
  custody, consensus, and personhood proofs specifically. No code
  path in this tree currently claims production-readiness for any of
  these, so this is a frozen constraint on the future, not a retrofit.
- **not started** — a dedicated "this PR was AI-assisted" flag on
  commits/PRs ([#78](https://github.com/britak420/Mininet/issues/78));
  an actual external audit engagement (not tracked in code at all —
  business/process work).

## What has no client, at all

No mobile, desktop, or web application exists anywhere in this
repository. `docs/UI_BETA_PLAN.md` is a plan, not code. This is a
backend/protocol Rust workspace only.

## Where to look for more detail

- `README.md`'s repository-map table — per-crate one-line status, kept
  in sync with this file but intentionally shorter.
- `docs/BETA_STATUS.md` — the narrower, nearer-term two-phone keystone
  beta target specifically, not the whole system.
- `docs/audits/` — point-in-time audit findings that inform several of
  the statuses above.
- `docs/DECISION_LOG.md` — why each of these choices was made; this file
  only says what's true today, not why.

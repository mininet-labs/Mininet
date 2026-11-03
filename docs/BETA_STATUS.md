# Open Beta status

**Status: OPEN for public testing and contribution; NOT Go-Live and NOT real-value ready.**

Start here: [`BETA_OPEN.md`](BETA_OPEN.md). The Forge/GitHub cutover state is in
[`FORGE_BETA_MIGRATION.md`](FORGE_BETA_MIGRATION.md).

## Beta target

The keystone target remains a real, honest device path: phones can form an
encrypted Mininet link without depending on the public internet, exchange and
verify the intended identity/presence evidence, relay over nearby peers where
the topology requires it, exercise resettable Beta MINI product flows, and
return reproducible evidence about failures and recovery.

Open Beta means **people may test and contribute now**. It does not mean every
acceptance gate has passed. Production MINI, treasury custody, production
personhood, and production cryptography remain separate later gates.

## What is implemented now

### Transport and local networking

PR #333 merged the BLE multi-hop transport slice:

- `mini-bearer::EncryptedLink<B: Bearer>` composes the existing encrypted
  channel with generic bearers;
- `mini-mesh` provides bounded deduplicating multi-hop relay over dynamic links;
- `mini-ffi::MeshHandle` exposes the mesh to Android;
- Android has `BlePeripheralServer`, `BleCentralRadio`, and `BleMeshService` for
  simultaneous advertise/serve and scan/connect roles;
- the relay algorithm has in-process and real loopback-TCP multi-hop tests; and
- Android CI / Android reproducibility were green on the PR #333 merge head.

This is real code, not yet universal hardware evidence. Physical-device testing
across Android versions/vendors, lifecycle/churn, background behavior, radio
failures, multi-hop topology, and product integration remains Open Beta work.

### Identity, presence, replay, and developer harness

The repository already contains:

- `did:mini` identity/delegation/recovery foundations;
- active challenge/response presence timing over the encrypted channel;
- durable replay storage for the keystone path;
- `mini keystone run` as a real CLI harness; and
- the broader no-GitHub developer/release demo path.

Personhood remains unsolved: identity roots are not proof of unique humans.
Hardware-backed ranging assurance must never be claimed above the actual signed
and validated evidence.

### Open Beta findings and contribution flow — PR #334

PR #334 introduces one explicit final-phase path:

```text
campaign/build
  -> structured finding
  -> append-only disposition
  -> Forge task / work claim
  -> implementation, reproduction, or review evidence
  -> accepted contribution receipt
  -> optional Beta MINI participation grant
```

`crates/mini-beta` encodes the campaign/finding/disposition/contribution/grant
objects directly on Mininet's signed, content-addressed object/store substrate.
The existing `mini-forge` coordination layer already provides task briefs,
expiring work claims, task suggestions, and exact-state technical-review
handoffs.

GitHub issue forms are now an adapter to those fields rather than the intended
long-term canonical model.

### Beta MINI — test value only

`mini-beta` also contains a reference Beta MINI ledger for final-phase product
and participation testing:

- explicit non-zero beta epoch;
- testing grants and contribution-backed participation grants;
- integer micro-BETA-MINI accounting;
- per-grant and epoch-supply caps;
- duplicate-grant rejection;
- transfers that mint nothing;
- cross-epoch rejection; and
- epoch rollover that starts at zero with **no carry-over or conversion**.

The crate's runtime dependencies are deliberately limited to `did-mini`,
`mini-objects`, and `mini-store`. Automated tests fail if production value,
settlement, treasury, chain, consensus, Forge governance, personhood/economy, or
airdrop dependencies are added.

Beta MINI therefore does not activate production value and has no automatic
conversion right into production MINI. It cannot buy governance/review/release/
personhood authority through this crate because those dependencies and APIs do
not exist here.

## How to participate

Use [`BETA_OPEN.md`](BETA_OPEN.md) for concrete one-phone, two-phone,
three-plus-phone, offline, lifecycle, malformed/adversarial, accessibility,
Rust, research, and reproducibility itineraries.

For an ordinary report, use the **Beta test report** issue form. It asks for:

- exact revision/release/object;
- evidence class;
- component/surface;
- reporter-claimed severity and concrete impact;
- environment needed to reproduce;
- steps;
- expected/observed result;
- redacted evidence; and
- explicit limitations.

Do not post secrets, stable device identifiers, private location/content, or a
Beta MINI account/claim handle in a public issue.

For a scoped task, use the Contributor intake form or an existing issue. Useful
work includes non-code testing, reproduction, hardware matrices,
accessibility/usability, documentation, security/research, code/tests, review,
reproducibility, and operational evidence.

## Pre-Go-Live identity boundary

During Pre-Go-Live, `docs/governance/52_PRE_GO_LIVE_GOVERNANCE_PAUSE.md`
requires canonical Mininet participation to remain **anonymous**. GitHub
usernames/emails and payment/reward handles are transport metadata, not Mininet
pseudonyms, reputation identities, reviewer credentials, or governance weight.

PR #334 therefore uses fresh artifact-scoped submission/claim/account handles
rather than a persistent contributor profile. Separate contributions must not
be silently linked merely to manufacture reputation continuity.

## What still stands between Open Beta and Go-Live

### Physical/product acceptance

- [ ] Wire the intended BLE/mesh/keystone surfaces into the user-facing product
  path with understandable start/stop/retry/error/recovery states.
- [ ] Prove direct two-phone local operation on physical Android devices.
- [ ] Prove A-B-C (and preferably larger) relay with no required direct A-C edge.
- [ ] Exercise vendor/API-version diversity, permission denial/revocation,
  Bluetooth off/on, screen-off/background/process restart, churn, reconnect,
  slow peers, duplicate paths, resource pressure, and malformed input.
- [ ] Record privacy-safe diagnostics and exact residual limitations.

### Beta MINI product/durability work

- [ ] Expose unmistakably labelled BETA wallet/grant/transfer UX. *Done:* the
  `mini beta` CLI (accounts, balances, transfer, grant policy/grant/approve/status)
  and a desktop **BETA wallet** panel (register account, refresh balances, send,
  create a grant policy, create a grant, approve a grant, check grant status) served by new application-core commands. Every surface labels amounts BETA,
  provisional and not production MINI. *Not done:* the desktop panel's layout has
  not been exercised on a real Windows display (its input parsing, request
  building, protocol framing and core handlers are unit-tested). Grant
  policy creation and grant creation are now also in the desktop panel (same
  checks as the CLI, via non-inserting `build_*` library twins so the exact
  signed object is journaled before commit; retry-safe like approval). They
  are signed by the PRIMARY identity, which must be the campaign's author, so
  the author and every policy member appear as public DIDs. The policy's
  start must be no earlier than its creation moment and grants can only be
  made once the window has started (the panel offers an absolute suggested
  window). Not done: this creation UI has likewise not been exercised on a real
  display. Grant approval is signed by the user's PRIMARY
  identity, not a per-account one, because policy members are public DIDs by
  design, so an approval publicly links that DID to the approved grant. Grant
  status counts only approvals present in the local store (found by scanning
  objects linking to the policy; invalid or non-member ones are ignored), an
  accepted grant is evidence rather than a conversion, and status fails closed
  when a policy has more approvals than the library's evidence limit. The desktop path is retry-safe and unlinkable:
  register/transfer/approve carry an operation id and the exact signed object is
  journaled before it is committed, so a lost response never causes a second
  signature; each BETA account is signed by its own identity derived (HKDF, never
  stored) from the unlocked root seed and a random per-account nonce, with its
  KEL carriers published for verified sync, so public BETA objects carry no primary
  DID. The list of owned accounts is local state (registration receipts), so a
  profile restored from the seed alone does not see its old accounts. The `mini beta` CLI works the same way: accounts
  and transfers are signed by per-account identities derived from the CLI root
  seed and a random nonce (`beta-accounts/` markers hold the local owned-account
  list), never by the CLI's primary DID.
- [x] Persist/replicate beta grant/transfer events rather than relying only on
  the reference in-memory accounting core. Balances are derived from the
  durable signed-object set (`mini-beta-exec`), so any replica that holds the
  objects derives the same provisional snapshot; there is no balance database.
  A two-node replication test is still outstanding.
- [ ] Provide a private fresh claim path for accepted participation without
  binding public GitHub identity to the beta account.
- [ ] Prove epoch reset/retirement through the actual product surface. *Proven through the CLI by `epoch_rollover_retires_old_value_and_carries_nothing_forward` (balances and accounts do not carry into a new epoch, old accounts cannot act there, history is unchanged); the application core that backs the desktop BETA wallet is proven the same way by `epoch_rollover_through_the_application_core_retires_old_value` (no carried accounts or issuance, old accounts cannot send or receive in the new epoch, old-epoch history untouched), and the panel's stale-view drop on epoch/campaign change is unit-tested. Not yet exercised: a real desktop session on a real display, so the box stays open.*

### Forge canonicality

- [x] Discover a beta campaign without GitHub. *(single replica: `beta_native_loop.rs`)*
- [x] Submit/replicate a finding without a GitHub account/API. *`crates/mini-cli/tests/beta_two_node_loop.rs`: two independent stores connected only by a real TCP `mini sync` replicate the campaign, finding, account, grant and approvals, and derive an identical BETA snapshot digest.*
- [x] Disposition it and produce a native Forge task. *(single replica: `beta_native_loop.rs`)*
- [ ] Claim the task and hand off exact-state review evidence natively. *Claiming is covered by `beta_native_loop.rs`; the exact-state review-evidence handoff is not yet part of that test.*
- [x] Record an accepted contribution and Beta MINI grant natively. *(single replica: `beta_native_loop.rs`)*
- [ ] Complete that entire loop during a GitHub outage. *The loop's code paths have no GitHub dependency and run in `beta_two_node_loop.rs` over loopback only; a recorded outage drill on separate machines is still to do.*
- [ ] Make GitHub a mirror/adapter rather than the authority that decides which
  beta/work state is real.
- [ ] Implement/prove the one-way `forge_canonical = true` transition and the
  irreversible shutdown of bootstrap canonical integration at Go-Live.

### Substantive production safety gates

Open Beta does not waive:

- external review of production-value cryptography/custody/settlement paths;
- exact settlement/finality correctness;
- release/update safety and voluntary owner adoption;
- privacy/threat-model findings;
- personhood's unresolved unique-human problem; or
- KEL freshness/revocation anchoring needed by high-value decisions.

## Build and test

The repository CI contract remains:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
```

For the new Open Beta core specifically:

```sh
cargo test -p mini-beta
```

The PR #334 branch now includes the `mini-beta` workspace package in the root
`Cargo.lock`; the lock entry is generated by Cargo and the temporary repair
workflow removed itself after producing that commit. This removes the stale-lock
implementation defect, but only the normal exact-head `--locked` CI and
reproducibility workflows are acceptance evidence. A generated lockfile is not
self-authorizing proof that the branch builds.

A passing suite proves implementation properties exercised by those tests. It
does not replace physical-device evidence, an external audit, or Go-Live
activation.

## Overall readiness statement

**Open Beta: YES.** People can begin producing useful evidence and contributing
to the final phase now.

**Production / Go-Live: NO.** Physical acceptance, durable/user-facing Beta MINI,
Forge-independent operation, the one-way Forge-canonical handoff, and the
remaining substantive safety gates still have to be proven.

## Storage guard (full-disk protection)

- `mini_durable::space`: every `atomic_replace` (all durable state: journals, manifests, seed objects, KELs, caches) first checks free space. If short, registered safe reclaimers run (built in: interrupted-write temporaries older than ten minutes), and only then does the write fail, cleanly and before touching anything, with a plain `StorageFull` error. An 8 MiB reserve is kept so small control writes still commit. If the platform cannot report free space the guard fails open.
- `mini-cli` runs housekeeping at the start of every command: when space is low it prunes expired seed leases and (when critical) drops the rebuildable inventory cache, and prints a plain-language warning to stderr. `mini storage status` reports the level.
- Not yet: warnings in the desktop and Android UI (they can call `space::level`/`warning`), reclaimers for other regenerable caches, and guarding the few writers that bypass `atomic_replace` (e.g. staged-file publish). The evidence archive is intentionally never reclaimed.
## Automatic housekeeping (no engineer needed)

- **Evidence journal:** when the seed evidence journal (or one peer's share of it)
  reaches 75% of its bound, the oldest receipts move automatically to
  `seed-evidence-archive/`. Nothing is deleted. Replay protection survives
  archiving via an append-only `replay.jsonl` index; `earnings` and
  `payment-export` read the archive too. There is no "archive explicitly" step.
- **Disk full:** I/O errors that mean "out of space" now say, in plain language,
  to free space and retry; writes are atomic so nothing is lost.
- **Not yet automatic:** the other durable stores (BETA operation journals,
  KEL/object stores, seed pool eviction) still fail closed when full without
  self-cleaning; proactive free-space checks need a platform statvfs and are not
  built. The archive itself grows until the disk does.

## Escalating admission cost (spam / flood protection)

- `mini-transport-security::admission` (`AdmissionMeter`): each admitted request from a source inside a sliding 5 s window raises the work the next one must prove by one bit (expected hashing doubles per extra request); quiet for a window and it resets. Past a per-source ceiling (default 40 bits) the source is refused. A global term raises everyone's required bits when total load exceeds a soft limit (capped lower), so a botnet spread across many sources still pays. Work is a BLAKE3 hash bound to the request digest and a per-window sequence number, so it can't be reused or replayed. Cost is compute, not money: ordinary internet access stays free.
- Not yet: wiring into the live listeners (seed, sync, relay, app service), per-class tuning, and any adversarial load testing. GPU/ASIC/botnet attackers pay less per unit than phones; this raises cost and rate-limits, it does not make flooding impossible, and must sit beside the existing connection/bandwidth quotas and peer-selection diversity. `source` must be unspoofable (authenticated session identity or an established-connection IP prefix). Unaudited.

## Escalating admission cost is live on `mini service run`

- `mini service run` now meters each connecting source (IPv4 address, or IPv6 /64). The first five connections in any 5 s window are free and need no extra round trip. After that the server answers the hello with a challenge, the client grinds BLAKE3 work bound to its own hello (cost doubles per extra connection), and only then does the handshake continue. Past 40 bits the source is refused. The authenticated sync client and `mini seed` client solve challenges automatically (they refuse work above ~2^26 hashes and report "peer is busy, try again shortly").
- Not yet covered: the one-shot listeners (`sync listen`, `sync serve-release`, the remote build worker), the desktop delivery listener in `mini-app-service`, relay/DHT listeners, and any per-class tuning or load testing. Behind NAT or a shared IP, many honest users share one meter. GPUs/botnets pay less per unit than phones: this raises cost and rate-limits, it does not make flooding impossible. Unaudited.

## Admission cost: more listeners

- The handshake gate now lives in `mini-transport-security::AdmissionGate` (generic over any bearer; one wire format). It covers `mini service run`, `mini sync listen --repeat` (with the `sync connect` client solving challenges) and the desktop delivery listener in `mini-app-service` (a per-service shared meter; the delivery sender solves challenges, bounded by its 5 s session deadline, so a heavily throttled sender simply retries later through the outbox).
- Still uncovered: single-accept one-shot servers (`sync serve-release`, the build worker) where a gate cannot help, and relay/DHT listeners. The delivery-listener gate has no dedicated integration test yet beyond the shared gate tests.

## Storage warnings and automatic space-saving

- **Housekeeping framework** (`mini_durable::space`): `housekeep(dir)` checks free space once and, only when the device is `Low` (< 256 MiB) or `Critical` (< 8 MiB), runs the built-in sweep of interrupted-write temporaries (now recursive, four levels deep, never following symlinks, ignoring files newer than ten minutes) plus any registered housekeepers, then reports the level before/after and the bytes recovered. Housekeepers run at safe points where no durable-state lock is held; inline reclaimers still run inside writes.
- **Registered today:** the CLI's seed housekeeper (expired seed leases; at `Critical`, the rebuildable inventory-verification cache). `mini` runs housekeeping at the start of every command; `mini storage status` reports the level.
- **App core:** new `StorageStatus` command / `Storage { level, free_mib, freed_kib }` reply. The core runs housekeeping on its profile directory before answering.
- **Desktop:** asks about once a minute and shows a plain-language banner at `Low`/`Critical` that says what Mininet already did and what to remove; no banner when space is fine.
- **Not done:** Android has no storage UI yet (its Kotlin shell does not use the app protocol). The big consumers — the object store, media blobs and the delivery outbox — have **no safe eviction path yet**: the store is append-only with no delete API, and the outbox's idempotency check needs the stored object bytes, so nothing there is auto-deleted. Evicting `EphemeralCache`-tier content needs a store design decision first.

## Automatic cache eviction with an owner-chosen retention

- **Policy:** other people's re-fetchable cached content (posts, comments, reactions, media manifests and media chunks) is removed automatically once it is older than the owner's retention: **7 days, 30 days (default), 6 months, or never**. Age is measured from when *this device* first saw the object (`seen/` row), never from an author-claimed timestamp. Under space pressure the retention shortens (Low: at most 7 days; Critical: at most 1 day) — **except "never", which is always honoured** (the owner is warned instead).
- **Never evicted:** the owner's own content; anything outside the allow-list (keys, KEL carriers, vouches, attestations, releases, heads, profiles, communities, forge objects, private envelopes); anything not in the default ephemeral tier (pinned, seeded, committed, cold-archive, private); anything a retained object still links to (a kept reply keeps its parent).
- **Mechanism:** `mini_store::Store::evict_cached` validates all-or-nothing, then removes the enumeration row first, every other index row, and the blob last, so a crash can leave an orphan but never an indexed object without content; the filesystem time index is rebuilt after a batch. `Backend` gained default-refusing `delete_blob`/`delete_meta`/`finish_deletions` (only the memory and filesystem backends implement them).
- **Surface:** `CacheRetentionGet`/`CacheRetentionSet` and a `CacheRetention` reply in the app protocol; a "Storage" card in the desktop System view (current headroom and the retention choice). The pass runs with each `StorageStatus` check (about once a minute from the desktop), bounded to 20,000 objects examined and 2,000 removed per pass.
- **Not done:** Android UI; evicted content is re-fetched only if a peer offers it again (there is no active re-fetch); unpinned content the owner *viewed* is still evictable unless the view promoted it to a seeding tier; the delivery outbox and the owner's own content have no automatic compaction.

## Follow-through on the "not done" list

- **Eviction no longer thrashes.** An evicted object leaves a tiny `evicted/<id>` marker and `mini-sync` uses `Store::wants` (not held *and* not evicted), so the next sync does not download it again. Markers lapse after about a year (pruned at most every six hours, 2,000 per call); after that a peer that still offers the object can serve it again and it ages out afresh. `Store::forget_eviction` exists for an explicit user action. This replaces the earlier "evicted content returns only if a peer offers it again" caveat: the intended behaviour is that it does not return on its own.
- **Android:** a storage banner (`StorageGuard`) warns below 256 MiB and below 8 MiB free using `StatFs`, checked each minute. The Android shell has no content cache, so there is no retention control there; the desktop core owns retention. Not compiled in this sandbox (CI builds it).
- **Personhood gate in decisions (opt-in, off by default):** `mini_forge::EligibleOnly` wraps any `IdentityOracle` so only roots that pass a caller-supplied predicate (and their devices) count toward a quorum; it only removes roots, adds no weight, and does not depend on any personhood or value crate. `mini_uniqueness::evaluate_roll` / `eligible_set` evaluate a candidate roll for a gated action (a payout list) under the stage in force. No quorum or payout uses them yet.
- **Policy hardening from the simulator:** a score now requires *live* seed-anchored vouching evidence (without it the score is capped below the audit window), and physical presence may lift the score at most `PRESENCE_LIFT` (15) points above the vouching strength. `POLICY_FORMAT_VERSION` is 2.
- **Left as they are, deliberately:** (1) relay/DHT listeners — no live accept loop exists in `mini-net`/`mini-relay` today (they are protocol layers over caller-supplied bearers; `mini-consensus` accepts single connections), so there is nothing to gate yet; `AdmissionGate` is ready for when one exists. (2) The delivery outbox is already bounded by its retained-operation cap and its records carry no timestamps or compactable form, so it is not auto-pruned. (3) The owner's own content is never removed automatically; a user-driven delete is a different feature.

## Graded standing (policy v3)

`PersonhoodPolicy::standing` grades roots Full / Provisional / Unvouched; value is split into pay-now and held (never forfeited) and ballots count when the root is Full at close (`mini_uniqueness::tally`); `vouch_strengths` turns graph trust into a per-node strength that does not lock newcomers out (three vouches reach Full) while making a fake identity cost about three colluder vouches. Nothing in a payout or quorum path calls these yet; enabling them is a deliberate step. See the design note for the simulator tables and the remaining limits.

- **Graded airdrop payouts (opt-in):** `mini_airdrop::add_graded` / `add_release` apply a published `PersonhoodPolicy`'s standing to snapshot building: Full is paid in full, Provisional gets the policy share now with the rest reported as held, Unvouched gets nothing now. Held value is released at Full standing, never forfeited. Callers must store the held amounts durably; this crate does not. Quorum voting (`tally`) is still not wired into forge/chain voting. Counts identity roots, not humans; unaudited (D-0047).

- **Community votes (library):** `mini_social::community_vote` counts up/down reactions on any object (a post or a proposal written as a post) for a community at a chosen close time. No new wire format. Only current members count, one equal ballot per identity root, only those at Full standing at close; others are reported as not counted. Not yet exposed in the app protocol or UI. Counts identity roots, not humans; reaction timestamps are author-declared, so a ballot can be backdated to slip in before the close, but that cannot add weight beyond one Full member's single ballot.
- **Outbox no longer stalls at 1024 records:** when full, `enqueue` first prunes delivered/cancelled records older than 30 days, then (last resort) the single oldest finished record. Queued, retrying and in-flight records are never dropped. Cost: a retry of an operation ID whose finished record was pruned could be sent again as a duplicate (receivers also dedupe by object ID). Age is taken from the record file's modification time.

- **Community vote command:** `Command::CommunityVote` / `Reply::CommunityVote` (tags 50 / 49) return yes/no/not-counted for a post in a community at a close time. This device has no personhood evidence store yet, so the reply carries `standing_checked: false` and counts every member's ballot; a UI must say so. No desktop/Android screen yet.

- **Desktop vote result:** each top-level discussion in an open community has a 📊 Result action showing for / against / not counted, with a plain note that personhood standing is not checked on this device. The ⬆ arrow publishes a Like, which `community_vote` now counts as a yes; the app has no downvote yet, so "against" stays 0 from the app. No Android screen yet.

- **Downvote + close time (desktop):** `Command::SetDownvote` (tag 51) publishes a downvote with the same operation-ID idempotency and journal as likes (journal tag 11), and downvotes are now delivered to peers (delivery previously rejected anything but Like). The desktop board has a ⬇ button and a per-session "Still open / closes after 1 day / 7 days" choice for the result count. The board's score column still shows likes only. Android has no social or community UI at all, so there is no Android vote screen to add.

- **Owner/seeder economics (unsettled credit, D-0543):** service tickets v2 carry a signed creator share and the posts/media received, so authors of any received content earn a share (never below a 20% published floor) and the seeder keeps the rest. The Earnings view shows what you paid, split between seeders and owners. Nothing is paid out: settlement is still gated behind the external audit (D-0047). Redemption requests still state their own share, so the settlement layer must use the tickets' signed share. Private-route exchanges attribute nothing.

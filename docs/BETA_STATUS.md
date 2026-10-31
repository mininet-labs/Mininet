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
  and a desktop **BETA wallet** panel (register account, refresh balances, send)
  served by new application-core commands. Every surface labels amounts BETA,
  provisional and not production MINI. *Not done:* the desktop panel's layout has
  not been exercised on a real Windows display (its input parsing, request
  building, protocol framing and core handlers are unit-tested), and grant
  approval is CLI-only for now. The desktop path is retry-safe and unlinkable:
  register/transfer carry an operation id and the exact signed object is
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

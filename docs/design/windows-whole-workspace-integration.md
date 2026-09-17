# Windows whole-workspace integration

**Status:** implementation scope / review contract  
**Target:** `mini-desktop` Windows product  
**Inventory:** every Rust workspace member is classified in `crates/mini-desktop/windows-crate-map.tsv`  
**Non-goal:** make every crate a direct dependency of `mininet-desktop.exe`

## 1. Objective

The Windows product should expose the useful Mininet system as one coherent application without turning the rendering process into the entire network.

"Use all crates" therefore means:

1. every workspace crate has an explicit Windows product decision;
2. every runtime-relevant crate is reachable through a tested user flow, application service, optional node role, isolated worker, setup/diagnostic tool, or another explicitly named boundary;
3. platform-specific development/mobile/legacy crates are explicitly classified instead of silently omitted; and
4. adding a workspace crate without deciding its Windows role fails a `mini-desktop` test.

It does **not** mean linking all workspace crates into one executable. That would increase attack surface, mix unrelated authorities, put long-running node work on the UI boundary, and make the existing voice/value and build-sandbox separations harder to prove.

The source-of-truth matrix is:

```text
crates/mini-desktop/windows-crate-map.tsv
```

`crates/mini-desktop/tests/windows_crate_map.rs` mechanically compares that table with the root workspace and with the actual direct Mininet dependencies of `mini-desktop`.

## 2. Current state

This W1 implementation branch contains 86 workspace crates: the original 84 plus `mini-app-protocol` and `mini-app-service`. `mini-desktop/Cargo.toml` directly links 14 Mininet crates: the prior 13 plus the zero-authority application protocol. The service implementation itself is deliberately not linked into the renderer. That direct-dependency count is not the same as product coverage: PR #345 separately established whole-workspace **diagnostic** classification, and the connected-client work already composes additional behavior through those dependencies.

The important gap is architectural: repository capabilities such as native search, relay/mesh, storage roles, Forge, personhood/presence, value/settlement, and network consensus do not yet have one consistent Windows application-service boundary and honest end-user lifecycle.

The connected-client design already points to the correct first move: a single application service that serializes store mutation/signing, keeps UI and key custody separate, and emits bounded events to clients. This design makes that service boundary the spine for whole-workspace integration.

### 2.1 W1 implementation now present on this branch

The architecture is no longer only a scope document:

- `mini-app-protocol` is a dependency-free, versioned, length-framed contract with explicit commands, typed errors, bounded strings/collections, and no generic execute or filesystem primitive;
- `mini-app-service` is a per-user process that owns DPAPI-backed controller reconstruction while unlocked, a process writer lock, durable sequence reservation, exact-signed-object publish recovery, idempotency receipts, feed materialization, and a bounded event backlog;
- the desktop launches that process as a sibling executable through a bounded command queue and fails closed for migrated signing actions if it is absent;
- onboarding root/profile creation, identity lock/unlock, plain Home posts, and Home/Explore feed snapshots use the service;
- Windows packaging ships `mininet-app-service.exe`, and native/cross-Windows CI includes the protocol and service.

The first transport is child-process stdin/stdout using the same bounded wire
contract. Per-user Windows named pipes remain the preferred next transport
hardening because they can add OS-level same-user/session access control without
changing command semantics.

This is substantial W1 progress, not W1 completion. Replies/reactions,
community and rich-profile mutations, media publication, messaging, sync/store
ingest ownership, and several connection workflows still open/sign/mutate
through the legacy desktop `Workspace`. Until those are migrated, the
application service is the authority for the listed W1 flows but the repository
must not claim the renderer has become a universal zero-authority client or
that the one-writer rule covers every existing beta mutation.

## 3. Target process topology

### 3.1 `mininet-desktop.exe` — presentation and user intent

The GUI owns:

- navigation, rendering, accessibility and local presentation state;
- explicit user intent and confirmation;
- bounded cached view models; and
- displaying service health, maturity, errors and offline state.

The GUI should progressively stop owning:

- signing keys and direct signing operations;
- direct store mutation;
- consensus/node loops;
- wallet/treasury state transitions;
- Forge governance/build execution;
- crawler/extractor execution; and
- heavyweight proof/sealing work.

A browser-style shell is allowed to be rich. It should not be the trust boundary for every subsystem.

### 3.2 Core application service — identity, objects and everyday application state

A per-user Mininet application-service process should become the normal backend for:

- `mini-durable`, `mini-crypto`, `did-mini`;
- `mini-objects`, `mini-store`, `mini-crdt`;
- `mini-sync`, `mini-messaging`, `mini-social`, `mini-media`;
- privacy/publication/transport policy;
- local search/query/index state;
- bootstrap, presence and uniqueness surfaces as their real hardware/protocol paths mature.

It is the **single writer** for normal application state. Workers and UI request mutations through typed commands instead of independently opening mutable stores. Read snapshots/events are bounded and versioned.

This service is a per-user application process, not an administrator-owned Windows Service. It must preserve the existing per-user/no-admin product model and owner-controlled networking defaults.

### 3.3 Optional node service — storage, connectivity and network roles

A separately startable node process owns roles that may consume sustained CPU, disk, bandwidth or inbound sockets:

- `mini-storage`, `mini-net`, `mini-dtn`, `mini-spacetime`, `mini-porep`, `mini-erasure`;
- `mini-resource-pricing`, `mini-relay`, `mini-bridge`, `mini-provider`;
- `mini-replication-policy`, `mini-storage-fraud`, `mini-mesh`;
- federation networking; and, only when explicitly enabled, chain/consensus/execution roles.

The owner chooses whether this process runs, what it may serve, storage/bandwidth limits, metered-network policy and whether it starts with the app. Stopping it must not remove the user's identity or local content.

### 3.4 Wallet/value service — value without governance coupling

Value-sensitive crates belong behind a separate local process boundary:

- `mini-value`, `mini-treasury`, `mini-custody`;
- `mini-private-payment`, `mini-settlement`, `mini-bounty`;
- `mini-engagement`, `mini-attest`, `mini-airdrop`, `mini-airdrop-treasury`;
- `mini-economy`, `mini-contribution`; and related verification/policy paths.

The desktop may show balances, pending claims, finalized settlement, service earnings and explicit payment actions, but it should communicate with this process through a narrow protocol rather than acquiring a value dependency graph itself.

This keeps the voice/value wall in the architecture rather than hiding value from users. The user can use wallet and Forge features in the same application window while the code that handles balances cannot become governance vote weight or silently join the Forge authority graph.

Real-value activation remains subject to the repository's external audit and governance gates. A Windows screen is not a production-readiness declaration.

### 3.5 Forge service and isolated build workers

Forge functionality belongs in its own service domain:

- `mini-forge`, `mini-provenance` and coordinator-side pipeline state;
- `mini-pipeline` and `mini-pipeline-protocol`;
- `mini-build-runner-wasmtime` as a spawned, isolated worker.

The user-facing Windows Forge can show repositories, tasks, claims, reviews, build evidence, governed merge/release state and contributor workflows. It must not load Wasmtime/compiler capability into the renderer and must not merge wallet/value authority into review or governance.

Build workers receive only the bounded inputs/capabilities required for a job and return deterministic evidence through the existing protocol boundary.

### 3.6 Isolated search/intake workers

Public-web and untrusted-document work is intentionally worker-shaped:

- `mini-crawler-fetch` performs explicit, policy-approved outbound fetches;
- `mini-extract-host` owns extractor lifecycle;
- `mini-extract-protocol` bounds the IPC contract;
- `mini-web-extract` runs on untrusted document content;
- indexing/ranking/query state returns through the application service.

Opening the application must not start crawling the web. External intake is owner-enabled, resource-bounded and provenance-preserving.

### 3.7 Setup, diagnostics and tooling

Some crates are product-adjacent executables rather than runtime backends:

- `mini-windows-setup` / `mini-setup` own the Windows install lifecycle;
- `mini-selftest` and `mini-value-selftest` remain diagnostics, preferably spawned rather than linked into the GUI;
- `mini-cli` remains developer/operator tooling;
- `mini-econ-sim` remains research tooling;
- `mini-ffi` is a mobile-host boundary;
- `mini-installer` is the legacy POSIX installer.

These are still classified in the Windows map so "all crates" never becomes "all except the ones we forgot." Platform-excluded means deliberately not a Windows runtime dependency, not unowned.

## 4. Local IPC contract

Windows integration should use a small, versioned local protocol rather than ad-hoc cross-process calls.

Preferred final transport: per-user Windows named pipes, with a loopback or child-process stdio transport allowed for portable CI and staged migration. The current W1 implementation uses bounded child-process stdio; moving the same protocol onto a same-user named pipe is transport hardening, not a command redesign. The protocol must have:

- explicit version and message type;
- bounded frame size and collection counts before allocation;
- request id, deadline and cancellation where work can block;
- capability-scoped commands rather than a generic "execute" message;
- typed errors with retryability and user-action hints;
- bounded event subscriptions with backpressure/coalescing;
- peer-process identity restricted to the current user/session where Windows permits it;
- no raw private key export and no generic filesystem read primitive;
- deterministic serialization for security-sensitive requests;
- restart-safe idempotency for mutations that can be retried; and
- protocol fuzz/adversarial tests independent of the GUI.

The first service protocol should be intentionally narrow: workspace status, identity lock/unlock/sign intent, feed/query snapshots, store-backed mutations, connection policy, outbox status and bounded events. Add new command families only with the vertical slice that consumes them.

## 5. Product surfaces

The crate map assigns every crate to one or more user concepts. The Windows information architecture should converge on these surfaces rather than exposing crate names as menus:

| Product surface | Main capabilities |
|---|---|
| Home / People / Communities | social graph, posts, reactions, profiles, presence when hardware permits |
| Messages | private conversations, attachments, delivery/outbox state |
| Media / Library / Creator | content-addressed files, playback, seeding, publishing, channels |
| Connections | peer discovery, sync, relay/bridge/mesh policy, zero-network mode |
| Search | local index, native/federated search, optional public-web intake |
| Services / Earnings | provider role, signed service tickets, storage/bandwidth work |
| Wallet | balances, payments, settlement states, private payment, airdrop/economic flows when gated |
| Forge | repos/tasks/review/build/provenance/release evidence, never purchased governance |
| Node | optional storage/mesh/consensus/testnet operation with resource controls |
| Identity / Privacy | keys, delegated devices, privacy policy, personhood evidence maturity |
| System / Diagnostics | component health, exact integration maturity, self-tests and installed version |

A crate can support a surface without becoming visible as a button. Low-level crypto, durable storage, erasure coding and protocol framing should normally appear as capabilities and health, not as implementation jargon in everyday navigation.

## 6. Implementation waves

The TSV's `wave` column is the executable migration sequence.

### W0 — inventory and existing Windows boundaries

Land the exhaustive map and drift tests; retain the existing verified installer and diagnostics process boundaries. No claim that all crates are implemented follows from W0.

**Exit:** every workspace crate classified; direct dependency classification is mechanically correct; architecture reviewed.

### W1 — application-service spine

Introduce the typed per-user application service and move normal state mutation/signing behind it. Port current social, messaging, media, sync, identity, vault and update-status flows without feature regression.

**Exit:** the GUI can be killed/restarted without corrupting service state; one-writer rule proven; identity stays locked by default; offline mode works; current Windows UX is service-backed.

### W2 — network, storage and provider roles

Add optional node-service lifecycle, storage/seeding, erasure/proofs, DTN, relay/bridge/mesh, resource pricing and provider/ticket flows with owner budgets.

**Exit:** two Windows machines exchange/resume content; node can stop independently; service tickets correspond to real exchanged work; resource limits are enforced; no provider is a trust root.

### W3 — search, intake and extraction

Wire local query/index/ranker first, then federated search and explicit public-web intake through isolated fetch/extract workers.

**Exit:** local search is indexed and bounded; federation works across real peers; public-web fetch is opt-in; extractor compromise has no direct key/wallet/store-write capability; ranking is explainable.

### W4 — Forge, bootstrap, presence and identity maturity

Expose native Forge workflows through the Forge service; wire bootstrap/recovery and real BLE/UWB presence when hardware evidence exists; surface uniqueness/personhood evidence without overstating it.

**Exit:** GitHub-independent Forge workflow is usable from Windows; build workers are isolated; real hardware tests exist for presence/bootstrap claims; identity-root vs verified-human distinction remains explicit.

### W5 — wallet, economics and commercial flows

Integrate the wallet/value process and gated economic/business workflows. Preserve settlement state distinctions and the value/governance process wall.

**Exit:** funded testnet flow covers receive/send/private payment/pending/finalized/service redemption where implemented; dependency checks prove wallet cannot buy Forge/governance authority; required external reviews are recorded before any production-value label.

### W6 — optional full-node/consensus mode

Expose execution/consensus/shielded verification only as an explicit node role with synchronization, resource budgets and testnet/production maturity labels.

**Exit:** multiple real Windows nodes converge/fail over through supported transports; restart/state-sync is proven; external audit and dynamic-validator limitations are not bypassed.

### W7 — tooling and intentional exclusions

Document Windows-facing equivalents/import/export for CLI/research/mobile/legacy-installer crates without loading them into the app runtime merely to satisfy a count.

**Exit:** no workspace crate is ambiguous: each is either delivered through a product/service path or deliberately excluded with an owner-facing/developer-facing rationale.

## 7. Definition of "implemented in the Windows app"

A runtime-relevant crate is not counted as integrated merely because Cargo can link it. It is integrated only when all applicable items are true:

1. **Reachability:** a real Windows user flow or enabled node role reaches its behavior.
2. **Boundary:** the crate runs in the process class declared in the map.
3. **Truthful maturity:** the UI states prototype/testnet/audit/hardware limits where required.
4. **Lifecycle:** start, stop, restart, cancellation and failure are handled without corrupting state.
5. **Security:** secrets/capabilities are no broader than the task needs; untrusted parsing/execution is isolated where planned.
6. **Persistence:** state has explicit ownership, migration and crash-recovery behavior.
7. **Networking:** network work is user-policy-controlled, bounded and testable in zero-network mode.
8. **Tests:** unit plus cross-process integration tests cover success and refusal/failure paths.
9. **Windows evidence:** Windows CI builds/runs the relevant process; hardware-dependent claims have physical evidence.
10. **Dependency gates:** cargo-tree/topology tests preserve the value/governance and build-runner boundaries.

Tooling/platform-excluded crates satisfy the contract by having the explicit non-runtime rationale and a supported product/developer path; they are not forced into the GUI.

## 8. CI and review gates

Every wave should extend, not replace, the current Windows checks:

- `cargo fmt --all -- --check`;
- strict Clippy on changed/runtime crates;
- `cargo test -p mini-desktop` including the map exhaustiveness tests;
- targeted Windows tests for DPAPI, named-pipe ACL/lifecycle and MSVC linking;
- process-topology/cargo-tree checks proving sensitive dependency walls;
- installer/package lifecycle tests;
- no-network startup test;
- cross-process crash/restart tests;
- two-instance and then two-machine integration tests;
- cross-NAT relay/failover evidence before "internet beta" claims; and
- measured weak-device budgets for indexing/media/node work.

The connected-client performance targets remain useful acceptance budgets: 4 GB RAM / two-core Windows reference hardware, warm launch under 2 s, responsive input/rendering, bounded text-feed memory, indexed query latency independent of old history, and idle CPU under 1%. They are targets until measured on a named revision and machine.

## 9. What this scope deliberately refuses

- Adding every crate to `mini-desktop/Cargo.toml` just to improve a number.
- Giving the GUI direct treasury, consensus, governance or compiler authority.
- Treating a diagnostics check as proof that an end-user workflow exists.
- Turning a loopback TCP test into a claim of internet connectivity.
- Presenting identity roots as verified humans while personhood remains unresolved.
- Presenting proof-of-space-time as proof of unique replication.
- Treating an offline/local payment as final ownership.
- Starting crawling, relaying, hosting, seeding, consensus or updating merely because the app launched.
- Requiring any single hosted provider for identity, local data or ownership continuity.
- Calling real-value, consensus, personhood or privacy functionality production-ready before its external gates are satisfied.

## 10. W1 implementation sequence

This branch implements the first authority-bearing W1 slice — **root/profile
onboarding + identity lock/unlock + local feed read + signed plain-post publish
+ bounded event notification + crash/idempotency recovery** — and packages the
service with the client.

The next W1 work should extend the same capability-scoped protocol rather than
create parallel mutation paths: replies/reactions/follows and rich profile
editing first, then communities/media/messaging/sync ingest, and finally removal
of renderer-owned signing/store-write dependencies once every current beta flow
has a service-backed equivalent.

That establishes the pattern all later crates use: UI intent -> typed local IPC
-> capability-scoped service -> store/network/worker -> bounded result/event.
Once W1 is fully migrated, later waves become composition rather than repeatedly
adding privileged logic to `main.rs`.

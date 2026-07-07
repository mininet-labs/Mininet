# Mininet UI Beta — Roadmap, Epics, Sprints, Tasks

**Goal:** one client, every surface (SPEC-09): the personal social layer
(microblog/photo/short-video feed), communities (threaded forums), the creator
space (media publishing), and the **forge portal** (the decentralized
"GitHub" through which Mininet updates itself) — accessible from phones,
desktops, and the web, offline-first, with no server and no owner.

This plan extends the existing Rust workspace (`mini-crypto`, `did-mini`,
`mini-bearer`, `mini-presence`, `mini-reward`, `mini-keystone`). Everything
below is Tier-O (organic) except where a [FREEZE] is noted.

---

## 1. Vision → SPEC-09 surface mapping

| "Like…" | Mininet surface (SPEC-09 §6) | Beta scope |
|---|---|---|
| Twitter / Insta / Snap | 6.1 Microblog + media feed | B1 |
| TikTok / YouTube | 6.1 short-video + creator objects | B3 (progressive) |
| Reddit | 6.2 Forums / communities (CRDT threads) | B2 |
| GitHub | 6.4 Code forge + release registry | B4 |
| "Network of networks" | One object model + read-bridges | B5 |
| WhatsApp/Signal | 6.3 E2E messaging | post-beta (needs pairwise channel work) |
| Marketplace / hosting / search | 6.5–6.7 | post-beta |

**The one rule that makes it "a network of networks":** every surface reads and
writes the same signed, content-addressed object (SPEC-09 §2). A community
thread can embed a commit; a feed post can link a thread; the forge PR is just
an object with governance semantics. No per-surface format, ever. [FREEZE]

---

## 2. Technology decisions (D-0020 — sovereignty-first, founder-directed)

**Founder directive:** sovereignty — nobody able to forbid, censor, block, or
kill it — outranks speed, cost, and polish. Every choice below follows.

**Kill-vector analysis (what could stop the app, and the answer)**

| Kill vector | Mitigation (canonical path) |
|---|---|
| App-store removal | **The network is the store**: binaries are content-addressed release objects synced peer-to-peer (D-0011/D-0012). Android sideload APK is canonical; stores are optional mirrors. |
| iOS platform control | Honest weak point: Apple gates native code. Ship SwiftUI shell best-effort (store / EU third-party stores / AltStore), plus PWA fallback. iOS users get the network; Apple can degrade, not kill it. |
| DNS / domain / CDN blocks | No canonical domain. Web viewer is a convenience mirror anyone can host; the object store + relays are the real distribution. |
| Push-notification chokepoints | None used — notifications derive from local sync. No Google Play Services dependency, ever. |
| Relay takedowns | Relays are dumb, ciphertext-only, self-hostable by anyone; BLE/Wi-Fi mesh works with zero relays. |
| UI-framework owner (Google/Meta) | No Flutter/React Native. UI stack below is MIT-licensed Rust or thin, rewritable native shells. |
| Toolchain supplier | Rust toolchain pinned + vendored source in-tree; deps vendored (`cargo vendor`); reproducible builds are a frozen requirement and drive every choice. |

**Core & bindings**
- **One Rust core** (existing workspace) holds all logic; UI layers are thin
  renderers, small enough to rewrite in a weekend. [FREEZE for the beta]
- **UniFFI** for the thin mobile shells; **wasm-bindgen** for web.

**UI, per platform (sovereignty order)**
- **Desktop (Linux/Windows/macOS) — first-class:** Rust-native UI via **egui**
  (MIT, pure Rust, vendorable — zero non-Rust UI toolchain), fully reproducible,
  full self-update. This is the sovereign reference client.
- **Android — first-class:** thin **Kotlin/Jetpack Compose shell** over the
  UniFFI core (Android's build chain is unavoidable for BLE/keystore anyway;
  the shell contains zero logic). Distributed as a **sideloaded APK through
  Mininet itself** + F-Droid; Play is a mirror.
- **Web — mirror surface:** the same egui core compiled to **WASM** (egui runs
  in-browser) + relay bearer; hostable by anyone, no canonical domain.
- **iOS — best-effort surface:** thin SwiftUI shell; store + EU third-party
  distribution; PWA fallback. Documented honestly as the least sovereign
  platform by Apple's design, not ours.

*Cost accepted:* two thin native mobile shells + one Rust UI ≈ more work than
one Flutter codebase, and egui's look is functional rather than platform-native.
That is the price of no framework owner; the founder directive pays it.
*(Alternatives — Flutter, React Native, native-everything, Dioxus/Slint — and
why each lost are recorded in D-0019/D-0020.)*

**Storage & data**
- **Content-addressed blob store** (BLAKE3 multihash, already in `mini-crypto`)
  for all payloads and media chunks.
- **SQLite** (rusqlite, vendored source) for indexes/heads/queues; OPFS on web.
- **Own minimal signed op-log CRDT** (SPEC-09 §3) — owned because merge
  semantics encode one-human authorship.

**Media**
- Chunked content-addressed blobs (1 MiB, Merkle manifest), progressive fetch,
  platform decoders. Nearby-first; relays accelerate; no CDN promises.

**Sync**
- `mini-sync`: store-and-forward replication over any `Bearer` (BLE, local
  Wi-Fi, relay), want/have + Merkle manifests, resumable — and it is the
  **distribution channel for the app itself**.

**Self-update**
- Releases are governance objects (D-0011): finality + timelock + artifact hash
  + ≥N independent reproducible-build attestations. **No forced update, no
  remote kill path** [FREEZE]. Desktop + Android sideload get full binary
  self-update through the network; iOS gets content/code-object updates in-app.

## 3. New crates & repo layout

```
crates/
  mini-objects    # SPEC-09 envelope: signed, typed, content-addressed; heads; CRDT ops
  mini-store      # blob store + SQLite indexes + head pointers + queues
  mini-sync       # want/have replication over Bearer; Merkle manifests; resume
  mini-social     # graph (follow), feed assembly, reach layer (speech-vs-reach)
  mini-forum      # communities, CRDT threads, member governance hooks
  mini-media      # chunking, manifests, progressive assembly
  mini-forge      # repo objects, PR/review objects, release registry client, update verifier
  mini-ffi        # UniFFI interface + wasm-bindgen exports (one API surface)
app/
  desktop/        # egui Rust client (Linux/Win/macOS) — the sovereign reference
  android/        # thin Kotlin/Compose shell over UniFFI (sideload-canonical)
  ios/            # thin SwiftUI shell (best-effort surface)
  web/            # egui-WASM mirror client (relay bearer)
```

---

## 4. Team tracks (parallel from day 1)

- **T-CORE** (2 devs): mini-objects, mini-store, CRDT, mini-ffi
- **T-SYNC** (1–2 devs): mini-sync, bearer adapters (BLE/Wi-Fi/relay)
- **T-APP** (2–3 devs): egui desktop client + Android Compose shell; onboarding, feed, forum, media UI (iOS shell staffed when available — never on the critical path)
- **T-FORGE** (1–2 devs): mini-forge + forge portal UI + update verifier
- **T-WEB** (1 dev): WASM build, PWA, relay client
- **T-QA/DX** (1 dev): CI, golden vectors, reproducible builds, device farm

Cadence: **2-week sprints**, 12 sprints to UI beta with the surfaces below —
honestly, the sovereignty stack adds ~2 sprints of UI cost vs a single-codebase
framework; the calendar absorbs it by making iOS non-critical-path and keeping
egui styling functional rather than polished. Task IDs:
`E<epic>.S<sprint>.T<n>`. Every task lists acceptance criteria (AC).

---

## 5. Epics

### E1 — Core bindings & app shell (T-CORE + T-APP) · Sprints 1–3
Make the existing Rust core callable from every surface.

- **E1.S1.T1** Define the `mini-ffi` API v0 (UDL): identity (incept, delegate,
  KEL export), channel (open/seal/open), presence (build/verify), reward
  (accrue). AC: UDL compiles; Kotlin+Swift bindings generate in CI.
- **E1.S1.T2** Shells: egui desktop app skeleton (pure Rust) + Android
  Kotlin/Compose skeleton over UniFFI, toolchains pinned/vendored in-tree.
  AC: "hello core" round-trip on Android emulator + Linux desktop in CI.
- **E1.S1.T3** Threading model: core behind a single command/event queue
  (no shared mutable state across FFI). AC: stress test 10k calls, no UB flags.
- **E1.S2.T1** Onboarding UX: create human root → delegate this device
  (primary) → seed backup ceremony (P6: user-held, never uploaded). AC: fresh
  install to usable identity < 2 min; secrets never cross FFI (audit).
- **E1.S2.T2** Device manager screen: list/delegate/revoke devices, capability
  display (VOTE/ATTEST badges). AC: revoke on device A reflects on B after sync.
- **E1.S3.T1** Keystone demo screen: two phones, airplane mode, run the
  `mini-keystone` flow with progress UI + accrued-points display. AC: live
  demo on 2 physical devices over BLE.
- **E1.S3.T2** WASM build of the egui client (identity+objects only).
  AC: web "hello identity" renders in CI headless browser via relay.
- **E1.S3.T3** Reproducible-build proof: two independent builders produce
  bit-identical Linux + APK artifacts from the pinned recipe. AC: hashes match
  in CI — this gate blocks all later sprints (frozen SPEC-11 requirement).

### E2 — Object model & local store (T-CORE) · Sprints 1–4
The SPEC-09 envelope everything else rides on.

- **E2.S1.T1** *(shipped)* `mini-objects`: envelope (type, author DID, links, payload hash,
  signature, timestamp), canonical encoding (reuse hardened codec patterns),
  verify(). AC: golden vectors; tamper tests; cross-checked against SPEC-09 §2.
- **E2.S2.T1** *(shipped)* Head pointers (mutable single-author state): signed head record
  (profile, post-edit). AC: edit publishes new object + head moves; old
  versions remain fetchable.
- **E2.S2.T2** *(shipped — fs+memory backends; SQLite at integration)* `mini-store`: blob store (BLAKE3-addressed files) + SQLite
  schema (objects, heads, authors, links, queues) + migrations. AC: 100k-object
  synthetic corpus; cold-start index < 2 s on mid phone.
- **E2.S3.T1** *(shipped)* Op-log CRDT: signed ops, per-author sequencing, deterministic
  merge, snapshot/compaction. AC: property tests — any op order converges;
  forged/duplicate ops rejected.
- **E2.S4.T1** Encrypted objects (private/audience payloads) using
  `mini-crypto` AEAD; key wrapping for audiences deferred to messaging epic.
  AC: private post unreadable from raw store; round-trips for owner.

### E3 — Sync & replication (T-SYNC) · Sprints 2–6
- **E3.S2.T1** *(shipped)* `mini-sync` protocol v0: have/want exchange over an established
  channel; Merkle manifest for sets; chunked transfer with resume. AC: two
  in-process peers reconcile 10k objects; kill/resume mid-transfer converges.
- **E3.S3.T1** **BLE bearer adapter** (Android first, then iOS): implements the
  existing `Bearer` trait (GATT, MTU framing, background limits documented).
  AC: keystone demo runs over real BLE; throughput baseline recorded.
- **E3.S4.T1** Local Wi-Fi / hotspot bearer (mDNS discovery + TCP framing).
  AC: two phones on one hotspot sync a 50 MB media object < 60 s.
- **E3.S5.T1** Optional relay bearer (self-hostable, dumb byte relay; carries
  only ciphertext; anyone can run one). AC: web/PWA syncs via relay; relay
  learns nothing but sizes/timing (documented honestly).
- **E3.S6.T1** Store-and-forward queues + periodic "refresh & submit" scheduler
  (A3 model: opportunistic, not always-on). AC: airplane-mode authored posts
  propagate on next encounter; battery budget measured.

### E4 — Social graph & profiles (T-APP + T-CORE) · Sprints 3–5
- **E4.S3.T1** *(shipped)* Profile objects (display name, avatar blob, bio) + head-based
  edit. AC: profile edit propagates; impersonation impossible (DID-bound).
- **E4.S3.T2** *(shipped)* Follow objects + `mini-social` graph index. AC: follow/unfollow
  offline; graph queries < 50 ms at 10k edges.
- **E4.S4.T1** Humanness badge: surfaces the personhood signal (presence-backed)
  on profiles — **display only**, never gates speech (P1/P2; rating ≠ conduct).
  AC: badge states rendered from verdict store; no reach coupling.
- **E4.S5.T1** Contact exchange in person: QR + BLE tap using presence flow
  ("met in real life" edge, consent both sides). AC: two-phone demo produces a
  mutual follow + presence verdict.

### E5 — Feed & posting (T-APP) · Sprints 4–7
- **E5.S4.T1** Composer: text + images (chunked blobs), drafts offline. AC:
  post authored in airplane mode appears locally instantly, syncs later.
- **E5.S5.T1** *(core shipped in mini-social; UI pending)* Feed assembly in core (`mini-social`): follows-first,
  chronological default; **speech-vs-reach** layer per SPEC-09 §5 — reach
  ranking is a *client-side, user-chosen filter*, never a hidden server
  algorithm [FREEZE]. AC: feed of 5k posts renders 60 fps; filter switchable.
- **E5.S6.T1** Reactions/support objects (SPEC-05 §9 social mechanics stub):
  support adds to storage budget signal; dislike soft-nudges reach only. AC:
  every post retains reach floor (invariant test).
- **E5.S7.T1** Notifications (local, from sync events). AC: mention/reply
  surfaces within one sync cycle; zero push-server dependency.

### E6 — Communities / forums (T-APP + T-CORE) · Sprints 5–8
- **E6.S5.T1** Community objects (name, charter, membership mode) + join/leave.
  AC: create community offline; membership syncs.
- **E6.S6.T1** CRDT threads: topics, nested comments, edits. AC: two devices
  comment concurrently offline; merge converges identically on both.
- **E6.S7.T1** Per-community governance menu (SPEC-08 hooks): M-of-N moderators
  *or* one-human-one-vote toggle; moderation = **filters/labels, not deletion**
  (SPEC-10 labeler pattern; author copy persists — P6). AC: mod label hides in
  default filter but content remains fetchable by choice.
- **E6.S8.T1** Community discovery index (local + gossip of community cards).
  AC: nearby/known communities listed without any directory server.

### E7 — Media & creator space (T-APP + T-CORE) · Sprints 6–9
- **E7.S6.T1** *(shipped)* `mini-media`: chunker (1 MiB), Merkle manifest, progressive
  assembler, integrity per chunk. AC: 200 MB file survives 3 interrupted syncs.
- **E7.S7.T1** Short-video capture/compress/publish (platform encoders);
  vertical feed player with prefetch-from-nearby. AC: record→publish→playback
  on second device via hotspot sync only.
- **E7.S8.T1** Creator page (channel = profile + pinned collections). AC:
  collection object renders across feed and forum embeds identically.
- **E7.S9.T1** Support-the-creator action wired to reward/social mechanics
  stub (display-only until chain). AC: support event appears in both ledgers.

### E8 — Forge portal: the self-updating "GitHub" (T-FORGE) · Sprints 5–10
- **E8.S5.T1** *(core shipped: blob/tree/commit/branches; git interop later)* `mini-forge` repo objects: commits/trees/blobs as content-
  addressed objects (SHA-256 git-interop mapping per SPEC-11), clone-from-
  object-store. AC: round-trip a real git repo ↔ object store bit-exact.
- **E8.S6.T1** *(shipped)* PR objects: branch head + diff manifest + discussion thread
  (reuses E6 CRDT). AC: open PR offline, review comments merge like forum.
- **E8.S7.T1** *(shipped)* Review/approval objects with signer identity; merge = signed
  head move recorded as governance record (money can never buy merge —
  [FREEZE], SPEC-11). AC: merge requires policy quorum; invariant test.
- **E8.S8.T1** *(core shipped: release + attest + verify_release_artifact_only; adoption gate = verify_governed_release)* Release registry client: release object = artifact hashes +
  build recipe + attestations; verifier checks finality*, timelock, ≥N
  attestations (*chain stub until mini-chain: signed-quorum placeholder,
  clearly labeled provisional). AC: tampered artifact rejected; missing
  attestation rejected.
- **E8.S9.T1** In-app update flow: fetch release objects via sync, verify,
  stage; desktop performs swap; mobile stages content/code-objects and
  deep-links store/sideload for binary. **No forced update path exists** —
  invariant test greps release code for any remote-trigger execution. AC:
  end-to-end self-update on desktop from a nearby peer, no internet.
- **E8.S10.T1** Forge portal UI: repo browser, PR list/diff/review, release
  board, "propose change to Mininet" wizard. AC: a doc-only PR to this very
  repo can be authored, reviewed, merged entirely in-app.

### E9 — Safety & filters (T-CORE + T-APP) · Sprints 7–9
Client-side, P5-respecting; no central takedown, no unmasking.

- **E9.S7.T1** Filter/label object type + subscribable filter lists (community
  labelers). AC: subscribing hides labeled content; unsubscribe restores.
- **E9.S8.T1** Personal blocklists + keyword mutes, local only. AC: block
  propagates across own devices only.
- **E9.S9.T1** Reach-floor invariant tests + "why am I seeing/not seeing this"
  inspector (every ranking decision explainable). AC: inspector shows the
  filter chain for any hidden post.

### E10 — Web & desktop surfaces (T-WEB) · Sprints 6–10
- **E10.S6.T1** egui-WASM build (objects/store on OPFS, relay bearer). AC: web
  mirror reads a feed synced via relay; any host can serve it (no canonical domain).
- **E10.S8.T1** Desktop packaging (egui) + APK sideload pipeline distributed as
  release objects through mini-sync. AC: fresh device installs the app from a
  nearby peer with no internet and verifies its release object.
- **E10.S10.T1** Read-only public web viewer for objects (share links render
  without install). AC: any object ID renders in PWA from relay.

### E11 — QA / DevX / reproducibility (T-QA) · continuous
- **E11.S1.T1** CI matrix: Rust tests, egui desktop build, Android shell build, WASM
  build, golden-vector suite. AC: red on any drift.
- **E11.S2.T1** Cross-device sync test rig (emulated bearers, packet loss,
  reorder). AC: nightly chaos run green.
- **E11.S4.T1** Reproducible-build attestation tooling (builders sign artifact
  hashes → attestation objects for E8). AC: 2 independent attestations in CI.
- **E11.S6.T1** On-device perf budgets (cold start < 3 s, feed 60 fps, sync
  battery < 3%/day idle). AC: dashboards + regression gates.

### E12 — Beta assembly & field test · Sprints 10–12
- **E12.S10.T1** Feature-freeze integration: feed + forum + media + forge on
  one build. AC: all epic ACs green on one binary.
- **E12.S11.T1** 50-person field pilot (one city): onboarding, in-person
  contact exchange, community + creator use, one real forge PR merged in-app.
  AC: pilot metrics captured locally, shared voluntarily (P6).
- **E12.S12.T1** Beta release object published through the forge itself; docs;
  the "first PR" goes out with the beta, not before. AC: release verifiable by
  a fresh install from a nearby peer.

---

## 6. Sprint calendar (who does what, in parallel)

| Sprint | T-CORE | T-SYNC | T-APP | T-FORGE | T-WEB | T-QA |
|---|---|---|---|---|---|---|
| 1 | E1.S1, E2.S1 | protocol design | E1.S1.T2 shell | forge design doc | — | E11.S1 |
| 2 | E2.S2 | E3.S2 | E1.S2 onboarding | object mapping spike | — | E11.S2 |
| 3 | E2.S3 CRDT | E3.S3 BLE | E1.S3 keystone UI, E4.S3 | — | WASM spike | rig |
| 4 | E2.S4 | E3.S4 Wi-Fi | E5.S4 composer | — | — | E11.S4 |
| 5 | E4/E6 core | E3.S5 relay | E6.S5 communities | E8.S5 repos | — | — |
| 6 | E7 media core | E3.S6 DTN | E5/E7 UI | E8.S6 PRs | E10.S6 PWA | E11.S6 |
| 7 | E9.S7 filters | hardening | E5.S7, E7.S7 | E8.S7 merges | — | chaos |
| 8 | — | perf | E6.S8, E7.S8 | E8.S8 registry | E10.S8 desktop | attest |
| 9 | E9.S9 | — | E7.S9, E9 UI | E8.S9 update | — | perf gates |
| 10 | integr. | integr. | integr. | E8.S10 portal UI | E10.S10 viewer | E12.S10 |
| 11 | ——— all hands: E12.S11 field pilot ——— | | | | | |
| 12 | ——— all hands: E12.S12 beta release via forge ——— | | | | | |

**Dependency spine:** E2 → (E3, E4) → E5/E6 → E7/E8 UI; E8.S8–S9 needs E3
relay + E11 attestations. The keystone demo (E1.S3 + E3.S3) is the sprint-3
public proof point.

---

## 7. Honest boundaries (stated in-app, not hidden)

1. Web has no BLE: browser surface joins via relay only.
2. iOS is the least sovereign platform by Apple's design: best-effort store /
   EU-third-party / PWA. Android sideload + desktop carry the full promise.
3. Media at TikTok scale needs relays/mesh density; beta promises nearby-first,
   relay-accelerated — not a CDN.
4. Search is federated and partial by design (SPEC-09 §6.7) — post-beta.
5. Until `mini-chain` lands, forge finality uses a labeled provisional
   signed-quorum; the chain replaces it without changing object formats.

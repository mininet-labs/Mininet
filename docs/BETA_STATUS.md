# Beta status

**Beta target:** the SPEC-03 keystone — two phones form an encrypted Mininet link
with no internet, exchange verified identities, prove range-bound co-presence, and
show local reward accrual. We do **not** publish until the beta is complete.

This is a narrower, nearer-term target than "global launch" — see the root
`README.md`'s [Path to a global launch](../README.md#path-to-a-global-launch-what-is-still-missing)
section for the full-network picture and `docs/STATUS.md` for the
comprehensive, living implementation-status account organized by domain
(voice/value, personhood, identity, money/finality, updates/forks,
privacy, storage, networking, AI/audit gates). The full 22-crate map lives
in the root README's repository-map table, not duplicated here.

## What stands between here and a demoable beta (honest list)

The identity/presence/reward/forge logic layers this beta needs are complete
and pass `cargo test --all --all-features` on a real toolchain today (see
[Build & test](#build--test) below — `Cargo.lock` is committed). What's
still missing for a real two-phone beta, in order:

1. **Bearer adapters** — BLE and local-Wi-Fi/hotspot behind the existing
   `Bearer` trait (device-side work needing real phone hardware). D-0042
   added a real `TcpBearer` (proven live in `mini-net`'s gossip demo), but
   that's IP-network connectivity, not BLE — the keystone demo itself is
   still in-process only and hasn't been ported to it yet. `mini-bearer::
   ble` (D-0342) has the MTU-bounded chunking/reassembly protocol logic a
   BLE-backed `Bearer` needs; `mini-bearer::android_ble` (D-0374) adds the
   `BleRadio` trait and `AndroidBleBearer`, a full, tested `impl Bearer`
   generic over any radio implementation — but `BleRadio` is a plain Rust
   trait, not yet a UniFFI callback interface, and no Kotlin
   `BluetoothGattServer`/`BluetoothGattCallback` implementation exists
   yet. This item is not closed by D-0370 or D-0374 — D-0370 is adjacent
   app-persistence work, and D-0374 only narrows the remaining gap to the
   UniFFI wiring, the real Kotlin GATT implementation, and a real
   two-device test, none of which are code-only.
2. ~~**Active range measurement**~~ — **shipped (D-0368)**:
   `mini_presence::active_range` performs a real challenge-response
   round-trip exchange over the already-bound encrypted channel
   (`send_range_challenge`/`respond_to_range_challenge`/
   `recv_range_response`); `mini-keystone::run_demo` now feeds
   `AttestationFields::rtt_samples_ms` with genuinely measured elapsed
   times instead of a hand-written literal. Still application-layer timing,
   not a formal distance-bounding protocol or hardware ranging — see that
   module's own "Honest limits" section — but the specific gap this item
   named (a claimed proximity number nobody else could check) is closed.
3. ~~**Persistent replay store**~~ — **shipped (D-0366, wired D-0367)**:
   `mini_presence::FileReplayGuard` is a file-backed `ReplayGuard` that
   survives process restarts (`Cargo.toml` unchanged — `std::fs` only, no
   new dependency). `mini-keystone::run_demo` now takes each side's
   `ReplayGuard` from the caller instead of constructing a throwaway
   `InMemoryReplayGuard` internally, so a real app can pass a
   `FileReplayGuard` opened at a persistent path and actually get
   cross-restart replay protection; the crate's own example does exactly
   that. `mini-uniqueness`/`mini-storage`/`mini-settlement` each define
   their own separate `ReplayGuard`-shaped trait and still only have an
   in-memory implementation; giving those a durable backend too is
   unstarted, separately-scoped work.
4. ~~**Standalone CLI harness**~~ — **shipped (D-0369)**: `mini keystone run
   --peer-home <path>` is a real `mini` subcommand driving identity →
   channel → range-bound presence → reward end to end (previously only
   reachable via `cargo run -p mini-keystone --example keystone`, not the
   actual binary); `mini repo`/`pr`/`build`/`release`/`installer` already
   covered forge PR → merge → release → verify as their own subcommands.
   `tools/no_github_outage_demo.sh` (D-0081) now runs the whole named
   chain — identity → channel → presence → reward → repo → commit → PR →
   review → governed merge → release → attestation → verify → install →
   health check → rollback → tamper-evident event log — as one script
   driving nothing but the real compiled `mini` binary.
5. **Android two-phone product path** — the signed LAN/QR social pairing
   path is implemented through Rust/UniFFI/Compose (D-0373): expiring QR,
   delegated-device verification, bounded TCP acceptance, durable replay
   rejection, and a signed follow object on each phone. It is not marked
   complete until Android CI assembles it and two physical devices prove
   scan, connect, follow, restart, and replay rejection. This is social
   pairing, not yet the encrypted keystone bearer/range/reward path.
6. **External crypto review** before any value- or update-bearing use.
7. **Personhood (SPEC-02)** — quorums today count *distinct verified identity
   roots, not humans*; "one human, one vote" is not yet enforced. D-0038
   redesigned personhood into an open-ended multi-signal system
   (`mini-uniqueness::status`), but the underlying behavioral/location ZK
   research problem (signal (b)) remains unsolved — see the root README.
8. **KEL freshness / revocation anchoring** — verifiers check the KEL handed to
   them, not that it is the latest globally known state; high-value actions need
   witness receipts / chain anchoring later.

## Before trusting any of this

```sh
cargo fmt --all
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo test --all --all-features
```

All three are clean on this tree today. The composed crypto (Pack 1
primitives + the `mini-bearer` channel, and every AI-authored prototype
under D-0036/D-0037/D-0040/D-0041) additionally warrants a proper
cryptographic review before the beta — or anything past it — ships:
"compiles, tests pass, and round-trips" is not "audited."

## UI beta (the product layer)

The full UI plan — surfaces, technologies, epics, 12 sprints, per-team tasks —
lives in `docs/UI_BETA_PLAN.md` (D-0019). Parallel tracks can start immediately;
the sprint-3 public proof point is the two-phone keystone demo with UI over real
BLE.

## Post-beta (not on the critical path)

Self-contained BLE bootstrap + Merkle chunk sync (`mini-bootstrap`), local release
verifier (`mini-update`), the custom Rust BFT chain + release registry
(`mini-chain`), ZK personhood (SPEC-02), and the self-hosted forge (SPEC-11). See
`docs/ROADMAP.md` for the full ordered plan.

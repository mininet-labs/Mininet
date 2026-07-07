# Beta status

**Beta target:** the SPEC-03 keystone — two phones form an encrypted Mininet link
with no internet, exchange verified identities, prove range-bound co-presence, and
show local reward accrual. We do **not** publish until the beta is complete.

## Crate status

| Crate | Purpose | Status |
|---|---|---|
| `mini-crypto` | Signatures, X25519, ChaCha20-Poly1305, HKDF, strong multihash, multibase | logic complete + hardened; needs local compile/clippy/test |
| `did-mini` | KERI identity: KEL, pre-rotation, device delegation, detached signing | logic complete + hardened |
| `mini-bearer` | Identity-agnostic transport + anonymous forward-secret channel | logic complete (in-process); BLE/Wi-Fi adapter pending |
| `mini-presence` | Mutually-signed co-presence envelope with reported RTT samples | alpha logic; active challenge-response range evidence pending |
| `mini-reward` | Deterministic, non-spendable presence-conditioned accrual by identity root | alpha logic; not money and not personhood |
| `mini-keystone` | In-process composition harness (`run_demo`) + example | alpha logic; physical bearer and active range pending |
| `mini-objects` | SPEC-09 unified signed content-addressed envelope | logic complete |
| `mini-store` | Blob store, indexes, signed head pointers, want-lists | logic complete |
| `mini-crdt` | Op-log CRDT for threads/docs (one-human authorship) | logic complete |
| `mini-sync` | Bucketed reconciliation + verified ingest + KEL carriers | logic complete |
| `mini-social` | Profiles, follow graph, explainable locally-computed feeds | logic complete |
| `mini-media` | Chunked content-addressed media, progressive assembly | logic complete |
| `mini-forge` | Repos, branches, releases + attestations, PRs, commit-bound reviews, self-amending merge governance | logic complete (git SHA-256 interop + update staging pending) |
| BLE / local-Wi-Fi adapter | Real physical bearer behind the `Bearer` trait | pending; also needs active range measurement and local compile/test |

## What stands between here and a demoable beta (honest list)

This tree is an **architecture alpha**: the logic layers are complete and
statically cross-checked, but nothing has been compiled — the authoring sandbox
has no Rust toolchain. In order:

> **Batch 7B-final (truth-sync, D-0031) is done in this tree.** Two latent
> compile errors were fixed (a stale `verify_release` test import; a by-value
> `verify_signature(kel)` that must borrow `&kel`); residual `human`-quorum
> wording was scrubbed to *identity root* everywhere current code was described;
> the KEL **root-carrier** envelope hole was closed (absorb the self-certifying
> KEL, but index the object only once its signing device is known and full
> provenance holds — else it stays transport-only); and the offline
> `tools/mininet_nav.py` navigator + generated map were (re)added. The one thing
> that still cannot be produced here is a real `Cargo.lock` — it needs a network
> + `cargo`. Everything below still requires a real environment.

1. **Toolchain pass** — `cargo fmt / clippy -D warnings / test --all /
   generate-lockfile` in a real environment, then commit `Cargo.lock`.
2. **Bearer adapters** — BLE and local-Wi-Fi/hotspot behind the existing
   `Bearer` trait (device-side work; only the in-process bearer exists today).
3. **Active range measurement** — the current RTT ceiling is a software
   thresholding hook over *reported* samples, not an active challenge-response
   measurement; no anti-relay claim is made until it is.
4. **Persistent replay store** — `ReplayGuard` is the durable interface;
   a device-store-backed implementation is pending (max-age already bounds the
   window).
5. **Standalone CLI harness** — one command driving identity → channel →
   presence → reward → forge PR → merge → release → verify.
6. **External crypto review** before any value- or update-bearing use.
7. **Personhood (SPEC-02)** — quorums today count *distinct verified identity
   roots, not humans*; "one human, one vote" is not yet enforced. The
   `IdentityOracle` seam is where a `PersonhoodOracle` will slot in.
8. **KEL freshness / revocation anchoring** — verifiers check the KEL handed to
   them, not that it is the latest globally known state; high-value actions need
   witness receipts / chain anchoring later.

## Before trusting any of this

Nothing here has been compiled in this environment (no `cargo`/`rustc`). Each pack
is written to be mergeable and was statically cross-checked, but the first real
Rust environment must run, from the workspace root:

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo generate-lockfile   # then commit Cargo.lock
```

The composed crypto (Pack 1 primitives + the `mini-bearer` channel) additionally
warrants a proper cryptographic review before the beta ships — "compiles and
round-trips" is not "audited."

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

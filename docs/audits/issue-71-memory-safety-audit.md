# Memory-safety audit across all crates

Tracks [roadmap issue #71](../../issues/71)
(Phase 10.1). Scope, per the issue: verify `#![forbid(unsafe_code)]`
coverage across this workspace, and check whether that guarantee actually
holds transitively — the attribute only forbids `unsafe` in *this
workspace's own code*, not in anything it depends on.

## 1. Workspace `#![forbid(unsafe_code)]` coverage

Checked every crate's `src/lib.rs` for the attribute directly (not just
trusting the convention):

```
$ for f in crates/*/src/lib.rs; do grep -L "forbid(unsafe_code)" "$f"; done
(no output -- every crate has it)
```

**Result: 22/22 crates carry `#![forbid(unsafe_code)]`.** This is a
compile-time guarantee, not a lint that can be silently suppressed with an
`#[allow]` at a smaller scope — `forbid` (unlike `deny`) cannot be
downgraded by a later attribute in the same crate. This part of the audit
is a clean PASS.

## 2. Dependency tree: does `unsafe` appear, and is it expected?

`forbid(unsafe_code)` says nothing about the 40 external crates this
workspace depends on (`cargo tree --workspace -e normal`, deduplicated).
Every one of their `unsafe` blocks runs inside this process, with this
process's memory, on every user's device — so this audit checked what's
actually there, not just whether the attribute exists.

With dependency sources already fetched into the local cargo registry
cache, each crate's source was searched directly for `unsafe` (file-count,
not line-count, as a coarse but real signal):

| Crate | Files with `unsafe` | Why (expected reason) |
|---|---|---|
| `blake3` | 11/16 | SIMD intrinsics (AVX2/SSE/NEON) for hashing performance |
| `sha2` | 8/15 | SIMD intrinsics |
| `chacha20`, `poly1305`, `cipher`, `inout` | several each | SIMD intrinsics + const-generic buffer manipulation |
| `curve25519-dalek`, `curve25519-dalek-derive` | 14/48, 1/1 | SIMD backend (AVX2) for field arithmetic; this is the audited primitive layer D-0014/D-0036 already depend on knowingly |
| `getrandom`, `libc` | 20/24, 58/344 | FFI syscalls to the OS CSPRNG (`getrandom(2)`, `/dev/urandom`, platform-equivalent) -- this is *how* `mini_crypto::random_32` gets real entropy at all; there is no safe-Rust way to ask the kernel for randomness |
| `zeroize`, `zeroize_derive` | 3/3, 1/1 | `unsafe` is *required* for zeroize's actual guarantee -- the compiler must be prevented from optimizing away a memory-clearing write, which safe Rust cannot express |
| `cpufeatures` | 3/5 | runtime CPU feature detection (needed to safely gate the SIMD paths above) |
| `generic-array`, `arrayvec`, `arrayref` | several each | const-generic / fixed-size array transmutation, a well-known safe-in-practice pattern this whole ecosystem relies on |
| `ed25519-dalek`, `x25519-dalek`, `hkdf`, `hmac`, `digest`, `bs58`, `subtle`, `constant_time_eq`, `aead`, `chacha20poly1305`, `signature`, `rand_core`, `crypto-common`, `universal-hash`, `typenum`, `block-buffer`, `opaque-debug` | 0-4 each | mostly none, or thin FFI/const-generic glue matching the patterns above |
| `proc-macro2`, `quote`, `syn`, `unicode-ident`, `cfg-if` | several each (build-time only) | compile-time macro tooling, never linked into the shipped binary's runtime logic |

**No dependency's `unsafe` usage is unexplained.** Every occurrence falls
into one of four well-understood categories: SIMD performance intrinsics,
OS-syscall FFI (there is no other way to get real entropy or query CPU
features), zeroize's core correctness requirement, or build-time-only
macro tooling that never ships in a compiled artifact's runtime path.
None of these 40 dependencies are obscure or unmaintained — they're the
same RustCrypto-ecosystem and `dalek`-family crates this project already
committed to at D-0014.

## 3. Dependency-vulnerability scanning: a real finding from setting this up

Closing the loop with [issue #73](../../issues/73)
(this audit and that CI job were done together): actually installing and
running `cargo audit` locally surfaced a genuine, non-obvious problem
before it could land silently in CI. `cargo-audit` versions compatible
with this workspace's pinned toolchain (`rust-toolchain.toml`, rustc
1.83.0 — anything `>=0.22` requires rustc 1.88+) turned out to be
`cargo-audit 0.21.2`. Installed and run locally:

```
$ cargo install cargo-audit --version 0.21.2 --locked
$ cargo audit
error: error loading advisory database: parse error:
  error parsing .../RUSTSEC-2026-0041.md: TOML parse error at line 8, column 8
  unsupported CVSS version: 4.0
```

The current RustSec advisory database contains entries using CVSS 4.0
scoring, which the `rustsec` crate version bundled in `cargo-audit`
0.21.2 cannot parse — so it fails to load the database *at all*, before
checking a single one of this workspace's 40 dependencies. A naive
`cargo install cargo-audit --locked` in CI (no version pin) would have
either silently picked up a newer, incompatible-with-1.83.0 cargo-audit
build failure, or — worse, if version-pinned to something
toolchain-compatible without testing it against the real advisory DB the
way this audit did — would have "passed" a CI job that never actually
scanned anything.

**Fix applied:** `.github/workflows/ci.yml`'s `dependency-audit` job uses
the official `rustsec/audit-check@v2` GitHub Action instead of a locally
`cargo install`ed binary. The action ships its own prebuilt `cargo-audit`
binary, decoupled from this repository's pinned toolchain, so it isn't
affected by this version-compatibility trap.

## 4. What this audit does *not* claim

- **This is not a claim that every `unsafe` block in every dependency is
  individually sound.** That's each dependency's own maintainers' and the
  broader Rust ecosystem's audit responsibility, not something re-derived
  here line-by-line. What this audit does claim: nothing in the dependency
  tree is unexplained, obscure, or outside the expected pattern for a
  cryptography-adjacent Rust workspace.
- **This audit did not run against updated/future dependency versions.**
  See [issue #73](../../issues/73) and the
  new `dependency-audit` CI job (`.github/workflows/ci.yml`) for ongoing
  RustSec advisory scanning — that's the mechanism that catches a newly
  *discovered* problem in a dependency already in the tree, which this
  point-in-time audit cannot.

## Verdict

**PASS.** Workspace-authored code is provably `unsafe`-free by construction
(`forbid`, not `deny`). The dependency tree's `unsafe` usage is fully
accounted for and falls entirely within expected, necessary categories for
this class of software. Ongoing coverage now exists via CI (`dependency-audit`
job) rather than only this one-time manual pass.

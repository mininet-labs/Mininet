# mini-build-runner-wasmtime

The isolated build runner — self-hosted forge spine Batch 2b.2 (D-0069,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md)).

**This is the only crate in this tree permitted to depend on
`wasmtime`/`wasmtime-wasi`.** `mini-cli`, `mini-forge`, `mini-chain`,
identity, and every ordinary node binary must never gain a dependency
edge to it — a coordinator drives it by spawning the compiled binary as a
subprocess and speaking `mini-pipeline-protocol` over its stdin/stdout,
never by linking this crate directly.

## What it does

One process, one `ExecutionRequest`, one `ExecutionResult`: reads exactly
one framed request from stdin, executes it, writes exactly one framed
result to stdout, then exits. A fresh process per step — rather than a
long-lived loop serving many requests — is deliberate: it gives a
coordinator the cleanest possible cancellation story (kill the child) and
guarantees no state from one step's `wasmtime::Store` can ever leak into
another's.

```
mini-build-runner-wasmtime --store-dir <path> --scratch-dir <path> --artifacts-dir <path>
```

`--store-dir` is a content-addressed store: `component_digest` resolves
under `objects/<hex digest>`, `source_digest` resolves under
`workspaces/<hex digest>/`. Every byte read from it is re-hashed and
compared against the digest the coordinator claimed — a corrupt or lying
store can never reach the sandbox as if it were the signed input.

## Deny-by-default, precisely

Filesystem and network access are structurally denied: an undeclared
`workspace:read`/`scratch:write`/`artifacts:write` means no `preopened_dir`
call happens at all, so the guest's `wasi:filesystem` imports have
nothing to resolve a path against. An undeclared `network:host("...")`
means the socket address-check closure has an empty allow-list, so every
connection attempt is refused.

`wasi:clocks/monotonic-clock` and `wasi:random/random` are **not**
structurally removable this way — `wasmtime_wasi::p2::bindings::sync::
Command` binds the full `wasi:cli/command` world, which treats clocks and
randomness as ambient interfaces every command gets, not interfaces a
world can selectively omit without hand-authoring a narrower WIT world (a
real follow-up, not done here). `clock:monotonic` and
`random:deterministic` are enforced as declared *policy* today; for
`random:deterministic` specifically, the host RNG really is swapped for a
BLAKE3-keyed deterministic stream seeded from the request's own
`deterministic_seed`. A component declaring neither capability still has
a working, non-deterministic clock and RNG available. Stated here, not
glossed over.

## Resource limits

- **Fuel** (`wasmtime::Config::consume_fuel`) is the primary, deterministic
  CPU-limiting mechanism.
- **Epoch interruption**, driven by a parent-side watchdog thread that
  sleeps `max_wall_clock_ms` then increments the engine's epoch, is the
  *emergency* stop for cases fuel accounting doesn't catch — never the
  primary reproducibility mechanism.
- **`wasmtime::ResourceLimiter`** caps linear memory growth at
  `max_memory_bytes`. A refused grant doesn't always trap cleanly (the
  guest's own allocator may abort instead) — the runner reclassifies the
  outcome from the definitively observable post-run state (was the limiter
  ever refused a grant?) rather than trusting the guest's own crash
  message.
- **`MemoryOutputPipe`** bounds stdout/stderr to `max_stdout_bytes`/
  `max_stderr_bytes`, with the same reclassify-from-observed-state
  handling (a full pipe surfaces to a guest's language runtime as an
  ordinary write failure, which e.g. Rust's `println!` turns into a panic
  and abort, not a clean host-level trap).
- `max_output_bytes` (total bytes written under `artifacts:write`) is
  checked after the run — WASI's filesystem host functions have no live
  per-directory quota to enforce mid-write.

## Scope limitation: native tools are not sandboxed here

Wasmtime executes WebAssembly components. It is not represented as a
complete sandbox for arbitrary native toolchains — `cargo build`,
`npm install`, and similar remain `mini_pipeline::StepKind::NativeTool`
steps, unsandboxed at the process level, and
`PipelineStep::trusted_provenance_eligible()` returns `false` for them
unconditionally. That remains true until a separate, digest-pinned,
OS-isolated execution mechanism is designed and decided the same explicit
way D-0069 decided Wasmtime.

## Dependency governance

`wasmtime` and `wasmtime-wasi` are pinned together to `=48.0.7`.
The workspace compiler is pinned to Rust 1.95.0, matching upstream's minimum
supported compiler for that runtime. This upgrade addresses
RUSTSEC-2026-0314, -0316, -0321, -0322, -0323, -0324 and -0327 reported
against 46.0.3, plus RUSTSEC-2026-0352 through -0359 and -0361 through
-0364 affecting the earlier 48.0.4 candidate (including the WASI symlink
rename sandbox escape). The October 14 upstream 48.0.6 release contains
these fixes. Historical 27.x/46.x remediation is retained in the manifest
comments; an old patched baseline does not cover later advisories.

Never bump these pins on autopilot: resolve the lockfile with Cargo and run
the real adversarial suite below. The Wasmtime 48 filesystem API combines
directory/file permissions as `FsPerms`: workspace mounts remain
`ReadOnly`, scratch/artifact mounts remain `ReadWrite`, and an absent
capability still creates no mount. Network admission and the limitation on
ambient clocks/randomness remain unchanged.

Default features are disabled; the requested Wasmtime features remain
`cranelift`, `runtime` and `std`. Transitive dependencies can enable
additional features. Inspect the current graph with
`cargo tree -p mini-build-runner-wasmtime -e features` rather than treating
an earlier version's feature inventory as the current resolved graph.

Compiles untrusted Wasm bytes directly inside this isolated process (not
via a separate trusted precompiler) — Wasmtime's own documentation warns
that deserializing arbitrary precompiled modules assumes trusted input.

## Testing

```sh
cargo test -p mini-build-runner-wasmtime
```

The unit tests (`content_store`, `limiter`, `random`) exercise internal
plumbing directly. The integration suite (`tests/adversarial.rs`) drives
the *actual compiled binary* as a child process — real freshly-compiled
WASI Preview 2 components (`rustc --target wasm32-wasip2` emits a true
Component Model binary directly, no `wasm-tools componentize` step
needed), spoken to over the real framed stdin/stdout protocol — against
D-0069's twelve-point exit criteria. See that file's module docs for
exactly which criteria are proven directly, proven partially, or covered
by another crate's own tests.

**Environment requirement:** the adversarial suite needs the
`wasm32-wasip2` Rust target installed (`rustup target add wasm32-wasip2`)
to compile its guest fixtures at test time.

License: CC0-1.0 (public domain).

The October 22 patch moves both pins to48.0.7 for RUSTSEC-2026-0378
(component host-stream panic) and RUSTSEC-2026-0379 (aarch64 br_table
code generation). Runner execution results and environment digests report
this exact runtime version; the adversarial suite checks the reported value.

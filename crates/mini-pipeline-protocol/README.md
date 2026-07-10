# mini-pipeline-protocol

Content-addressed request/result messages for the coordinator ↔ isolated
runner IPC channel — self-hosted forge spine Batch 2b.1 (D-0069,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md)).

**No Wasmtime dependency.** This crate only defines the wire format: a
length-delimited, size-bounded framing (`[u32 big-endian length][payload]`,
refusing anything over a caller-supplied bound before allocating), and two
message types.

- `ExecutionRequest` — exactly what to run (`component_digest`,
  `source_digest`) and exactly what it may do (`capabilities`, `limits`,
  a `deterministic_seed` derived from the execution plan's own digest,
  never OS entropy). `digest()` binds a result back to the exact request
  that produced it, the same commit-binding discipline
  `mini_forge::approve` uses for reviewed commits.
- `ExecutionResult` — everything a `mini-provenance` record needs:
  component/source (via the request digest), the runner binary's own
  digest, the exact Wasmtime version, a runtime-config digest, the
  capabilities actually granted, output digests, exit status, fuel
  consumed, wall-clock elapsed, and stdout/stderr digests. Always carries
  `execution_security`, set by a real runner to
  `EXECUTION_SECURITY_WASMTIME_ISOLATED` — so a future weaker or
  unenforced executor could never silently reuse this type to claim
  isolation it didn't provide; an `"unenforced"` value round-trips as
  exactly that, never silently upgraded.

`mini-build-runner-wasmtime` is the only crate that actually produces an
`ExecutionResult` by running anything; this crate is pure data plus
framing, usable by both sides of the IPC channel (and by tests, and by a
future coordinator) without pulling in Wasmtime.

```sh
cargo test -p mini-pipeline-protocol
```

License: CC0-1.0 (public domain).

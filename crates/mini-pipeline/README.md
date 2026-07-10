# mini-pipeline

Pure pipeline manifest, policy, capability, and execution-plan types —
self-hosted forge spine Batch 2b.1 (D-0069,
[`docs/design/self-hosted-forge-spine.md`](../../docs/design/self-hosted-forge-spine.md)).

**This crate has no Wasmtime dependency, deliberately, permanently.** It
only describes what a pipeline step is allowed to do (`Capability`,
`ResourceLimits`) and how steps relate to each other
(`PipelineManifest`). Nothing in this crate executes anything. The only
crate in this tree allowed to link `wasmtime`/`wasmtime-wasi` is
`mini-build-runner-wasmtime` (Batch 2b.2) — `mini-cli`, `mini-forge`,
`mini-chain`, identity, and every ordinary node binary depend on this
crate (or nothing) for pipeline types, never on the runner.

## Deny-by-default, structurally

`Capability` has no "grant everything" variant: `workspace:read`,
`scratch:write`, `artifacts:write`, `clock:monotonic`,
`random:deterministic`, `network:host("...")`, `secret:read("...")` are
the entire vocabulary. A `StepKind::WasmComponent` step's `capabilities`
list is the *whole* set of host interfaces `mini-build-runner-wasmtime`
will construct a linker from — anything not listed is absent from the
guest's imports, not merely disabled by a runtime flag.

`StepKind::NativeTool` is the opposite case, named honestly: unsandboxed
host processes, and `PipelineStep::trusted_provenance_eligible` returns
`false` for them unconditionally — a structural fact the type system
enforces rather than a convention callers must remember. Native build
tools remain prohibited from trusted pipelines until a separate,
digest-pinned, OS-isolated execution mechanism is implemented and decided
the same explicit way D-0069 decided Wasmtime.

## What this crate does not claim

Validating a manifest here proves the *policy* is well-formed — names are
unique, `depends_on` resolves only to strictly earlier steps (no forward
references), resource limits are sane. It proves nothing about whether
any step's capabilities were actually enforced at runtime; that evidence
comes from `mini-build-runner-wasmtime`'s signed execution result
(`mini-pipeline-protocol`) and, ultimately, a `mini-provenance` record.

```sh
cargo test -p mini-pipeline
```

License: CC0-1.0 (public domain).

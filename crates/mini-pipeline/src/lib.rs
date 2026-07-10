//! Pure pipeline manifest, policy, capability, and execution-plan types —
//! self-hosted forge spine Batch 2b.1 (D-0069,
//! `docs/design/self-hosted-forge-spine.md`).
//!
//! **This crate has no Wasmtime dependency, deliberately, permanently.**
//! It only describes what a pipeline step is allowed to do
//! ([`Capability`], [`ResourceLimits`]) and how steps relate to each other
//! ([`PipelineManifest`]). Nothing in this crate executes anything. The
//! only crate in this tree allowed to link `wasmtime`/`wasmtime-wasi` is
//! `mini-build-runner-wasmtime` (Batch 2b.2) — `mini-cli`, `mini-forge`,
//! `mini-chain`, identity, and every ordinary node binary depend on this
//! crate (or nothing) for pipeline types, never on the runner.
//!
//! ## Deny-by-default, structurally
//!
//! [`Capability`] has no "grant everything" variant. A [`StepKind::
//! WasmComponent`] step's `capabilities` list is the *entire* set of host
//! interfaces `mini-build-runner-wasmtime` will construct a linker from —
//! anything not listed is absent from the guest's imports, not merely
//! disabled by a runtime flag. [`StepKind::NativeTool`] steps are the
//! opposite case, named honestly: unsandboxed host processes, and
//! [`PipelineStep::trusted_provenance_eligible`] returns `false` for them
//! unconditionally, a structural fact the type system enforces rather
//! than a convention callers must remember.
//!
//! ## What this crate does not claim
//!
//! Validating a manifest here proves the *policy* is well-formed — names
//! are unique, dependencies resolve to earlier steps, resource limits are
//! sane. It proves nothing about whether any step's capabilities were
//! actually enforced at runtime; that evidence comes from
//! `mini-build-runner-wasmtime`'s signed execution result
//! (`mini-pipeline-protocol`) and, ultimately, a `mini-provenance` record.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod capability;
mod error;
mod limits;
mod manifest;

pub use capability::{Capability, MAX_CAPABILITY_ARG_BYTES};
pub use error::{PipelineError, Result};
pub use limits::ResourceLimits;
pub use manifest::{
    PipelineManifest, PipelineStep, StepKind, MAX_ARGUMENTS, MAX_ARGUMENT_BYTES,
    MAX_CAPABILITIES_PER_STEP, MAX_NAME_BYTES, MAX_STEPS,
};

//! Content-addressed, length-delimited request/result messages between a
//! pipeline coordinator and an isolated build runner — self-hosted forge
//! spine Batch 2b.1 (D-0069, `docs/design/self-hosted-forge-spine.md`).
//!
//! **No Wasmtime dependency, deliberately, permanently** — same rule as
//! `mini-pipeline`. This crate is purely the wire format both sides of the
//! coordinator/runner process boundary share: [`frame::write_framed`]/
//! [`frame::read_framed`] handle the length-delimited size-bounded
//! framing; [`ExecutionRequest`] is what the coordinator sends;
//! [`ExecutionResult`] is what the runner sends back, carrying every field
//! D-0069's tenth exit criterion names (component/source/runner-binary
//! digest, Wasmtime version, runtime-config digest, capabilities granted,
//! fuel consumed, output digests, exit status).
//!
//! [`ExecutionResult::execution_security`] is the honesty seam: it is
//! always [`EXECUTION_SECURITY_WASMTIME_ISOLATED`] when produced by
//! `mini-build-runner-wasmtime`, so a future weaker or unenforced executor
//! can never reuse this same result type to silently claim isolation it
//! didn't provide — the string travels with the data, not as a
//! convention callers have to remember.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod frame;
mod request;
mod result;

pub use error::{ProtocolError, Result};
pub use frame::{read_framed, write_framed};
pub use request::ExecutionRequest;
pub use result::{
    ExecutionResult, ExitStatus, ResourceExceeded, EXECUTION_SECURITY_WASMTIME_ISOLATED,
    MAX_OUTPUTS,
};

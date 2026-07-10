//! What the isolated runner sends back: everything a `mini-provenance`
//! record needs (component/source/runner-binary digests, Wasmtime
//! version, runtime-config digest, capabilities granted, resource limits,
//! deterministic inputs, output digests, exit status -- exactly the field
//! list D-0069's exit criterion 10 names), plus an explicit
//! `execution_security` marker so a future weaker executor could never
//! silently reuse this type to claim enforcement it didn't provide.

use mini_pipeline::{Capability, MAX_CAPABILITIES_PER_STEP};

use crate::codec::{
    put_capabilities, put_digest, put_digests, put_str, put_u64, take_capabilities, take_digest,
    take_digests, take_str, take_u64,
};
use crate::error::{ProtocolError, Result};

/// Set by `mini-build-runner-wasmtime` on every result it produces.
/// Present so the *type itself* carries an honest claim about how strong
/// the isolation was, rather than callers having to infer it from which
/// runner happened to produce the message.
pub const EXECUTION_SECURITY_WASMTIME_ISOLATED: &str = "wasmtime-isolated";

/// Maximum output digests one result may report (matches
/// `mini_pipeline`'s `MAX_CAPABILITIES_PER_STEP`-scale hostile-input
/// bound; deliberately generous since a step may write many small
/// artifacts).
pub const MAX_OUTPUTS: usize = 4096;

/// Why an execution ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitStatus {
    /// The component ran to completion inside its declared limits.
    Success,
    /// The guest trapped (a WebAssembly-level fault: e.g. an unreachable
    /// instruction, an out-of-bounds access the guest itself triggered).
    GuestTrap(String),
    /// A declared resource limit was hit.
    ResourceExceeded(ResourceExceeded),
    /// The runner itself failed for a reason unrelated to the guest
    /// (I/O error setting up the sandbox, compilation failure, etc.).
    RunnerError(String),
}

/// Which declared limit was exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceExceeded {
    Fuel,
    Memory,
    WallClock,
    OutputBytes,
    StdoutBytes,
    StderrBytes,
    OpenFiles,
}

/// The isolated runner's report on one [`crate::ExecutionRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Binds this result to the exact request that produced it -- the
    /// same commit-binding discipline `mini_forge::approve` uses.
    pub request_digest: [u8; 32],
    /// Always [`EXECUTION_SECURITY_WASMTIME_ISOLATED`] from this runner.
    pub execution_security: String,
    /// Content digest of the runner binary itself.
    pub runner_binary_digest: [u8; 32],
    /// The exact Wasmtime version string in use.
    pub wasmtime_version: String,
    /// Digest of the runner's runtime configuration (feature flags,
    /// fuel/epoch policy, linker construction parameters).
    pub runtime_config_digest: [u8; 32],
    /// The capability set the linker was actually built from -- must
    /// equal the request's declared set exactly; a runner that grants
    /// anything else has a bug, and callers should treat a mismatch as a
    /// hard failure, not silently trust either side.
    pub capabilities_granted: Vec<Capability>,
    /// Digests of every file the step wrote under `artifacts:write`.
    pub output_digests: Vec<[u8; 32]>,
    pub exit_status: ExitStatus,
    /// Fuel actually consumed (recorded for reproducibility comparison
    /// across independent runners, not just as a limit check).
    pub fuel_consumed: u64,
    pub wall_clock_ms: u64,
    pub stdout_digest: [u8; 32],
    pub stderr_digest: [u8; 32],
}

impl ExecutionResult {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        put_digest(&mut w, &self.request_digest);
        put_str(&mut w, &self.execution_security);
        put_digest(&mut w, &self.runner_binary_digest);
        put_str(&mut w, &self.wasmtime_version);
        put_digest(&mut w, &self.runtime_config_digest);
        put_capabilities(&mut w, &self.capabilities_granted);
        put_digests(&mut w, &self.output_digests);
        encode_exit_status(&mut w, &self.exit_status);
        put_u64(&mut w, self.fuel_consumed);
        put_u64(&mut w, self.wall_clock_ms);
        put_digest(&mut w, &self.stdout_digest);
        put_digest(&mut w, &self.stderr_digest);
        w
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut off = 0usize;
        let request_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let execution_security = take_str(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let runner_binary_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let wasmtime_version = take_str(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let runtime_config_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let capabilities_granted = take_capabilities(b, &mut off, MAX_CAPABILITIES_PER_STEP)?;
        let output_digests = take_digests(b, &mut off, MAX_OUTPUTS)?;
        let exit_status = decode_exit_status(b, &mut off)?;
        let fuel_consumed = take_u64(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let wall_clock_ms = take_u64(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let stdout_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let stderr_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        if off != b.len() {
            return Err(ProtocolError::BadMessage);
        }
        Ok(ExecutionResult {
            request_digest,
            execution_security,
            runner_binary_digest,
            wasmtime_version,
            runtime_config_digest,
            capabilities_granted,
            output_digests,
            exit_status,
            fuel_consumed,
            wall_clock_ms,
            stdout_digest,
            stderr_digest,
        })
    }
}

const EXIT_SUCCESS: u8 = 0;
const EXIT_GUEST_TRAP: u8 = 1;
const EXIT_RESOURCE_EXCEEDED: u8 = 2;
const EXIT_RUNNER_ERROR: u8 = 3;

const RESOURCE_FUEL: u8 = 0;
const RESOURCE_MEMORY: u8 = 1;
const RESOURCE_WALL_CLOCK: u8 = 2;
const RESOURCE_OUTPUT_BYTES: u8 = 3;
const RESOURCE_STDOUT_BYTES: u8 = 4;
const RESOURCE_STDERR_BYTES: u8 = 5;
const RESOURCE_OPEN_FILES: u8 = 6;

fn encode_exit_status(w: &mut Vec<u8>, status: &ExitStatus) {
    match status {
        ExitStatus::Success => w.push(EXIT_SUCCESS),
        ExitStatus::GuestTrap(msg) => {
            w.push(EXIT_GUEST_TRAP);
            put_str(w, msg);
        }
        ExitStatus::ResourceExceeded(r) => {
            w.push(EXIT_RESOURCE_EXCEEDED);
            w.push(match r {
                ResourceExceeded::Fuel => RESOURCE_FUEL,
                ResourceExceeded::Memory => RESOURCE_MEMORY,
                ResourceExceeded::WallClock => RESOURCE_WALL_CLOCK,
                ResourceExceeded::OutputBytes => RESOURCE_OUTPUT_BYTES,
                ResourceExceeded::StdoutBytes => RESOURCE_STDOUT_BYTES,
                ResourceExceeded::StderrBytes => RESOURCE_STDERR_BYTES,
                ResourceExceeded::OpenFiles => RESOURCE_OPEN_FILES,
            });
        }
        ExitStatus::RunnerError(msg) => {
            w.push(EXIT_RUNNER_ERROR);
            put_str(w, msg);
        }
    }
}

fn decode_exit_status(b: &[u8], off: &mut usize) -> Result<ExitStatus> {
    let tag = *b.get(*off).ok_or(ProtocolError::BadMessage)?;
    *off += 1;
    match tag {
        EXIT_SUCCESS => Ok(ExitStatus::Success),
        EXIT_GUEST_TRAP => Ok(ExitStatus::GuestTrap(
            take_str(b, off).ok_or(ProtocolError::BadMessage)?,
        )),
        EXIT_RESOURCE_EXCEEDED => {
            let sub = *b.get(*off).ok_or(ProtocolError::BadMessage)?;
            *off += 1;
            let r = match sub {
                RESOURCE_FUEL => ResourceExceeded::Fuel,
                RESOURCE_MEMORY => ResourceExceeded::Memory,
                RESOURCE_WALL_CLOCK => ResourceExceeded::WallClock,
                RESOURCE_OUTPUT_BYTES => ResourceExceeded::OutputBytes,
                RESOURCE_STDOUT_BYTES => ResourceExceeded::StdoutBytes,
                RESOURCE_STDERR_BYTES => ResourceExceeded::StderrBytes,
                RESOURCE_OPEN_FILES => ResourceExceeded::OpenFiles,
                _ => return Err(ProtocolError::BadMessage),
            };
            Ok(ExitStatus::ResourceExceeded(r))
        }
        EXIT_RUNNER_ERROR => Ok(ExitStatus::RunnerError(
            take_str(b, off).ok_or(ProtocolError::BadMessage)?,
        )),
        _ => Err(ProtocolError::BadMessage),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_result(exit_status: ExitStatus) -> ExecutionResult {
        ExecutionResult {
            request_digest: [1u8; 32],
            execution_security: EXECUTION_SECURITY_WASMTIME_ISOLATED.to_string(),
            runner_binary_digest: [2u8; 32],
            wasmtime_version: "27.0.0".to_string(),
            runtime_config_digest: [3u8; 32],
            capabilities_granted: vec![Capability::WorkspaceRead],
            output_digests: vec![[4u8; 32], [5u8; 32]],
            exit_status,
            fuel_consumed: 12345,
            wall_clock_ms: 678,
            stdout_digest: [6u8; 32],
            stderr_digest: [7u8; 32],
        }
    }

    #[test]
    fn success_round_trips() {
        let r = a_result(ExitStatus::Success);
        assert_eq!(ExecutionResult::decode(&r.encode()).unwrap(), r);
    }

    #[test]
    fn every_exit_status_variant_round_trips() {
        let variants = [
            ExitStatus::Success,
            ExitStatus::GuestTrap("unreachable".to_string()),
            ExitStatus::ResourceExceeded(ResourceExceeded::Fuel),
            ExitStatus::ResourceExceeded(ResourceExceeded::Memory),
            ExitStatus::ResourceExceeded(ResourceExceeded::WallClock),
            ExitStatus::ResourceExceeded(ResourceExceeded::OutputBytes),
            ExitStatus::ResourceExceeded(ResourceExceeded::StdoutBytes),
            ExitStatus::ResourceExceeded(ResourceExceeded::StderrBytes),
            ExitStatus::ResourceExceeded(ResourceExceeded::OpenFiles),
            ExitStatus::RunnerError("compile failed".to_string()),
        ];
        for v in variants {
            let r = a_result(v.clone());
            assert_eq!(ExecutionResult::decode(&r.encode()).unwrap().exit_status, v);
        }
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let r = a_result(ExitStatus::Success);
        let mut bytes = r.encode();
        bytes.push(0xff);
        assert!(ExecutionResult::decode(&bytes).is_err());
    }

    #[test]
    fn an_unenforced_execution_security_string_is_preserved_not_silently_upgraded() {
        // Confirms this type never invents a stronger claim than what was
        // actually written -- a hostile or buggy producer claiming
        // "unenforced" round-trips as exactly that, not as
        // wasmtime-isolated.
        let mut r = a_result(ExitStatus::Success);
        r.execution_security = "unenforced".to_string();
        let decoded = ExecutionResult::decode(&r.encode()).unwrap();
        assert_eq!(decoded.execution_security, "unenforced");
    }
}

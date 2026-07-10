//! What the coordinator sends the isolated runner: exactly what to run and
//! exactly what it may do -- never more than the manifest's own declared
//! capabilities (`mini_pipeline::PipelineStep::kind`'s `capabilities`
//! list), so the runner's linker-construction step has nothing to guess
//! at or default open.

use mini_pipeline::{Capability, ResourceLimits, MAX_CAPABILITIES_PER_STEP};

use crate::codec::{
    put_capabilities, put_digest, put_u64, take_capabilities, take_digest, take_u64,
};
use crate::error::{ProtocolError, Result};

/// One request to execute a single `wasm-component` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRequest {
    /// Content id of the component's bytes.
    pub component_digest: [u8; 32],
    /// Content id of the input workspace snapshot the component sees
    /// under `workspace:read`.
    pub source_digest: [u8; 32],
    /// The exact, complete capability set to build the linker from --
    /// nothing outside this list is ever importable by the guest.
    pub capabilities: Vec<Capability>,
    /// Declared resource limits for this run.
    pub limits: ResourceLimits,
    /// Deterministic random seed (derived from the execution plan's own
    /// digest by the caller) -- never OS entropy, so `random:
    /// deterministic` steps stay reproducible.
    pub deterministic_seed: [u8; 32],
}

impl ExecutionRequest {
    /// Encode to the canonical wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::new();
        put_digest(&mut w, &self.component_digest);
        put_digest(&mut w, &self.source_digest);
        put_capabilities(&mut w, &self.capabilities);
        encode_limits(&mut w, &self.limits);
        put_digest(&mut w, &self.deterministic_seed);
        w
    }

    /// Decode from [`Self::encode`]'s wire form. Strict: rejects trailing
    /// bytes and over-bound capability counts.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut off = 0usize;
        let component_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let source_digest = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        let capabilities = take_capabilities(b, &mut off, MAX_CAPABILITIES_PER_STEP)?;
        let limits = decode_limits(b, &mut off)?;
        let deterministic_seed = take_digest(b, &mut off).ok_or(ProtocolError::BadMessage)?;
        if off != b.len() {
            return Err(ProtocolError::BadMessage);
        }
        Ok(ExecutionRequest {
            component_digest,
            source_digest,
            capabilities,
            limits,
            deterministic_seed,
        })
    }

    /// This request's own content digest -- what [`crate::ExecutionResult
    /// ::request_digest`] binds a result back to, the same commit-binding
    /// discipline `mini_forge::approve` uses for reviewed commits.
    pub fn digest(&self) -> [u8; 32] {
        blake3::hash(&self.encode()).into()
    }
}

pub(crate) fn encode_limits(w: &mut Vec<u8>, limits: &ResourceLimits) {
    put_u64(w, limits.max_fuel);
    put_u64(w, limits.max_memory_bytes);
    put_u64(w, limits.max_wall_clock_ms);
    put_u64(w, limits.max_output_bytes);
    put_u64(w, limits.max_stdout_bytes);
    put_u64(w, limits.max_stderr_bytes);
    put_u64(w, limits.max_open_files as u64);
}

pub(crate) fn decode_limits(b: &[u8], off: &mut usize) -> Result<ResourceLimits> {
    let max_fuel = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_memory_bytes = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_wall_clock_ms = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_output_bytes = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_stdout_bytes = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_stderr_bytes = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_open_files_u64 = take_u64(b, off).ok_or(ProtocolError::BadMessage)?;
    let max_open_files =
        u32::try_from(max_open_files_u64).map_err(|_| ProtocolError::BadMessage)?;
    Ok(ResourceLimits {
        max_fuel,
        max_memory_bytes,
        max_wall_clock_ms,
        max_output_bytes,
        max_stdout_bytes,
        max_stderr_bytes,
        max_open_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_request() -> ExecutionRequest {
        ExecutionRequest {
            component_digest: [1u8; 32],
            source_digest: [2u8; 32],
            capabilities: vec![Capability::WorkspaceRead, Capability::ArtifactsWrite],
            limits: ResourceLimits::conservative_default(),
            deterministic_seed: [3u8; 32],
        }
    }

    #[test]
    fn round_trips() {
        let req = a_request();
        let decoded = ExecutionRequest::decode(&req.encode()).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let req = a_request();
        let mut bytes = req.encode();
        bytes.push(0xff);
        assert!(ExecutionRequest::decode(&bytes).is_err());
    }

    #[test]
    fn truncated_bytes_are_rejected() {
        let req = a_request();
        let mut bytes = req.encode();
        bytes.truncate(bytes.len() - 5);
        assert!(ExecutionRequest::decode(&bytes).is_err());
    }

    #[test]
    fn digest_is_deterministic_and_sensitive_to_content() {
        let a = a_request();
        let mut b = a_request();
        b.component_digest[0] ^= 1;
        assert_eq!(a.digest(), a.digest());
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn too_many_capabilities_is_rejected() {
        let mut req = a_request();
        req.capabilities = (0..MAX_CAPABILITIES_PER_STEP + 1)
            .map(|_| Capability::WorkspaceRead)
            .collect();
        let bytes = req.encode();
        assert!(ExecutionRequest::decode(&bytes).is_err());
    }
}

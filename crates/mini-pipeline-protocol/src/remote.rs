//! Bounded messages for sending one exact build to a remote volunteer worker.
//!
//! This is transport-neutral. Callers carry the bytes inside an already
//! encrypted `mini-bearer::Channel`; the isolated runner still receives the
//! narrower [`crate::ExecutionRequest`] over its local subprocess pipe.

use std::collections::BTreeSet;

use crate::{
    ExecutionRequest, ExecutionResult, ProtocolError, Result, EXECUTION_SECURITY_WASMTIME_ISOLATED,
};

pub const MAX_REMOTE_COMPONENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_REMOTE_WORKSPACE_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_REMOTE_ARTIFACT_BYTES: usize = 14 * 1024 * 1024;
pub const MAX_REMOTE_REQUEST_BYTES: usize = 15 * 1024 * 1024;
pub const MAX_REMOTE_RESPONSE_BYTES: usize = 15 * 1024 * 1024;
pub const MAX_WORKSPACE_FILES: usize = 16_384;
const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBuildRequest {
    pub execution: ExecutionRequest,
    pub component: Vec<u8>,
    pub workspace: Vec<WorkspaceFile>,
}

impl RemoteBuildRequest {
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let execution = self.execution.encode();
        let mut out = Vec::new();
        put_bytes(&mut out, &execution)?;
        put_bytes(&mut out, &self.component)?;
        put_count(&mut out, self.workspace.len())?;
        for file in &self.workspace {
            put_bytes(&mut out, file.path.as_bytes())?;
            put_bytes(&mut out, &file.bytes)?;
        }
        if out.len() > MAX_REMOTE_REQUEST_BYTES {
            return Err(too_large(out.len(), MAX_REMOTE_REQUEST_BYTES));
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_REMOTE_REQUEST_BYTES {
            return Err(too_large(bytes.len(), MAX_REMOTE_REQUEST_BYTES));
        }
        let mut cursor = Cursor::new(bytes);
        let execution = ExecutionRequest::decode(cursor.bytes(1024 * 1024)?)?;
        let component = cursor.bytes(MAX_REMOTE_COMPONENT_BYTES)?.to_vec();
        let count = cursor.count(MAX_WORKSPACE_FILES)?;
        let mut workspace = Vec::with_capacity(count);
        for _ in 0..count {
            let path = std::str::from_utf8(cursor.bytes(MAX_PATH_BYTES)?)
                .map_err(|_| ProtocolError::BadMessage)?
                .to_owned();
            let file = cursor.bytes(MAX_REMOTE_WORKSPACE_BYTES)?.to_vec();
            workspace.push(WorkspaceFile { path, bytes: file });
        }
        cursor.finish()?;
        let request = Self {
            execution,
            component,
            workspace,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<()> {
        if self.execution.capabilities.iter().any(|capability| {
            matches!(
                capability,
                mini_pipeline::Capability::NetworkHost(_)
                    | mini_pipeline::Capability::SecretRead(_)
            )
        }) {
            // A volunteer worker must never be asked to expose its network or
            // secrets. Those capabilities require a separately authenticated,
            // owner-configured execution lane.
            return Err(ProtocolError::BadMessage);
        }
        if self.component.len() > MAX_REMOTE_COMPONENT_BYTES
            || blake3::hash(&self.component).as_bytes() != &self.execution.component_digest
        {
            return Err(ProtocolError::BadMessage);
        }
        if self.workspace.len() > MAX_WORKSPACE_FILES {
            return Err(ProtocolError::BadMessage);
        }
        let mut total = 0usize;
        let mut paths = BTreeSet::new();
        for file in &self.workspace {
            validate_relative_path(&file.path)?;
            if !paths.insert(file.path.as_str()) {
                return Err(ProtocolError::BadMessage);
            }
            total = total
                .checked_add(file.bytes.len())
                .ok_or(ProtocolError::BadMessage)?;
            if total > MAX_REMOTE_WORKSPACE_BYTES {
                return Err(ProtocolError::BadMessage);
            }
        }
        if workspace_digest(&self.workspace) != self.execution.source_digest {
            return Err(ProtocolError::BadMessage);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBuildResponse {
    pub result: ExecutionResult,
    pub artifacts: Vec<Vec<u8>>,
}

impl RemoteBuildResponse {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let result = self.result.encode();
        let mut out = Vec::new();
        put_bytes(&mut out, &result)?;
        put_count(&mut out, self.artifacts.len())?;
        let mut total = 0usize;
        for artifact in &self.artifacts {
            total = total
                .checked_add(artifact.len())
                .ok_or(ProtocolError::BadMessage)?;
            if total > MAX_REMOTE_ARTIFACT_BYTES {
                return Err(ProtocolError::BadMessage);
            }
            put_bytes(&mut out, artifact)?;
        }
        if out.len() > MAX_REMOTE_RESPONSE_BYTES {
            return Err(too_large(out.len(), MAX_REMOTE_RESPONSE_BYTES));
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_REMOTE_RESPONSE_BYTES {
            return Err(too_large(bytes.len(), MAX_REMOTE_RESPONSE_BYTES));
        }
        let mut cursor = Cursor::new(bytes);
        let result = ExecutionResult::decode(cursor.bytes(1024 * 1024)?)?;
        let count = cursor.count(crate::MAX_OUTPUTS)?;
        let mut artifacts = Vec::with_capacity(count);
        let mut total = 0usize;
        for _ in 0..count {
            let artifact = cursor.bytes(MAX_REMOTE_ARTIFACT_BYTES)?.to_vec();
            total = total
                .checked_add(artifact.len())
                .ok_or(ProtocolError::BadMessage)?;
            if total > MAX_REMOTE_ARTIFACT_BYTES {
                return Err(ProtocolError::BadMessage);
            }
            artifacts.push(artifact);
        }
        cursor.finish()?;
        Ok(Self { result, artifacts })
    }

    pub fn verify_for(&self, request: &ExecutionRequest) -> Result<()> {
        if self.result.request_digest != request.digest()
            || self.result.execution_security != EXECUTION_SECURITY_WASMTIME_ISOLATED
            || self.result.capabilities_granted != request.capabilities
            || self.artifacts.len() != self.result.output_digests.len()
        {
            return Err(ProtocolError::BadMessage);
        }
        for (artifact, claimed) in self.artifacts.iter().zip(&self.result.output_digests) {
            if blake3::hash(artifact).as_bytes() != claimed {
                return Err(ProtocolError::BadMessage);
            }
        }
        Ok(())
    }
}

pub fn workspace_digest(files: &[WorkspaceFile]) -> [u8; 32] {
    let mut ordered: Vec<_> = files.iter().collect();
    ordered.sort_by(|a, b| a.path.cmp(&b.path));
    let mut hasher = blake3::Hasher::new();
    for file in ordered {
        hasher.update(file.path.as_bytes());
        hasher.update(&[0]);
        hasher.update(&(file.bytes.len() as u64).to_le_bytes());
        hasher.update(&file.bytes);
    }
    *hasher.finalize().as_bytes()
}

fn validate_relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains('\\')
        || path.contains('\0')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ProtocolError::BadMessage);
    }
    Ok(())
}

fn put_count(out: &mut Vec<u8>, value: usize) -> Result<()> {
    let value = u32::try_from(value).map_err(|_| ProtocolError::BadMessage)?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    put_count(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn too_large(declared: usize, max: usize) -> ProtocolError {
    ProtocolError::MessageTooLarge {
        declared: u32::try_from(declared).unwrap_or(u32::MAX),
        max,
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn count(&mut self, max: usize) -> Result<usize> {
        let raw: [u8; 4] = self
            .take(4)?
            .try_into()
            .expect("four-byte slice has exact length");
        let count = u32::from_be_bytes(raw) as usize;
        if count > max {
            return Err(too_large(count, max));
        }
        Ok(count)
    }

    fn bytes(&mut self, max: usize) -> Result<&'a [u8]> {
        let len = self.count(max)?;
        self.take(len)
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(ProtocolError::BadMessage)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ProtocolError::BadMessage)?;
        self.offset = end;
        Ok(value)
    }

    fn finish(self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::BadMessage)
        }
    }
}

#[cfg(test)]
mod tests {
    use mini_pipeline::{Capability, ResourceLimits};

    use super::*;

    fn request() -> RemoteBuildRequest {
        let component = b"component".to_vec();
        let workspace = vec![WorkspaceFile {
            path: "src/main.rs".into(),
            bytes: b"fn main() {}".to_vec(),
        }];
        RemoteBuildRequest {
            execution: ExecutionRequest {
                component_digest: blake3::hash(&component).into(),
                source_digest: workspace_digest(&workspace),
                capabilities: vec![Capability::WorkspaceRead],
                limits: ResourceLimits::conservative_default(),
                deterministic_seed: [7; 32],
            },
            component,
            workspace,
        }
    }

    #[test]
    fn request_round_trips_and_binds_content() {
        let request = request();
        assert_eq!(
            RemoteBuildRequest::decode(&request.encode().unwrap()).unwrap(),
            request
        );
    }

    #[test]
    fn traversal_and_tampering_fail_closed() {
        let mut traversal = request();
        traversal.workspace[0].path = "../escape".into();
        assert!(traversal.encode().is_err());

        let mut tampered = request();
        tampered.component.push(0);
        assert!(tampered.encode().is_err());

        let mut ambient = request();
        ambient.execution.capabilities = vec![Capability::SecretRead("token".into())];
        assert!(ambient.encode().is_err());
    }

    fn response_for(request: &ExecutionRequest, artifact: &[u8]) -> RemoteBuildResponse {
        RemoteBuildResponse {
            result: ExecutionResult {
                request_digest: request.digest(),
                execution_security: EXECUTION_SECURITY_WASMTIME_ISOLATED.to_string(),
                runner_binary_digest: [2; 32],
                wasmtime_version: "test".into(),
                runtime_config_digest: [3; 32],
                capabilities_granted: request.capabilities.clone(),
                output_digests: vec![blake3::hash(artifact).into()],
                exit_status: crate::ExitStatus::Success,
                fuel_consumed: 1,
                wall_clock_ms: 1,
                stdout_digest: blake3::hash(b"").into(),
                stderr_digest: blake3::hash(b"").into(),
            },
            artifacts: vec![artifact.to_vec()],
        }
    }

    #[test]
    fn response_round_trips_and_rejects_untrusted_bindings() {
        let request = request();
        let response = response_for(&request.execution, b"artifact");
        let decoded = RemoteBuildResponse::decode(&response.encode().unwrap()).unwrap();
        decoded.verify_for(&request.execution).unwrap();

        let mut wrong_request = decoded.clone();
        wrong_request.result.request_digest[0] ^= 1;
        assert!(wrong_request.verify_for(&request.execution).is_err());

        let mut wrong_artifact = decoded.clone();
        wrong_artifact.artifacts[0][0] ^= 1;
        assert!(wrong_artifact.verify_for(&request.execution).is_err());

        let mut false_isolation = decoded;
        false_isolation.result.execution_security = "unenforced".into();
        assert!(false_isolation.verify_for(&request.execution).is_err());
    }
}

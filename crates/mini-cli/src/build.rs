//! `mini build run` — drive a real sandboxed build through the actual
//! compiled `mini-build-runner-wasmtime` binary, as a genuine child
//! process speaking real `mini-pipeline-protocol` framing over its
//! stdin/stdout. `mini-cli` never links that crate in-process: per its
//! own module docs (D-0069), it is the only crate in this workspace
//! permitted to link Wasmtime, and every other caller must treat it as a
//! subprocess, never a library.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use mini_bearer::{Bearer, Initiator, Responder, TcpBearer};
use mini_pipeline::{Capability, ResourceLimits};
use mini_pipeline_protocol::{
    read_framed, workspace_digest, write_framed, ExecutionRequest, ExecutionResult,
    RemoteBuildRequest, RemoteBuildResponse, WorkspaceFile,
};

use crate::error::{CliError, Result};
use crate::json::{CommandResult, JsonValue};

const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
const HARD_TIMEOUT: Duration = Duration::from_secs(120);
const RUNNER_BIN_NAME: &str = "mini-build-runner-wasmtime";
const REMOTE_BUILD_AAD: &[u8] = b"MINI/BUILD-WORKER1";

/// Locate the real runner binary: first next to this `mini` executable
/// (the expected production layout, both binaries installed side by
/// side), then fall back to letting `PATH` resolve it. Does not itself
/// verify the fallback exists -- a missing binary surfaces as a clear
/// spawn error either way.
fn runner_binary_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(RUNNER_BIN_NAME);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(RUNNER_BIN_NAME)
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_capability(s: &str) -> Result<Capability> {
    Ok(match s {
        "workspace-read" => Capability::WorkspaceRead,
        "scratch-write" => Capability::ScratchWrite,
        "artifacts-write" => Capability::ArtifactsWrite,
        "clock-monotonic" => Capability::ClockMonotonic,
        "random-deterministic" => Capability::RandomDeterministic,
        other => {
            if let Some(host) = other.strip_prefix("network-host:") {
                Capability::NetworkHost(host.to_string())
            } else if let Some(name) = other.strip_prefix("secret-read:") {
                Capability::SecretRead(name.to_string())
            } else {
                return Err(CliError::Usage(format!("unknown --capability {other:?} -- expected one of workspace-read, scratch-write, artifacts-write, clock-monotonic, random-deterministic, network-host:<host>, secret-read:<name>")));
            }
        }
    })
}

/// `mini build run --component <path> --store-dir <dir> --scratch-dir
/// <dir> --artifacts-dir <dir> [--capability <cap>]... [--max-fuel N]
/// [--max-memory-bytes N] [--max-wall-clock-ms N] [--max-output-bytes N]`
///
/// Content-addresses `component` (and an empty workspace snapshot, since
/// this command runs standalone rather than against a `mini repo`
/// checkout) under `store_dir` the way the runner's own on-disk protocol
/// expects, then spawns the real binary and reports its attested result.
#[allow(clippy::too_many_arguments)]
pub fn run(
    component_path: &Path,
    store_dir: &Path,
    scratch_dir: &Path,
    artifacts_dir: &Path,
    capabilities: Vec<Capability>,
    limits: ResourceLimits,
) -> Result<CommandResult> {
    let component = std::fs::read(component_path).map_err(|e| CliError::Io(e.to_string()))?;

    std::fs::create_dir_all(store_dir.join("objects")).map_err(|e| CliError::Io(e.to_string()))?;
    std::fs::create_dir_all(store_dir.join("workspaces"))
        .map_err(|e| CliError::Io(e.to_string()))?;
    std::fs::create_dir_all(scratch_dir).map_err(|e| CliError::Io(e.to_string()))?;
    std::fs::create_dir_all(artifacts_dir).map_err(|e| CliError::Io(e.to_string()))?;

    let component_digest: [u8; 32] = blake3::hash(&component).into();
    std::fs::write(
        store_dir.join("objects").join(hex(&component_digest)),
        &component,
    )
    .map_err(|e| CliError::Io(e.to_string()))?;

    // No source workspace for a standalone `mini build run` -- the
    // runner still wants a valid (possibly empty) content-addressed
    // snapshot to point at.
    let empty_workspace_dir = store_dir.join("workspaces").join("empty");
    std::fs::create_dir_all(&empty_workspace_dir).map_err(|e| CliError::Io(e.to_string()))?;
    let source_digest = mini_build_runner_wasmtime_content_store_hash(&empty_workspace_dir)?;
    let renamed = store_dir.join("workspaces").join(hex(&source_digest));
    if renamed != empty_workspace_dir {
        let _ = std::fs::remove_dir_all(&renamed);
        std::fs::rename(&empty_workspace_dir, &renamed).map_err(|e| CliError::Io(e.to_string()))?;
    }

    let request = ExecutionRequest {
        component_digest,
        source_digest,
        capabilities,
        limits,
        deterministic_seed: [0u8; 32],
    };

    let result = execute_runner(&request, store_dir, scratch_dir, artifacts_dir)?;
    Ok(format_result(&result))
}

fn execute_runner(
    request: &ExecutionRequest,
    store_dir: &Path,
    scratch_dir: &Path,
    artifacts_dir: &Path,
) -> Result<ExecutionResult> {
    let binary = runner_binary_path();
    let mut child = Command::new(&binary)
        .arg("--store-dir")
        .arg(store_dir)
        .arg("--scratch-dir")
        .arg(scratch_dir)
        .arg("--artifacts-dir")
        .arg(artifacts_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            CliError::Build(format!(
                "failed to spawn {binary:?} -- is it installed next to `mini` or on PATH? ({e})"
            ))
        })?;

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let encoded = request.encode();
    let writer = std::thread::spawn(move || {
        let _ = write_framed(&mut stdin, &encoded);
    });
    let stderr_reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let frame = read_framed(&mut stdout, MAX_RESULT_BYTES);
        let _ = tx.send(frame);
    });

    let frame = rx.recv_timeout(HARD_TIMEOUT).map_err(|_| {
        let _ = child.kill();
        CliError::Build("build runner did not respond within the timeout".to_string())
    })?;
    let _ = writer.join();
    let status = child.wait().map_err(|e| CliError::Io(e.to_string()))?;
    let stderr_bytes = stderr_reader.join().unwrap_or_default();

    let frame = frame
        .map_err(|e| CliError::Build(format!("reading runner result failed: {e}")))?
        .ok_or_else(|| {
            CliError::Build(format!(
                "runner exited (status {status:?}) without a result frame; stderr: {}",
                String::from_utf8_lossy(&stderr_bytes)
            ))
        })?;
    ExecutionResult::decode(&frame)
        .map_err(|e| CliError::Build(format!("undecodable runner result: {e}")))
}

fn format_result(result: &ExecutionResult) -> CommandResult {
    let mut out = format!(
        "exit_status: {:?}\nexecution_security: {}\nfuel_consumed: {}\nwall_clock_ms: {}\n",
        result.exit_status, result.execution_security, result.fuel_consumed, result.wall_clock_ms
    );
    out.push_str(&format!(
        "runner_binary_digest: {}\nruntime_config_digest: {}\n",
        hex(&result.runner_binary_digest),
        hex(&result.runtime_config_digest)
    ));
    for d in &result.output_digests {
        out.push_str(&format!("output_digest: {}\n", hex(d)));
    }

    let output_digest_hexes: Vec<String> = result.output_digests.iter().map(hex).collect();
    CommandResult::new(out)
        .field(
            "exit_status",
            JsonValue::str(format!("{:?}", result.exit_status)),
        )
        .field(
            "execution_security",
            JsonValue::str(result.execution_security.to_string()),
        )
        .field("fuel_consumed", JsonValue::num(result.fuel_consumed))
        .field("wall_clock_ms", JsonValue::num(result.wall_clock_ms))
        .field(
            "runner_binary_digest",
            JsonValue::str(hex(&result.runner_binary_digest)),
        )
        .field(
            "runtime_config_digest",
            JsonValue::str(hex(&result.runtime_config_digest)),
        )
        .field("output_digests", JsonValue::strs(output_digest_hexes))
}

/// Serve one explicitly requested remote build and then exit.
pub fn serve(addr: &str, work_dir: &Path) -> Result<CommandResult> {
    if work_dir.exists() {
        return Err(CliError::Build(format!(
            "worker directory already exists: {}",
            work_dir.display()
        )));
    }
    let store_dir = work_dir.join("store");
    let scratch_dir = work_dir.join("scratch");
    let artifacts_dir = work_dir.join("artifacts");
    std::fs::create_dir_all(store_dir.join("objects"))
        .and_then(|_| std::fs::create_dir_all(store_dir.join("workspaces")))
        .and_then(|_| std::fs::create_dir_all(&scratch_dir))
        .and_then(|_| std::fs::create_dir_all(&artifacts_dir))
        .map_err(|e| CliError::Io(e.to_string()))?;

    let listener = TcpListener::bind(addr).map_err(|e| CliError::Io(e.to_string()))?;
    let (stream, peer) = listener.accept().map_err(|e| CliError::Io(e.to_string()))?;
    let mut bearer = TcpBearer::from_stream(stream).map_err(|e| CliError::Build(e.to_string()))?;
    let hello = bearer.recv().map_err(|e| CliError::Build(e.to_string()))?;
    let (mut channel, response) =
        Responder::respond(&hello).map_err(|e| CliError::Build(e.to_string()))?;
    bearer
        .send(&response)
        .map_err(|e| CliError::Build(e.to_string()))?;

    let ciphertext = bearer.recv().map_err(|e| CliError::Build(e.to_string()))?;
    let plaintext = channel
        .open(&ciphertext, REMOTE_BUILD_AAD)
        .map_err(|e| CliError::Build(e.to_string()))?;
    let remote = RemoteBuildRequest::decode(&plaintext)
        .map_err(|e| CliError::Build(format!("invalid remote build request: {e}")))?;

    materialize_remote_request(&remote, &store_dir)?;
    let result = execute_runner(&remote.execution, &store_dir, &scratch_dir, &artifacts_dir)?;
    let artifacts = collect_artifact_bytes(&artifacts_dir)?;
    let response = RemoteBuildResponse { result, artifacts };
    response
        .verify_for(&remote.execution)
        .map_err(|e| CliError::Build(format!("local runner result failed verification: {e}")))?;
    let encoded = response
        .encode()
        .map_err(|e| CliError::Build(e.to_string()))?;
    let ciphertext = channel
        .seal(&encoded, REMOTE_BUILD_AAD)
        .map_err(|e| CliError::Build(e.to_string()))?;
    bearer
        .send(&ciphertext)
        .map_err(|e| CliError::Build(e.to_string()))?;
    Ok(
        CommandResult::new(format!("served one verified build to {peer}\n"))
            .field("peer", JsonValue::str(peer.to_string()))
            .field(
                "request_digest",
                JsonValue::str(hex(&remote.execution.digest())),
            ),
    )
}

/// Dispatch one exact build to a reachable worker and verify its response.
#[allow(clippy::too_many_arguments)]
pub fn dispatch(
    peer: &str,
    component_path: &Path,
    workspace_dir: &Path,
    artifacts_dir: &Path,
    capabilities: Vec<Capability>,
    limits: ResourceLimits,
) -> Result<CommandResult> {
    if artifacts_dir.exists() {
        return Err(CliError::Build(format!(
            "output directory already exists: {}",
            artifacts_dir.display()
        )));
    }
    let component = std::fs::read(component_path).map_err(|e| CliError::Io(e.to_string()))?;
    let workspace = read_workspace(workspace_dir)?;
    let execution = ExecutionRequest {
        component_digest: blake3::hash(&component).into(),
        source_digest: workspace_digest(&workspace),
        capabilities,
        limits,
        deterministic_seed: [0; 32],
    };
    let remote = RemoteBuildRequest {
        execution: execution.clone(),
        component,
        workspace,
    };
    let encoded = remote
        .encode()
        .map_err(|e| CliError::Build(format!("remote request exceeds policy: {e}")))?;

    let mut bearer = TcpBearer::connect(peer).map_err(|e| CliError::Build(e.to_string()))?;
    let (initiator, hello) = Initiator::start().map_err(|e| CliError::Build(e.to_string()))?;
    bearer
        .send(&hello)
        .map_err(|e| CliError::Build(e.to_string()))?;
    let handshake = bearer.recv().map_err(|e| CliError::Build(e.to_string()))?;
    let mut channel = initiator
        .finish(&handshake)
        .map_err(|e| CliError::Build(e.to_string()))?;
    let ciphertext = channel
        .seal(&encoded, REMOTE_BUILD_AAD)
        .map_err(|e| CliError::Build(e.to_string()))?;
    bearer
        .send(&ciphertext)
        .map_err(|e| CliError::Build(e.to_string()))?;

    let ciphertext = bearer.recv().map_err(|e| CliError::Build(e.to_string()))?;
    let plaintext = channel
        .open(&ciphertext, REMOTE_BUILD_AAD)
        .map_err(|e| CliError::Build(e.to_string()))?;
    let response = RemoteBuildResponse::decode(&plaintext)
        .map_err(|e| CliError::Build(format!("invalid worker response: {e}")))?;
    response
        .verify_for(&execution)
        .map_err(|e| CliError::Build(format!("untrusted worker response rejected: {e}")))?;

    std::fs::create_dir_all(artifacts_dir).map_err(|e| CliError::Io(e.to_string()))?;
    for (digest, bytes) in response
        .result
        .output_digests
        .iter()
        .zip(&response.artifacts)
    {
        std::fs::write(artifacts_dir.join(hex(digest)), bytes)
            .map_err(|e| CliError::Io(e.to_string()))?;
    }
    Ok(format_result(&response.result)
        .field("peer", JsonValue::str(peer.to_string()))
        .field("request_digest", JsonValue::str(hex(&execution.digest()))))
}

fn materialize_remote_request(request: &RemoteBuildRequest, store_dir: &Path) -> Result<()> {
    std::fs::write(
        store_dir
            .join("objects")
            .join(hex(&request.execution.component_digest)),
        &request.component,
    )
    .map_err(|e| CliError::Io(e.to_string()))?;
    let workspace_dir = store_dir
        .join("workspaces")
        .join(hex(&request.execution.source_digest));
    std::fs::create_dir_all(&workspace_dir).map_err(|e| CliError::Io(e.to_string()))?;
    for file in &request.workspace {
        let path = workspace_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::Io(e.to_string()))?;
        }
        std::fs::write(path, &file.bytes).map_err(|e| CliError::Io(e.to_string()))?;
    }
    Ok(())
}

fn read_workspace(dir: &Path) -> Result<Vec<WorkspaceFile>> {
    let mut entries = Vec::new();
    collect_files(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries
        .into_iter()
        .map(|(path, bytes)| WorkspaceFile {
            path: path.replace('\\', "/"),
            bytes,
        })
        .collect())
}

fn collect_artifact_bytes(dir: &Path) -> Result<Vec<Vec<u8>>> {
    let mut entries = Vec::new();
    collect_files(dir, dir, &mut entries)?;
    let mut artifacts: Vec<Vec<u8>> = entries.into_iter().map(|(_, bytes)| bytes).collect();
    artifacts.sort_by_key(|bytes| *blake3::hash(bytes).as_bytes());
    Ok(artifacts)
}

/// Content-address a directory the same way the runner verifies workspace
/// snapshots -- re-implemented here (a handful of lines) rather than
/// depending on `mini-build-runner-wasmtime`'s library surface for it,
/// keeping the D-0069 "subprocess only" boundary exact: this crate's only
/// dependency-graph edge to that crate is `[dev-dependencies]`, used
/// solely by tests.
fn mini_build_runner_wasmtime_content_store_hash(dir: &Path) -> Result<[u8; 32]> {
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_files(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = blake3::Hasher::new();
    for (rel, content) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(&[0u8]);
        // Little-endian, matching `mini-build-runner-wasmtime::content_store::
        // hash_directory_tree` exactly -- this only ever hashes an empty
        // workspace today (zero files means zero bytes fed to the hasher
        // regardless of endianness, so a mismatch here would stay
        // undetected), but the doc comment above claims byte-for-byte
        // compatibility and this makes that literally true rather than
        // true by accident of an empty input.
        hasher.update(&(content.len() as u64).to_le_bytes());
        hasher.update(content);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).map_err(|e| CliError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| CliError::Io(e.to_string()))?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| CliError::Io(e.to_string()))?;
        if file_type.is_symlink() {
            return Err(CliError::Build(format!(
                "workspace/artifact symlinks are not allowed: {}",
                path.display()
            )));
        } else if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let content = std::fs::read(&path).map_err(|e| CliError::Io(e.to_string()))?;
            out.push((rel, content));
        } else {
            return Err(CliError::Build(format!(
                "workspace/artifact entry is not a regular file: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn parse_capabilities(raw: Vec<String>) -> Result<Vec<Capability>> {
    raw.into_iter().map(|s| parse_capability(&s)).collect()
}

pub fn default_limits() -> ResourceLimits {
    ResourceLimits {
        max_fuel: 200_000_000,
        max_memory_bytes: 128 * 1024 * 1024,
        max_wall_clock_ms: 10_000,
        max_output_bytes: 32 * 1024 * 1024,
        max_stdout_bytes: 1024 * 1024,
        max_stderr_bytes: 1024 * 1024,
        max_open_files: 32,
    }
}

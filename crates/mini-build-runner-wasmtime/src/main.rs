//! The isolated build runner's entry point (D-0069, self-hosted forge
//! spine Batch 2b.2). One process, one [`mini_pipeline_protocol::
//! ExecutionRequest`], one [`mini_pipeline_protocol::ExecutionResult`]:
//! reads exactly one framed request from stdin, executes it under
//! [`sandbox::execute`], writes exactly one framed result to stdout, then
//! exits. A fresh process per step -- rather than a long-lived loop
//! serving many requests -- is the deliberate choice: it gives the
//! coordinator the cleanest possible cancellation story (kill the child)
//! and guarantees no state from one step's `Store` can ever leak into
//! another's.
//!
//! Usage: `mini-build-runner-wasmtime --store-dir <path>
//! --scratch-dir <path> --artifacts-dir <path>`. `--store-dir` is the
//! content-addressed store the request's `component_digest`/
//! `source_digest` resolve against (see [`content_store`]);
//! `--scratch-dir`/`--artifacts-dir` are host directories this one
//! invocation owns exclusively -- the coordinator is responsible for
//! giving each step its own, never a shared or reused directory.

use std::io::{self, Write};
use std::path::PathBuf;

use mini_pipeline_protocol::{read_framed, write_framed, ExecutionRequest, ExecutionResult};

use mini_build_runner_wasmtime::content_store;
use mini_build_runner_wasmtime::error::{Result, RunnerError};
use mini_build_runner_wasmtime::sandbox;

/// The exact Wasmtime version this runner is built against -- kept as a
/// literal in lockstep with `Cargo.toml`'s `=46.0.1` pin (D-0069
/// dependency-governance requirement: this string travels into every
/// provenance record, so it must never silently drift from what was
/// actually compiled in).
const WASMTIME_VERSION: &str = "46.0.1";

/// Refuses any single message over 256 MiB before allocating a buffer for
/// it -- generous enough for a real component or workspace manifest,
/// small enough that a hostile or buggy coordinator can't force an
/// unbounded allocation.
const MAX_MESSAGE_BYTES: usize = 256 * 1024 * 1024;

struct Args {
    store_dir: PathBuf,
    scratch_dir: PathBuf,
    artifacts_dir: PathBuf,
}

fn parse_args() -> Args {
    let mut store_dir = None;
    let mut scratch_dir = None;
    let mut artifacts_dir = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().unwrap_or_else(|| {
            eprintln!("missing value for {flag}");
            std::process::exit(2);
        });
        match flag.as_str() {
            "--store-dir" => store_dir = Some(PathBuf::from(value)),
            "--scratch-dir" => scratch_dir = Some(PathBuf::from(value)),
            "--artifacts-dir" => artifacts_dir = Some(PathBuf::from(value)),
            other => {
                eprintln!("unknown flag {other}");
                std::process::exit(2);
            }
        }
    }
    match (store_dir, scratch_dir, artifacts_dir) {
        (Some(store_dir), Some(scratch_dir), Some(artifacts_dir)) => Args {
            store_dir,
            scratch_dir,
            artifacts_dir,
        },
        _ => {
            eprintln!(
                "usage: mini-build-runner-wasmtime --store-dir <path> --scratch-dir <path> --artifacts-dir <path>"
            );
            std::process::exit(2);
        }
    }
}

fn main() {
    let args = parse_args();
    if let Err(e) = run(&args) {
        eprintln!("mini-build-runner-wasmtime: {e}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<()> {
    let stdin = io::stdin();
    let mut lock = stdin.lock();
    let frame = read_framed(&mut lock, MAX_MESSAGE_BYTES)?.ok_or(RunnerError::NoRequest)?;
    let request = ExecutionRequest::decode(&frame).map_err(RunnerError::Protocol)?;

    let result = execute_request(args, &request)?;

    let stdout = io::stdout();
    let mut lock = stdout.lock();
    write_framed(&mut lock, &result.encode())?;
    lock.flush()?;
    Ok(())
}

fn execute_request(args: &Args, request: &ExecutionRequest) -> Result<ExecutionResult> {
    let component_bytes =
        content_store::read_verified_component(&args.store_dir, &request.component_digest)?;
    let workspace_dir =
        content_store::verified_workspace_dir(&args.store_dir, &request.source_digest)?;

    std::fs::create_dir_all(&args.scratch_dir)?;
    std::fs::create_dir_all(&args.artifacts_dir)?;

    let workspace = sandbox::Workspace {
        workspace_dir: Some(workspace_dir.as_path()),
        scratch_dir: Some(args.scratch_dir.as_path()),
        artifacts_dir: Some(args.artifacts_dir.as_path()),
    };

    let mut outcome = sandbox::execute(
        &component_bytes,
        &workspace,
        &request.capabilities,
        &request.limits,
        request.deterministic_seed,
    )?;

    let (output_digests, total_output_bytes) = collect_output_digests(&args.artifacts_dir)?;
    // `max_output_bytes` is enforced here, after the run: WASI's
    // filesystem host functions have no live per-directory quota to
    // check mid-write, so a step that stayed within its fuel/memory/wall-
    // clock budget but wrote too much can still only be caught by
    // inspecting what it actually left behind.
    if matches!(
        outcome.exit_status,
        mini_pipeline_protocol::ExitStatus::Success
    ) && total_output_bytes > request.limits.max_output_bytes
    {
        outcome.exit_status = mini_pipeline_protocol::ExitStatus::ResourceExceeded(
            mini_pipeline_protocol::ResourceExceeded::OutputBytes,
        );
    }

    let runner_binary_digest = hash_self()?;
    let runtime_config_digest = hash_runtime_config();

    Ok(ExecutionResult {
        request_digest: request.digest(),
        execution_security: mini_pipeline_protocol::EXECUTION_SECURITY_WASMTIME_ISOLATED
            .to_string(),
        runner_binary_digest,
        wasmtime_version: WASMTIME_VERSION.to_string(),
        runtime_config_digest,
        capabilities_granted: request.capabilities.clone(),
        output_digests,
        exit_status: outcome.exit_status,
        fuel_consumed: outcome.fuel_consumed,
        wall_clock_ms: outcome.wall_clock_ms,
        stdout_digest: blake3::hash(&outcome.stdout).into(),
        stderr_digest: blake3::hash(&outcome.stderr).into(),
    })
}

/// Every file written under `artifacts:write`, digested individually and
/// sorted by digest so the list is deterministic regardless of
/// filesystem iteration order, plus the total byte count for the
/// `max_output_bytes` check in [`execute_request`].
fn collect_output_digests(artifacts_dir: &std::path::Path) -> Result<(Vec<[u8; 32]>, u64)> {
    let mut digests = Vec::new();
    let mut total_bytes: u64 = 0;
    walk_artifacts(artifacts_dir, &mut |bytes: &[u8]| {
        total_bytes += bytes.len() as u64;
        digests.push(blake3::hash(bytes).into());
    })?;
    digests.sort();
    Ok((digests, total_bytes))
}

fn walk_artifacts(dir: &std::path::Path, on_file: &mut impl FnMut(&[u8])) -> Result<()> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_artifacts(&path, on_file)?;
        } else if file_type.is_file() {
            let bytes = std::fs::read(&path)?;
            on_file(&bytes);
        }
    }
    Ok(())
}

fn hash_self() -> Result<[u8; 32]> {
    let exe = std::env::current_exe()?;
    let bytes = std::fs::read(exe)?;
    Ok(blake3::hash(&bytes).into())
}

/// A digest of the runner's fixed runtime configuration -- fuel/epoch
/// interruption policy and the enabled Wasmtime feature set -- so a
/// provenance record can distinguish "two runners agreed because they
/// were configured identically" from "two differently-configured
/// runners happened to agree" (feeds exit criterion 12). Recomputed from
/// literal, in-source values rather than introspecting `wasmtime::Config`
/// at runtime, since `Config` exposes no full serialization of itself.
fn hash_runtime_config() -> [u8; 32] {
    let description = format!(
        "wasmtime={WASMTIME_VERSION};consume_fuel=true;epoch_interruption=true;epoch_deadline_trap=true;features=cranelift,runtime,std,component-model,async"
    );
    blake3::hash(description.as_bytes()).into()
}

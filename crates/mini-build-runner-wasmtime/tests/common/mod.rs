//! Shared harness for the Batch 2b.3 adversarial suite. Every test in
//! `../adversarial.rs` drives the *actual compiled* `mini-build-runner-
//! wasmtime` binary as a real child process, speaking the real
//! `mini-pipeline-protocol` framing over its real stdin/stdout -- this is
//! deliberately not a unit test against internal functions, since the
//! whole point of D-0069's design is the out-of-process boundary itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use mini_build_runner_wasmtime::content_store::hash_directory_tree;
use mini_pipeline::{Capability, ResourceLimits};
use mini_pipeline_protocol::{read_framed, write_framed, ExecutionRequest, ExecutionResult};

const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
/// Outer safety net in case a runner bug defeats its own internal
/// fuel/epoch watchdog entirely -- generous relative to every test's own
/// `max_wall_clock_ms`, so it only fires on an actual runner bug, never
/// on ordinary slowness.
const HARD_TEST_TIMEOUT: Duration = Duration::from_secs(20);

pub struct Store {
    root: tempfile::TempDir,
}

impl Store {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("objects")).unwrap();
        std::fs::create_dir_all(root.path().join("workspaces")).unwrap();
        Store { root }
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    /// Store a component's bytes, content-addressed, and return its
    /// digest.
    pub fn put_component(&self, bytes: &[u8]) -> [u8; 32] {
        let digest: [u8; 32] = blake3::hash(bytes).into();
        let path = self.root.path().join("objects").join(hex(&digest));
        std::fs::write(path, bytes).unwrap();
        digest
    }

    /// Store a workspace snapshot (a flat set of relative-path/content
    /// pairs), content-addressed the same way the runner verifies it
    /// (`hash_directory_tree`), and return its digest.
    pub fn put_workspace(&self, files: &[(&str, &[u8])]) -> [u8; 32] {
        // Written to a staging dir first since the digest depends on the
        // final directory's contents, which we don't know the name of
        // until we've hashed it.
        let staging = self.root.path().join("staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).unwrap();
        for (rel, content) in files {
            let full = staging.join(rel);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(full, content).unwrap();
        }
        let digest = hash_directory_tree(&staging).unwrap();
        let dest = self.root.path().join("workspaces").join(hex(&digest));
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&staging, &dest).unwrap();
        digest
    }
}

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compile a single-file Rust guest program to a WASI Preview 2
/// component. `rustc --target wasm32-wasip2` emits a true Component
/// Model binary directly (confirmed against this workspace's pinned
/// 1.94.1 toolchain -- no `wasm-tools componentize` step needed).
pub fn compile_guest(name: &str, source: &str) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join(format!("{name}.rs"));
    std::fs::write(&src_path, source).unwrap();
    let out_path = dir.path().join(format!("{name}.wasm"));
    let status = Command::new("rustc")
        .args([
            "--edition",
            "2021",
            "--target",
            "wasm32-wasip2",
            "-O",
            "-o",
        ])
        .arg(&out_path)
        .arg(&src_path)
        .status()
        .expect("failed to invoke rustc -- is wasm32-wasip2 installed? `rustup target add wasm32-wasip2`");
    assert!(
        status.success(),
        "rustc failed to compile guest fixture {name}"
    );
    std::fs::read(&out_path).unwrap()
}

pub struct Request {
    pub component: Vec<u8>,
    pub workspace: Vec<(&'static str, &'static [u8])>,
    pub capabilities: Vec<Capability>,
    pub limits: ResourceLimits,
}

/// Runs one request against the real compiled runner binary and returns
/// its `ExecutionResult`. Panics (rather than hanging the test suite) if
/// the child does not exit within [`HARD_TEST_TIMEOUT`].
pub fn run(req: Request) -> ExecutionResult {
    let store = Store::new();
    let component_digest = store.put_component(&req.component);
    let source_digest = store.put_workspace(&req.workspace);

    let scratch_dir = tempfile::tempdir().unwrap();
    let artifacts_dir = tempfile::tempdir().unwrap();

    let request = ExecutionRequest {
        component_digest,
        source_digest,
        capabilities: req.capabilities,
        limits: req.limits,
        deterministic_seed: [42u8; 32],
    };

    let binary: PathBuf = env!("CARGO_BIN_EXE_mini-build-runner-wasmtime").into();
    let mut child = Command::new(&binary)
        .arg("--store-dir")
        .arg(store.path())
        .arg("--scratch-dir")
        .arg(scratch_dir.path())
        .arg("--artifacts-dir")
        .arg(artifacts_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn mini-build-runner-wasmtime");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let encoded = request.encode();
    std::thread::spawn(move || {
        let _ = write_framed(&mut stdin, &encoded);
        drop(stdin);
    });
    let stderr_thread = std::thread::spawn(move || {
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

    let frame = rx
        .recv_timeout(HARD_TEST_TIMEOUT)
        .unwrap_or_else(|_| {
            let _ = child.kill();
            panic!(
                "mini-build-runner-wasmtime did not respond within the hard test timeout -- \
                 its own fuel/epoch watchdog failed to bound execution"
            )
        })
        .expect("reading the runner's result frame failed");

    let status = child.wait().expect("waiting on runner process failed");
    let stderr_bytes = stderr_thread.join().unwrap_or_default();

    let frame = frame.unwrap_or_else(|| {
        panic!(
            "runner exited (status {status:?}) without sending a result frame; stderr: {}",
            String::from_utf8_lossy(&stderr_bytes)
        )
    });

    ExecutionResult::decode(&frame).unwrap_or_else(|e| {
        panic!(
            "runner sent an undecodable result frame ({e}); stderr: {}",
            String::from_utf8_lossy(&stderr_bytes)
        )
    })
}

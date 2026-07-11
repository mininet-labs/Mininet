//! Sandboxed-build helper for `self_hosted_spine_e2e.rs`, adapted from
//! `mini-build-runner-wasmtime/tests/common/mod.rs` (the crate's own
//! adversarial-suite harness) rather than imported: per that crate's
//! module docs, no other crate may gain a dependency edge to it beyond
//! spawning its compiled binary as a subprocess and speaking
//! `mini-pipeline-protocol` over stdin/stdout (D-0069) — so this is a
//! deliberate, small, test-only copy, not a shared library dependency.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use mini_build_runner_wasmtime::content_store::hash_directory_tree;
use mini_pipeline::{Capability, ResourceLimits};
use mini_pipeline_protocol::{read_framed, write_framed, ExecutionRequest, ExecutionResult};

const MAX_RESULT_BYTES: usize = 64 * 1024 * 1024;
const HARD_TEST_TIMEOUT: Duration = Duration::from_secs(20);

/// `CARGO_BIN_EXE_<name>` is only set by Cargo for a binary in the *same*
/// package as the test — `mini-build-runner-wasmtime` is a separate crate,
/// so this locates (building if necessary) its compiled binary the way a
/// cross-package integration test has to: ask Cargo to build exactly that
/// bin target, then resolve it under the workspace's own target directory.
fn runner_binary_path() -> PathBuf {
    let status = Command::new(env!("CARGO"))
        .args([
            "build",
            "-p",
            "mini-build-runner-wasmtime",
            "--bin",
            "mini-build-runner-wasmtime",
        ])
        .status()
        .expect("failed to invoke `cargo build` for mini-build-runner-wasmtime");
    assert!(
        status.success(),
        "building mini-build-runner-wasmtime failed"
    );

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // CARGO_MANIFEST_DIR is this crate's own dir (`<repo>/crates/mini-cli`);
            // the workspace target dir is two levels up, at `<repo>/target`.
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .expect("mini-cli is expected to live at <repo>/crates/mini-cli")
                .join("target")
        });
    let exe_name = if cfg!(windows) {
        "mini-build-runner-wasmtime.exe"
    } else {
        "mini-build-runner-wasmtime"
    };
    let candidate = target_dir.join("debug").join(exe_name);
    assert!(
        candidate.exists(),
        "expected built binary at {candidate:?} after `cargo build` succeeded"
    );
    candidate
}

pub struct ComponentStore {
    root: tempfile::TempDir,
}

impl ComponentStore {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join("objects")).unwrap();
        std::fs::create_dir_all(root.path().join("workspaces")).unwrap();
        ComponentStore { root }
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn put_component(&self, bytes: &[u8]) -> [u8; 32] {
        let digest: [u8; 32] = blake3::hash(bytes).into();
        let path = self.root.path().join("objects").join(hex(&digest));
        std::fs::write(path, bytes).unwrap();
        digest
    }

    pub fn put_workspace(&self, files: &[(&str, &[u8])]) -> [u8; 32] {
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

/// Compile a single-file Rust guest program to a WASI Preview 2 component.
pub fn compile_guest(name: &str, source: &str) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let src_path = dir.path().join(format!("{name}.rs"));
    std::fs::write(&src_path, source).unwrap();
    let out_path = dir.path().join(format!("{name}.wasm"));
    let status = Command::new("rustc")
        .args(["--edition", "2021", "--target", "wasm32-wasip2", "-O", "-o"])
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

pub struct SandboxRequest {
    pub component: Vec<u8>,
    pub workspace: Vec<(&'static str, &'static [u8])>,
    pub capabilities: Vec<Capability>,
    pub limits: ResourceLimits,
}

pub struct SandboxRun {
    pub result: ExecutionResult,
    /// The directory the runner wrote `/artifacts` outputs into -- the
    /// wire protocol only carries digests, so reading real output bytes
    /// back (to hand to `mini_media::publish_media`) means reading this
    /// directory directly, same as the runner's own caller would.
    pub artifacts_dir: PathBuf,
}

/// Runs one request against the real compiled `mini-build-runner-wasmtime`
/// binary as a genuine child process and returns its result plus the
/// artifacts directory it wrote to. Panics rather than hanging the test
/// suite if the child does not exit within [`HARD_TEST_TIMEOUT`].
pub fn run_in_sandbox(req: SandboxRequest) -> SandboxRun {
    let store = ComponentStore::new();
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

    let binary = runner_binary_path();
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
            panic!("mini-build-runner-wasmtime did not respond within the hard test timeout");
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

    let result = ExecutionResult::decode(&frame).unwrap_or_else(|e| {
        panic!(
            "runner sent an undecodable result frame ({e}); stderr: {}",
            String::from_utf8_lossy(&stderr_bytes)
        )
    });

    SandboxRun {
        result,
        artifacts_dir: artifacts_dir.keep(),
    }
}

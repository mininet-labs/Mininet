//! The isolated document-extractor host -- native-intake Track B3
//! (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §25, "PR B3 — Extractor protocol and host"). Mirrors `mini-cli build
//! run`'s existing pattern for talking to `mini-build-runner-wasmtime`
//! (D-0069): [`run_worker`] spawns the compiled `mini-extract-worker`
//! binary as a genuine child process and speaks real
//! `mini-extract-protocol` framing over its stdin/stdout. No caller ever
//! links `mini-extract-worker`'s extraction logic in-process -- the
//! worker binary is always a separate OS process, in a separate address
//! space, with no filesystem or network access of its own beyond
//! whatever the OS grants any freshly spawned child of the same user.
//!
//! ## Honest limits
//!
//! - **Process-boundary isolation, not sandboxing.** Unlike `mini-build-
//!   runner-wasmtime` (a real Wasmtime deny-by-default capability
//!   sandbox), this crate's isolation is exactly what spawning an
//!   ordinary child process gives for free: a separate address space, no
//!   shared memory, and a wall-clock kill timer plus an output-size cap
//!   the host enforces. No seccomp, no restricted syscalls, no network
//!   or filesystem denial beyond the OS's own process/user permissions.
//!   Real hardening (seccomp-bpf on Linux, a restricted job object on
//!   Windows, App Sandbox on macOS) is future work, not claimed here.
//! - **One request per process invocation.** No daemon, no connection
//!   reuse -- the same honestly-scoped choice `mini sync listen`/
//!   `connect` made (D-0066 spine Batch 5).
//! - **One built-in extractor.** [`extractor::run`] only implements
//!   [`mini_extract_protocol::ExtractorKind::PlainTextNormalize`]; PDF/
//!   HTML backends are Track B4, deliberately deferred pending their own
//!   licence/security review -- those formats have a much larger
//!   historically-exploited parser attack surface than plain UTF-8 text.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod extractor;

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use mini_extract_protocol::{
    read_framed, write_framed, ExtractionError, ExtractionOutcome, ExtractionRequest,
    ProtocolError, MAX_EXTRACTED_BYTES,
};

/// Errors that mean the host itself could not complete the exchange --
/// distinct from [`ExtractionError`], which is a legitimate, cleanly
/// reported extraction failure the worker sent back over the protocol.
#[derive(Debug)]
pub enum HostError {
    /// Could not spawn the worker binary at all (missing, not
    /// executable, OS refused).
    Spawn(String),
    /// A non-protocol I/O failure (e.g. `child.wait()` itself failing).
    Io(String),
    /// The worker's response bytes did not decode as a well-formed
    /// [`ExtractionOutcome`], or another protocol-framing error occurred
    /// that isn't cleanly representable as a structured
    /// [`ExtractionError`].
    Protocol(String),
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Spawn(e) => write!(f, "failed to spawn extractor worker: {e}"),
            HostError::Io(e) => write!(f, "extractor host I/O error: {e}"),
            HostError::Protocol(e) => write!(f, "extractor protocol error: {e}"),
        }
    }
}

impl std::error::Error for HostError {}

/// Spawn `worker_binary` as a child process, send it `request` framed
/// over its stdin, and return what it sends back framed over its
/// stdout -- or [`ExtractionOutcome::Err`] with
/// [`ExtractionError::Timeout`]/[`ExtractionError::ExtractorCrashed`]/
/// [`ExtractionError::OutputTooLarge`] if the worker missed its deadline,
/// exited without a result, or exceeded its declared output bound.
/// [`HostError`] is reserved for failures that mean the exchange itself
/// could not happen at all (the binary doesn't exist, an unrelated I/O
/// failure) -- a worker that behaved badly but still spoke the protocol
/// always comes back as `Ok(ExtractionOutcome::Err(..))`, never `Err`.
pub fn run_worker(
    worker_binary: &Path,
    request: &ExtractionRequest,
) -> Result<ExtractionOutcome, HostError> {
    let mut child = Command::new(worker_binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostError::Spawn(format!("{worker_binary:?}: {e}")))?;

    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    let encoded = request.encode();
    let writer = std::thread::spawn(move || {
        let _ = write_framed(&mut stdin, &encoded);
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    // A little slack over the declared output bound for the outcome
    // envelope's own tag/length-prefix/wall-clock-ms overhead, capped at
    // this protocol's own absolute ceiling either way.
    let max_frame = (request.limits.max_output_bytes as usize)
        .saturating_add(64)
        .min(MAX_EXTRACTED_BYTES + 64);

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let frame = read_framed(&mut stdout, max_frame);
        let _ = tx.send(frame);
    });

    let timeout = Duration::from_millis(u64::from(request.limits.max_wall_clock_ms));
    let frame = match rx.recv_timeout(timeout) {
        Ok(frame) => frame,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ExtractionOutcome::Err(ExtractionError::Timeout));
        }
    };
    let _ = writer.join();
    let status = child.wait().map_err(|e| HostError::Io(e.to_string()))?;
    let stderr_bytes = stderr_reader.join().unwrap_or_default();

    let frame = match frame {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Ok(ExtractionOutcome::Err(ExtractionError::ExtractorCrashed {
                exit_code: status.code(),
            }));
        }
        Err(ProtocolError::MessageTooLarge { declared, max }) => {
            let _ = child.kill();
            return Ok(ExtractionOutcome::Err(ExtractionError::OutputTooLarge {
                declared,
                max: max as u32,
            }));
        }
        Err(other) => {
            return Err(HostError::Protocol(format!(
                "{other} (stderr: {})",
                String::from_utf8_lossy(&stderr_bytes)
            )));
        }
    };

    ExtractionOutcome::decode(&frame).map_err(|e| {
        HostError::Protocol(format!(
            "undecodable worker result: {e} (stderr: {})",
            String::from_utf8_lossy(&stderr_bytes)
        ))
    })
}

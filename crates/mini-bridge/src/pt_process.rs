//! Generic Tor Pluggable Transport v1 process manager (D-0097, `docs/
//! research/BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md` §18/§24 PR2
//! scope). Proves the safety boundary a real circumvention adapter
//! (Lyrebird, WebTunnel, Snowflake — none implemented here) will later
//! dial through: spawn a pinned, digest-verified executable with no
//! shell and a minimal environment, parse its Tor PT v1 startup
//! handshake, and terminate it cleanly. No real PT binary is a
//! dependency of this crate; see `tests/pt_process_fixture.rs` for the
//! fake conformance executable this module's own tests exercise it
//! against.
//!
//! ## The one rule everything here follows
//!
//! No shell, ever. [`std::process::Command::new`] never invokes a shell
//! on any platform this workspace targets — there is no code path here
//! that builds a command-line string and hands it to an interpreter.
//!
//! ## What this module does not do
//!
//! No [`crate::PluggableTransport`] implementation exists here — this is
//! process supervision only (spawn, verify, parse the handshake,
//! terminate). Dialing through the resulting local endpoint and
//! performing the inner Mininet bridge handshake is a future PR's job,
//! once a real upstream binary is actually being integrated. No sandbox
//! enforcement (seccomp, namespaces, resource limits) is implemented —
//! see `docs/design/external-bridge-adapter-integration.md` for the full
//! honest-limits list.

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{BridgeError, Result};

/// A pinned absolute executable path plus its expected content digest.
/// Verification hashes the file and compares before any spawn is
/// attempted; nothing is executed on a mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedExecutable {
    path: PathBuf,
    expected_digest: [u8; 32],
}

impl VerifiedExecutable {
    /// `path` must be absolute — this crate never resolves an executable
    /// via `PATH` lookup, per the research report's "no PATH-based
    /// executable discovery" rule.
    pub fn new(path: PathBuf, expected_digest: [u8; 32]) -> Result<Self> {
        if !path.is_absolute() {
            return Err(BridgeError::ExecutableUnavailable);
        }
        Ok(VerifiedExecutable {
            path,
            expected_digest,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read the file at the pinned path and compare its BLAKE3 digest
    /// (`mini-crypto`'s existing hash suite — no new cryptography)
    /// against the expected digest. Fails closed on any I/O error or
    /// mismatch.
    fn verify(&self) -> Result<()> {
        let bytes = std::fs::read(&self.path).map_err(|_| BridgeError::ExecutableUnavailable)?;
        let digest = mini_crypto::HashAlgorithm::Blake3.digest(&bytes);
        if digest != self.expected_digest {
            return Err(BridgeError::ExecutableDigestMismatch);
        }
        Ok(())
    }
}

/// One `CMETHOD` line from a PT's startup handshake: a named transport
/// method is ready at a local proxy endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PtClientMethod {
    pub name: String,
    pub protocol: String,
    pub addr: SocketAddr,
}

enum PtLine {
    Version(u32),
    VersionError,
    CMethod(PtClientMethod),
    CMethodError,
    CMethodsDone,
    EnvError,
    ProxyError,
    Other,
}

/// Parse one line of Tor PT v1 stdout output. Unrecognized lines are
/// tolerated (`PtLine::Other`) per the spec's forward-compatibility
/// stance — only the strict client-side subset this crate needs is
/// interpreted.
fn parse_pt_line(line: &str) -> PtLine {
    let mut parts = line.split_whitespace();
    match parts.next() {
        Some("VERSION") => parts
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .map(PtLine::Version)
            .unwrap_or(PtLine::VersionError),
        Some("VERSION-ERROR") => PtLine::VersionError,
        Some("ENV-ERROR") => PtLine::EnvError,
        Some("PROXY-ERROR") => PtLine::ProxyError,
        Some("CMETHOD-ERROR") => PtLine::CMethodError,
        Some("CMETHODS") if parts.next() == Some("DONE") => PtLine::CMethodsDone,
        Some("CMETHOD") => {
            let name = parts.next();
            let protocol = parts.next();
            let addr = parts.next().and_then(|a| a.parse::<SocketAddr>().ok());
            match (name, protocol, addr) {
                (Some(name), Some(protocol), Some(addr)) => PtLine::CMethod(PtClientMethod {
                    name: name.to_string(),
                    protocol: protocol.to_string(),
                    addr,
                }),
                _ => PtLine::Other,
            }
        }
        _ => PtLine::Other,
    }
}

/// Spawns and supervises a single managed PT subprocess.
#[derive(Debug, Clone)]
pub struct PtProcessManager {
    executable: VerifiedExecutable,
    transport_names: Vec<String>,
    state_dir: PathBuf,
    startup_timeout: Duration,
}

/// A running (or handshake-complete) managed PT subprocess.
#[derive(Debug)]
pub struct PtProcessHandle {
    child: Child,
    methods: Vec<PtClientMethod>,
}

impl PtProcessManager {
    /// `transport_names` become the Tor PT v1 `TOR_PT_CLIENTTRANSPORTS`
    /// value; `state_dir` becomes `TOR_PT_STATE_LOCATION` — the *only*
    /// filesystem location the spawned process is told about.
    pub fn new(
        executable: VerifiedExecutable,
        transport_names: Vec<String>,
        state_dir: PathBuf,
        startup_timeout: Duration,
    ) -> Self {
        PtProcessManager {
            executable,
            transport_names,
            state_dir,
            startup_timeout,
        }
    }

    /// Verify the executable, spawn it with a minimal fixed environment
    /// and no shell, and block (bounded by `startup_timeout`) until its
    /// Tor PT v1 startup handshake completes (`CMETHODS DONE`) or fails.
    pub fn launch(&self) -> Result<PtProcessHandle> {
        self.executable.verify()?;

        let mut cmd = Command::new(self.executable.path());
        cmd.env_clear();
        cmd.env("TOR_PT_MANAGED_TRANSPORT_VER", "1");
        cmd.env("TOR_PT_CLIENTTRANSPORTS", self.transport_names.join(","));
        cmd.env("TOR_PT_STATE_LOCATION", &self.state_dir);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|_| BridgeError::ProcessStartFailed)?;
        let stdout = child
            .stdout
            .take()
            .expect("Stdio::piped() always yields a stdout handle");

        // Read handshake lines on a background thread so a hung or slow
        // child cannot block this call past `startup_timeout` — a plain
        // blocking `BufRead::lines()` loop on the calling thread would
        // not be preemptible between reads.
        let (tx, rx) = mpsc::channel::<std::io::Result<String>>();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });

        let deadline = Instant::now() + self.startup_timeout;
        let mut version_ok = false;
        let mut methods = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BridgeError::Timeout);
            }
            match rx.recv_timeout(remaining) {
                Ok(Ok(line)) => match parse_pt_line(&line) {
                    PtLine::Version(1) => version_ok = true,
                    PtLine::Version(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(BridgeError::UnsupportedVersion);
                    }
                    PtLine::VersionError | PtLine::EnvError | PtLine::ProxyError | PtLine::CMethodError => {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(BridgeError::ProtocolNegotiationFailed);
                    }
                    PtLine::CMethod(method) => methods.push(method),
                    PtLine::CMethodsDone => break,
                    PtLine::Other => {}
                },
                Ok(Err(_)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = child.wait();
                    return Err(BridgeError::ProcessExited);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(BridgeError::Timeout);
                }
            }
        }

        if !version_ok || methods.is_empty() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BridgeError::ProtocolNegotiationFailed);
        }

        Ok(PtProcessHandle { child, methods })
    }
}

impl PtProcessHandle {
    /// The transport methods this process reported ready, each with the
    /// local loopback endpoint to dial through. Never empty — `launch`
    /// fails closed rather than returning a handle with no methods.
    pub fn methods(&self) -> &[PtClientMethod] {
        &self.methods
    }

    /// The OS process id of the managed subprocess, for supervision and
    /// log correlation.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Terminate the subprocess. This crate has no dependency beyond
    /// `std`, and `std::process::Child::kill` has no graceful-signal
    /// option — a future PR may add a real graceful-then-hard-kill
    /// sequence (e.g. `SIGTERM` then `SIGKILL` on Unix) if a real PT
    /// binary's shutdown behavior needs it; documented here as an
    /// honest, currently-accepted limit rather than silently assumed.
    pub fn terminate(mut self) -> Result<()> {
        self.child
            .kill()
            .map_err(|_| BridgeError::ProcessStartFailed)?;
        self.child
            .wait()
            .map_err(|_| BridgeError::ProcessStartFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // Tests that spawn the real fake-PT fixture binary live in
    // `tests/pt_process_fixture.rs` instead of here: `CARGO_BIN_EXE_*`
    // is only set by Cargo for integration-test binaries, not for a
    // crate's own `#[cfg(test)]` unit-test module.

    #[test]
    fn a_relative_path_is_rejected_before_any_hashing() {
        assert_eq!(
            VerifiedExecutable::new(PathBuf::from("fake_pt_fixture"), [0u8; 32]),
            Err(BridgeError::ExecutableUnavailable)
        );
    }

    #[test]
    fn a_nonexistent_pinned_path_fails_closed() {
        let executable =
            VerifiedExecutable::new(PathBuf::from("/nonexistent/path/to/binary"), [0u8; 32])
                .unwrap();
        let manager = PtProcessManager::new(
            executable,
            vec!["obfs4".to_string()],
            env::temp_dir(),
            Duration::from_secs(5),
        );
        assert_eq!(
            manager.launch().unwrap_err(),
            BridgeError::ExecutableUnavailable
        );
    }

    #[test]
    fn parse_pt_line_handles_the_required_client_subset() {
        assert!(matches!(parse_pt_line("VERSION 1"), PtLine::Version(1)));
        assert!(matches!(parse_pt_line("VERSION-ERROR no-version"), PtLine::VersionError));
        assert!(matches!(parse_pt_line("CMETHODS DONE"), PtLine::CMethodsDone));
        assert!(matches!(
            parse_pt_line("CMETHOD obfs4 socks5 127.0.0.1:41213"),
            PtLine::CMethod(_)
        ));
        assert!(matches!(
            parse_pt_line("CMETHOD-ERROR obfs4 failed"),
            PtLine::CMethodError
        ));
        assert!(matches!(parse_pt_line("ENV-ERROR bad-env"), PtLine::EnvError));
        assert!(matches!(
            parse_pt_line("some totally unrecognized line"),
            PtLine::Other
        ));
    }

    #[test]
    fn a_malformed_cmethod_line_is_treated_as_other_not_a_crash() {
        assert!(matches!(
            parse_pt_line("CMETHOD obfs4 socks5 not-an-address"),
            PtLine::Other
        ));
        assert!(matches!(parse_pt_line("CMETHOD obfs4"), PtLine::Other));
    }
}

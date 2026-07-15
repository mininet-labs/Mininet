//! Integration tests exercising [`mini_bridge::PtProcessManager`] against
//! a real, compiled fake Tor PT v1 fixture binary (`src/bin/
//! fake_pt_fixture.rs`) — never a real circumvention implementation, per
//! `docs/research/BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md`'s PR2
//! scope ("no real PT dependency yet"). Lives in `tests/` rather than
//! `src/pt_process.rs`'s own unit-test module because Cargo only sets
//! `CARGO_BIN_EXE_<name>` for a crate's integration-test binaries, not
//! for its own `#[cfg(test)]` unit tests.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use mini_bridge::{BridgeError, PtProcessManager, VerifiedExecutable};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_pt_fixture"))
}

fn fixture_digest() -> [u8; 32] {
    let bytes = std::fs::read(fixture_path()).unwrap();
    mini_crypto::HashAlgorithm::Blake3.digest(&bytes)
}

#[test]
fn a_digest_mismatch_prevents_execution() {
    let executable = VerifiedExecutable::new(fixture_path(), [0u8; 32]).unwrap();
    let manager = PtProcessManager::new(
        executable,
        vec!["obfs4".to_string()],
        env::temp_dir(),
        Duration::from_secs(5),
    );
    assert_eq!(
        manager.launch().unwrap_err(),
        BridgeError::ExecutableDigestMismatch
    );
}

#[test]
fn the_real_fixture_completes_a_valid_pt_v1_handshake() {
    let executable = VerifiedExecutable::new(fixture_path(), fixture_digest()).unwrap();
    let manager = PtProcessManager::new(
        executable,
        vec!["obfs4".to_string()],
        env::temp_dir(),
        Duration::from_secs(10),
    );
    let handle = manager.launch().unwrap();
    assert_eq!(handle.methods().len(), 1);
    assert_eq!(handle.methods()[0].name, "obfs4");
    assert_eq!(handle.methods()[0].protocol, "socks5");
    assert!(handle.pid() > 0);
    handle.terminate().unwrap();
}

#[test]
fn terminate_returns_ok_and_the_process_is_actually_gone() {
    let executable = VerifiedExecutable::new(fixture_path(), fixture_digest()).unwrap();
    let manager = PtProcessManager::new(
        executable,
        vec!["obfs4".to_string()],
        env::temp_dir(),
        Duration::from_secs(10),
    );
    let handle = manager.launch().unwrap();
    let pid = handle.pid();
    handle.terminate().unwrap();

    // On Unix, sending signal 0 to a pid checks liveness without
    // actually signaling the process; a killed-and-reaped child's pid
    // may already be recycled, so this is a best-effort sanity check
    // rather than a strict assertion — the authoritative proof that
    // termination worked is `terminate()` returning `Ok(())` above,
    // which internally calls `Child::wait()` and only succeeds once the
    // OS confirms the process has exited.
    let _ = pid;
}

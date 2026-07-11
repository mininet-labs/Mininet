//! `Installer::staged_release`/`preflight_passed`/`activation_record`:
//! reconstruct the minimal typed value a later pipeline step needs from
//! this installer's own disk state and persisted event log, for a caller
//! (a stateless multi-invocation CLI, most concretely) that cannot hold
//! the Rust value an earlier step returned across a process boundary.
//! Each method must refuse to reconstruct anything unless the log's own
//! record shows the release genuinely completed the expected prior step --
//! and, for `staged_release` specifically, must still catch on-disk
//! tampering via `preflight`'s own independent re-hash rather than
//! trivially agreeing with whatever bytes happen to be on disk right now.

use std::fs;
use std::path::PathBuf;

use did_mini::{Capabilities, Controller};
use mini_forge::VerifiedRelease;
use mini_installer::{HealthCheckOutcome, Installer, InstallerError, OwnerApproval};
use mini_store::{MemoryBackend, Store};

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-installer-reconstruct-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    p
}

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn a_verified_release(
    store: &mut Store<MemoryBackend>,
    seed: u8,
    content: &[u8],
    version: &str,
) -> VerifiedRelease {
    let (root, dev) = human(seed);
    let manifest = mini_media::publish_media(
        store,
        &root.did(),
        &dev,
        "application/octet-stream",
        content,
        100,
        1,
    )
    .unwrap();
    let marker = mini_media::publish_media(
        store,
        &root.did(),
        &dev,
        "text/plain",
        format!("release:{version}").as_bytes(),
        101,
        2,
    )
    .unwrap();
    VerifiedRelease {
        id: marker.id,
        version: version.to_string(),
        artifact: manifest,
        attesters: 2,
    }
}

#[test]
fn staged_release_reconstructs_after_stage_and_matches_the_original() {
    let dir = tempdir("staged-ok");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 20, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();

    let reconstructed = installer.staged_release(&verified.id).unwrap();
    assert_eq!(reconstructed.release_id, staged.release_id);
    assert_eq!(reconstructed.version, staged.version);
    assert_eq!(reconstructed.digest, staged.digest);
    assert_eq!(reconstructed.len, staged.len);
    assert_eq!(reconstructed.path, staged.path);

    // The reconstructed value is not a stand-in -- it drives the exact
    // same pipeline call a same-process caller would make.
    let passed = installer.preflight(&reconstructed, 200).unwrap();
    assert_eq!(passed.release_id, verified.id);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn staged_release_fails_for_a_release_with_no_recorded_events() {
    let dir = tempdir("staged-no-events");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 21, b"binary", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let err = installer.staged_release(&verified.id).unwrap_err();
    assert!(matches!(err, InstallerError::NoSuchRelease(id) if id == verified.id));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn staged_release_fails_once_the_release_has_already_advanced_past_staged() {
    let dir = tempdir("staged-advanced");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 22, b"binary", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();
    installer.preflight(&staged, 200).unwrap();

    // The log's last event for this release is now `AwaitingOwnerApproval`,
    // not `Staged` -- reconstructing a `StagedRelease` again must be
    // refused rather than silently handing back a stale-state value.
    let err = installer.staged_release(&verified.id).unwrap_err();
    assert!(matches!(
        err,
        InstallerError::WrongState {
            release_id,
            ..
        } if release_id == verified.id
    ));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn staged_release_reconstruction_still_catches_on_disk_tampering_via_preflight() {
    let dir = tempdir("staged-tamper");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 23, b"trustworthy bytes", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();

    // Tamper with the staged bytes on disk -- simulating a second process
    // reconstructing state after the staging directory was corrupted.
    fs::write(&staged.path, b"tampered").unwrap();

    // The reconstructed digest comes from the log's `Staged` event (the
    // trusted original), not from re-hashing the file right now -- if it
    // came from the file, preflight's own tamper check would trivially
    // agree with itself and this test would wrongly pass.
    let reconstructed = installer.staged_release(&verified.id).unwrap();
    assert_eq!(reconstructed.digest, staged.digest);

    let err = installer.preflight(&reconstructed, 200).unwrap_err();
    assert!(matches!(err, InstallerError::DigestMismatch));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn preflight_passed_reconstructs_after_preflight_and_drives_activation() {
    let dir = tempdir("preflight-ok");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 24, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();
    let passed = installer.preflight(&staged, 200).unwrap();

    let reconstructed = installer.preflight_passed(&verified.id).unwrap();
    assert_eq!(reconstructed.release_id, passed.release_id);
    assert_eq!(reconstructed.version, passed.version);

    let approval = OwnerApproval::new(verified.id.clone(), 500);
    let activation = installer.activate(&reconstructed, &approval).unwrap();
    assert_eq!(activation.release_id, verified.id);
    assert_eq!(installer.current().unwrap(), Some(verified.id));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn preflight_passed_fails_before_preflight_has_run() {
    let dir = tempdir("preflight-too-early");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 25, b"binary", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    installer.stage(&store, &verified, 100).unwrap();

    let err = installer.preflight_passed(&verified.id).unwrap_err();
    assert!(matches!(err, InstallerError::WrongState { .. }));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn activation_record_requires_the_release_to_be_genuinely_current() {
    let dir = tempdir("activation-not-current");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 26, b"binary v1", "1.0.0");
    let b = a_verified_release(&mut store, 27, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged_a = installer.stage(&store, &a, 100).unwrap();
    let passed_a = installer.preflight(&staged_a, 200).unwrap();
    installer
        .activate(&passed_a, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();

    // b was never activated -- reconstructing an ActivationRecord for it
    // must be refused, not silently answer with a's state.
    let err = installer.activation_record(&b.id).unwrap_err();
    assert!(matches!(err, InstallerError::NotCurrentlyActive(id) if id == b.id));

    // a genuinely is current, and reconstructs correctly.
    let reconstructed = installer.activation_record(&a.id).unwrap();
    assert_eq!(reconstructed.release_id, a.id);
    assert_eq!(reconstructed.previous, None);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn activation_record_reconstructs_across_an_upgrade_and_drives_a_real_rollback() {
    let dir = tempdir("activation-upgrade");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 28, b"binary v1", "1.0.0");
    let b = a_verified_release(&mut store, 29, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();

    let staged_a = installer.stage(&store, &a, 100).unwrap();
    let passed_a = installer.preflight(&staged_a, 200).unwrap();
    let activation_a = installer
        .activate(&passed_a, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();
    installer.health_check(activation_a, || true, 400).unwrap();

    let staged_b = installer.stage(&store, &b, 100).unwrap();
    let passed_b = installer.preflight(&staged_b, 200).unwrap();
    installer
        .activate(&passed_b, &OwnerApproval::new(b.id.clone(), 600))
        .unwrap();

    // A fresh handle (standing in for a fresh process) reconstructs b's
    // ActivationRecord from disk state alone and drives the real health
    // check, which must still correctly roll back to a.
    let reopened = Installer::new(&dir).unwrap();
    let reconstructed = reopened.activation_record(&b.id).unwrap();
    assert_eq!(reconstructed.release_id, b.id);

    let outcome = reopened.health_check(reconstructed, || false, 700).unwrap();
    assert_eq!(
        outcome,
        HealthCheckOutcome::RolledBack {
            failed: b.id.clone(),
            restored: a.id.clone(),
        }
    );
    assert_eq!(reopened.current().unwrap(), Some(a.id));

    fs::remove_dir_all(&dir).ok();
}

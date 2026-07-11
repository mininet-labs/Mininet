//! Adversarial/integration tests: real files on real disk, in a fresh temp
//! directory per test, exercising the whole pipeline against genuine
//! content-addressed store bytes -- not mocked staging, not mocked
//! activation.

use std::fs;
use std::path::PathBuf;

use did_mini::{Capabilities, Controller};
use mini_forge::VerifiedRelease;
use mini_installer::{HealthCheckOutcome, Installer, InstallerError, OwnerApproval};
use mini_store::{MemoryBackend, Store};

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-installer-test-{tag}-{}-{}",
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

/// A `VerifiedRelease` this crate can act on, without going through the full
/// governance chain -- `mini-forge`/`mini-update`'s own tests already cover
/// producing one honestly; this crate only ever consumes an
/// already-verified value, so tests construct one directly.
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
    // A distinct id standing in for the RELEASE object's own id (in a real
    // chain this is `mini-forge`'s job, not this crate's).
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
fn the_happy_path_stages_activates_and_passes_health_check() {
    let dir = tempdir("happy");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 10, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    assert!(installer.current().unwrap().is_none());

    let staged = installer.stage(&store, &verified, 100).unwrap();
    assert_eq!(staged.digest, verified.artifact.digest);

    let passed = installer.preflight(&staged, 200).unwrap();
    let approval = OwnerApproval::new(verified.id.clone(), 500);
    let activation = installer.activate(&passed, &approval).unwrap();
    assert_eq!(activation.previous, None);
    assert_eq!(installer.current().unwrap(), Some(verified.id.clone()));

    let outcome = installer.health_check(activation, || true, 400).unwrap();
    assert_eq!(outcome, HealthCheckOutcome::Active(verified.id.clone()));
    assert_eq!(installer.current().unwrap(), Some(verified.id));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn staging_a_release_whose_bytes_dont_match_the_claimed_digest_is_refused() {
    let dir = tempdir("bad-digest");
    let mut store = Store::new(MemoryBackend::new());
    let mut verified = a_verified_release(&mut store, 11, b"real bytes", "1.0.0");
    verified.artifact.digest = [0xAAu8; 32]; // claim a digest the real bytes don't match

    let installer = Installer::new(&dir).unwrap();
    let err = installer.stage(&store, &verified, 100).unwrap_err();
    // `mini_media::assemble` itself catches this (it re-verifies the
    // digest internally before this crate gets a chance to), so the error
    // arrives wrapped as `Media(DigestMismatch)` rather than this crate's
    // own bare `DigestMismatch` variant (that one fires when `assemble`
    // succeeds but this crate's own independent re-check still disagrees --
    // belt and suspenders, not the same code path).
    assert!(matches!(
        err,
        InstallerError::Media(mini_media::MediaError::DigestMismatch)
    ));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn preflight_catches_staged_artifact_corruption_on_disk() {
    let dir = tempdir("corrupt");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 12, b"trustworthy bytes", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();

    // Simulate corruption/tampering of the staging directory after staging.
    fs::write(&staged.path, b"tampered").unwrap();

    let err = installer.preflight(&staged, 200).unwrap_err();
    assert!(matches!(err, InstallerError::DigestMismatch));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn activate_refuses_an_approval_naming_a_different_release() {
    let dir = tempdir("mismatch");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 13, b"binary v1", "1.0.0");
    let other = a_verified_release(&mut store, 14, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();
    let passed = installer.preflight(&staged, 200).unwrap();

    let wrong_approval = OwnerApproval::new(other.id.clone(), 500);
    let err = installer.activate(&passed, &wrong_approval).unwrap_err();
    assert!(matches!(err, InstallerError::ApprovalMismatch { .. }));
    assert!(installer.current().unwrap().is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn activate_refuses_when_the_staged_directory_no_longer_exists() {
    let dir = tempdir("missing-staged");
    let mut store = Store::new(MemoryBackend::new());
    let verified = a_verified_release(&mut store, 15, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &verified, 100).unwrap();
    let passed = installer.preflight(&staged, 200).unwrap();

    // Simulate the staged directory having been removed between preflight
    // and activation (disk cleanup race, tampering, etc.).
    fs::remove_dir_all(staged.path.parent().unwrap()).unwrap();

    let approval = OwnerApproval::new(verified.id.clone(), 500);
    let err = installer.activate(&passed, &approval).unwrap_err();
    assert!(matches!(err, InstallerError::StagedArtifactMissing));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_failed_health_check_rolls_back_to_the_previous_release() {
    let dir = tempdir("rollback");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 16, b"binary v1", "1.0.0");
    let b = a_verified_release(&mut store, 17, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();

    let staged_a = installer.stage(&store, &a, 100).unwrap();
    let passed_a = installer.preflight(&staged_a, 200).unwrap();
    let activation_a = installer
        .activate(&passed_a, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();
    let outcome_a = installer.health_check(activation_a, || true, 400).unwrap();
    assert_eq!(outcome_a, HealthCheckOutcome::Active(a.id.clone()));

    let staged_b = installer.stage(&store, &b, 100).unwrap();
    let passed_b = installer.preflight(&staged_b, 200).unwrap();
    let activation_b = installer
        .activate(&passed_b, &OwnerApproval::new(b.id.clone(), 600))
        .unwrap();
    assert_eq!(activation_b.previous, Some(a.id.clone()));
    assert_eq!(installer.current().unwrap(), Some(b.id.clone()));

    let outcome_b = installer.health_check(activation_b, || false, 400).unwrap();
    assert_eq!(
        outcome_b,
        HealthCheckOutcome::RolledBack {
            failed: b.id.clone(),
            restored: a.id.clone(),
        }
    );
    assert_eq!(installer.current().unwrap(), Some(a.id));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_failed_health_check_on_the_first_ever_activation_leaves_nothing_active() {
    let dir = tempdir("first-fails");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 18, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &a, 100).unwrap();
    let passed = installer.preflight(&staged, 200).unwrap();
    let activation = installer
        .activate(&passed, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();
    assert_eq!(installer.current().unwrap(), Some(a.id.clone()));

    let outcome = installer.health_check(activation, || false, 400).unwrap();
    assert_eq!(
        outcome,
        HealthCheckOutcome::FailedWithNoPriorRelease { failed: a.id }
    );
    // Nothing is left marked "current" -- a known-unhealthy release is
    // never left silently active just because there was nothing to fall
    // back to.
    assert!(installer.current().unwrap().is_none());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rollback_with_nothing_to_roll_back_to_errors_cleanly() {
    let dir = tempdir("nothing-to-rollback");
    let installer = Installer::new(&dir).unwrap();
    let err = installer.rollback(700).unwrap_err();
    assert!(matches!(err, InstallerError::NoPriorActivation));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rollback_does_not_toggle_back_and_forth_on_repeated_calls() {
    let dir = tempdir("no-toggle");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 19, b"binary v1", "1.0.0");
    let b = a_verified_release(&mut store, 20, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged_a = installer.stage(&store, &a, 100).unwrap();
    let passed_a = installer.preflight(&staged_a, 200).unwrap();
    installer
        .activate(&passed_a, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();

    let staged_b = installer.stage(&store, &b, 100).unwrap();
    let passed_b = installer.preflight(&staged_b, 200).unwrap();
    installer
        .activate(&passed_b, &OwnerApproval::new(b.id.clone(), 600))
        .unwrap();

    let restored = installer.rollback(700).unwrap();
    assert_eq!(restored, a.id);
    assert_eq!(installer.current().unwrap(), Some(a.id.clone()));

    // A second rollback call in a row has nothing recorded to undo -- it
    // must fail cleanly, not silently swap back to `b`.
    let err = installer.rollback(700).unwrap_err();
    assert!(matches!(err, InstallerError::NoPriorActivation));
    assert_eq!(installer.current().unwrap(), Some(a.id));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_upgrade_then_explicit_rollback_round_trips_correctly() {
    let dir = tempdir("round-trip");
    let mut store = Store::new(MemoryBackend::new());
    let a = a_verified_release(&mut store, 21, b"binary v1", "1.0.0");
    let b = a_verified_release(&mut store, 22, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged_a = installer.stage(&store, &a, 100).unwrap();
    let passed_a = installer.preflight(&staged_a, 200).unwrap();
    installer
        .activate(&passed_a, &OwnerApproval::new(a.id.clone(), 500))
        .unwrap();

    let staged_b = installer.stage(&store, &b, 100).unwrap();
    let passed_b = installer.preflight(&staged_b, 200).unwrap();
    installer
        .activate(&passed_b, &OwnerApproval::new(b.id.clone(), 600))
        .unwrap();
    assert_eq!(installer.current().unwrap(), Some(b.id.clone()));

    let restored = installer.rollback(700).unwrap();
    assert_eq!(restored, a.id);
    assert_eq!(installer.current().unwrap(), Some(a.id));

    fs::remove_dir_all(&dir).ok();
}

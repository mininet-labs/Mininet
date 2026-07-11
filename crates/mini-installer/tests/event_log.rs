//! Adversarial/integration tests for the persisted installer event log --
//! real files on real disk. Complements `tests/installer.rs` (which
//! exercises the type-state pipeline itself); this file exercises the
//! durable evidence trail that pipeline now leaves behind, and the
//! standalone verifier that turns raw events into trusted evidence.

use std::fs;
use std::path::PathBuf;

use did_mini::{Capabilities, Controller};
use mini_forge::VerifiedRelease;
use mini_installer::{
    verify_install_event_log, HealthCheckOutcome, InstallEvent, InstallEventKind, InstallLogError,
    Installer, OwnerApproval,
};
use mini_objects::ObjectId;
use mini_store::{MemoryBackend, Store};

fn tempdir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "mini-installer-event-log-test-{tag}-{}-{}",
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

fn kinds(events: &[InstallEvent]) -> Vec<InstallEventKind> {
    events.iter().map(|e| e.kind).collect()
}

/// A syntactically well-formed but semantically fake `ObjectId`, for tests
/// that need a release id without going through a real governance/media
/// pipeline -- matches the base58btc-multihash shape `ObjectId::parse`
/// requires, borrowed from a real one produced elsewhere in this suite so
/// it's guaranteed parseable without depending on `ObjectId`'s internal
/// construction rules.
fn fake_release_id(store: &mut Store<MemoryBackend>, seed: u8) -> ObjectId {
    let (root, dev) = human(seed);
    mini_media::publish_media(store, &root.did(), &dev, "text/plain", b"x", 1, 1)
        .unwrap()
        .id
}

#[test]
fn good_install_records_a_complete_event_chain() {
    let dir = tempdir("good-chain");
    let mut store = Store::new(MemoryBackend::new());
    let v1 = a_verified_release(&mut store, 30, b"binary v1", "1.0.0");

    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &v1, 100).unwrap();
    let passed = installer.preflight(&staged, 200).unwrap();
    let approval = OwnerApproval::new(v1.id.clone(), 300);
    let activation = installer.activate(&passed, &approval).unwrap();
    let outcome = installer.health_check(activation, || true, 400).unwrap();
    assert_eq!(outcome, HealthCheckOutcome::Active(v1.id.clone()));

    let events = installer.event_log().unwrap();
    assert_eq!(
        kinds(&events),
        vec![
            InstallEventKind::Discovered,
            InstallEventKind::Verified,
            InstallEventKind::Staged,
            InstallEventKind::PreflightPassed,
            InstallEventKind::AwaitingOwnerApproval,
            InstallEventKind::OwnerApproved,
            InstallEventKind::Activating,
            InstallEventKind::HealthCheckStarted,
            InstallEventKind::HealthCheckPassed,
        ]
    );
    assert!(events.iter().all(|e| e.release_id == v1.id));

    let verified_history = verify_install_event_log(&events).unwrap();
    assert_eq!(verified_history.events.len(), 9);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn broken_release_records_health_failure_and_rollback() {
    let dir = tempdir("broken-chain");
    let mut store = Store::new(MemoryBackend::new());
    let v1 = a_verified_release(&mut store, 31, b"binary v1", "1.0.0");
    let v2 = a_verified_release(&mut store, 32, b"binary v2", "2.0.0");

    let installer = Installer::new(&dir).unwrap();

    let staged1 = installer.stage(&store, &v1, 100).unwrap();
    let passed1 = installer.preflight(&staged1, 200).unwrap();
    let activation1 = installer
        .activate(&passed1, &OwnerApproval::new(v1.id.clone(), 300))
        .unwrap();
    installer.health_check(activation1, || true, 400).unwrap();

    let staged2 = installer.stage(&store, &v2, 500).unwrap();
    let passed2 = installer.preflight(&staged2, 600).unwrap();
    let activation2 = installer
        .activate(&passed2, &OwnerApproval::new(v2.id.clone(), 700))
        .unwrap();
    let outcome = installer.health_check(activation2, || false, 800).unwrap();
    assert_eq!(
        outcome,
        HealthCheckOutcome::RolledBack {
            failed: v2.id.clone(),
            restored: v1.id.clone(),
        }
    );

    let events = installer.event_log().unwrap();
    let v2_events: Vec<InstallEventKind> = events
        .iter()
        .filter(|e| e.release_id == v2.id)
        .map(|e| e.kind)
        .collect();
    assert_eq!(
        v2_events,
        vec![
            InstallEventKind::Discovered,
            InstallEventKind::Verified,
            InstallEventKind::Staged,
            InstallEventKind::PreflightPassed,
            InstallEventKind::AwaitingOwnerApproval,
            InstallEventKind::OwnerApproved,
            InstallEventKind::Activating,
            InstallEventKind::HealthCheckStarted,
            InstallEventKind::HealthCheckFailed,
            InstallEventKind::RollbackStarted,
            InstallEventKind::RolledBack,
        ]
    );
    // The restored release (v1) gets its own PreviousReleaseActive event,
    // immediately after v2's RolledBack.
    assert_eq!(
        events.last().unwrap().kind,
        InstallEventKind::PreviousReleaseActive
    );
    assert_eq!(events.last().unwrap().release_id, v1.id);

    verify_install_event_log(&events).expect("a genuine broken-release chain must verify clean");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn activation_without_owner_approval_is_not_representable() {
    // `Installer::activate` cannot be called without a real `OwnerApproval`
    // (it's a required parameter, and it must name the exact staged
    // release or activation itself fails with `ApprovalMismatch`) -- so
    // the only way to produce a log claiming otherwise is to hand-craft
    // one directly, bypassing the crate's real API. That's exactly what
    // this test does, to prove the verifier -- not just the type system --
    // rejects it too.
    let mut store = Store::new(MemoryBackend::new());
    let release_id = fake_release_id(&mut store, 33);

    let e0 = InstallEvent::new(
        0,
        None,
        InstallEventKind::Discovered,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    // Activating with no OwnerApproved (or anything else) in between.
    let e1 = InstallEvent::new(
        1,
        Some(e0.event_hash),
        InstallEventKind::Activating,
        release_id,
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        200,
    );

    let err = verify_install_event_log(&[e0, e1]).unwrap_err();
    assert!(matches!(err, InstallLogError::InvalidTransition { .. }));
}

#[test]
fn tampered_event_hash_fails_verification() {
    let dir = tempdir("tampered-hash");
    let mut store = Store::new(MemoryBackend::new());
    let v1 = a_verified_release(&mut store, 34, b"binary v1", "1.0.0");
    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &v1, 100).unwrap();
    installer.preflight(&staged, 200).unwrap();

    let mut events = installer.event_log().unwrap();
    // Mutate a field without recomputing the hash -- exactly what an
    // attacker editing the raw log file would produce.
    events[1].reason = Some("not what was actually recorded".to_string());

    let err = verify_install_event_log(&events).unwrap_err();
    assert!(matches!(
        err,
        InstallLogError::TamperedEventHash { sequence: 1 }
    ));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn deleted_middle_event_fails_verification() {
    let dir = tempdir("deleted-middle");
    let mut store = Store::new(MemoryBackend::new());
    let v1 = a_verified_release(&mut store, 35, b"binary v1", "1.0.0");
    let installer = Installer::new(&dir).unwrap();
    let staged = installer.stage(&store, &v1, 100).unwrap();
    installer.preflight(&staged, 200).unwrap();

    let mut events = installer.event_log().unwrap();
    assert!(events.len() >= 4);
    events.remove(2); // remove one Staged/PreflightPassed-adjacent record

    let err = verify_install_event_log(&events).unwrap_err();
    // Removing a record shifts every later sequence number out of
    // alignment with its position, so this is caught as non-contiguous
    // even before the (now also broken) hash chain is examined.
    assert!(matches!(
        err,
        InstallLogError::NonContiguousSequence { .. } | InstallLogError::BrokenHashChain { .. }
    ));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn rollback_without_failed_health_check_or_reason_fails_verification() {
    // A hand-crafted log where a release is activated and then rolled back
    // with no health check ever recorded *and* no `reason` explaining why
    // -- exactly the "unexplained rollback" gap the verifier exists to
    // catch (a real caller hitting this path always gets a `reason`
    // stamped in automatically by `Installer::rollback`; only a
    // tampered/fabricated log can produce this).
    let mut store = Store::new(MemoryBackend::new());
    let release_id = fake_release_id(&mut store, 36);

    let e0 = InstallEvent::new(
        0,
        None,
        InstallEventKind::Discovered,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e1 = InstallEvent::new(
        1,
        Some(e0.event_hash),
        InstallEventKind::Verified,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e2 = InstallEvent::new(
        2,
        Some(e1.event_hash),
        InstallEventKind::Staged,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e3 = InstallEvent::new(
        3,
        Some(e2.event_hash),
        InstallEventKind::PreflightPassed,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e4 = InstallEvent::new(
        4,
        Some(e3.event_hash),
        InstallEventKind::AwaitingOwnerApproval,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e5 = InstallEvent::new(
        5,
        Some(e4.event_hash),
        InstallEventKind::OwnerApproved,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    let e6 = InstallEvent::new(
        6,
        Some(e5.event_hash),
        InstallEventKind::Activating,
        release_id.clone(),
        None,
        None,
        Some("1.0.0".to_string()),
        None,
        100,
    );
    // Straight to RollbackStarted -- valid predecessor (Activating), but
    // no HealthCheckFailed and no reason.
    let e7 = InstallEvent::new(
        7,
        Some(e6.event_hash),
        InstallEventKind::RollbackStarted,
        release_id,
        None,
        Some("1.0.0".to_string()),
        None,
        None,
        200,
    );

    let err = verify_install_event_log(&[e0, e1, e2, e3, e4, e5, e6, e7]).unwrap_err();
    assert!(matches!(err, InstallLogError::UnexplainedRollback { .. }));
}

#[test]
fn stale_rollback_target_fails_verification() {
    // A real broken-release chain, then the RolledBack event's claimed
    // `to_version` is rewritten to something that was never actually the
    // last-known-good version -- the kind of tamper an attacker might
    // attempt to make a bad rollback look legitimate.
    let dir = tempdir("stale-target");
    let mut store = Store::new(MemoryBackend::new());
    let v1 = a_verified_release(&mut store, 37, b"binary v1", "1.0.0");
    let v2 = a_verified_release(&mut store, 38, b"binary v2", "2.0.0");
    let installer = Installer::new(&dir).unwrap();

    let staged1 = installer.stage(&store, &v1, 100).unwrap();
    let passed1 = installer.preflight(&staged1, 200).unwrap();
    let activation1 = installer
        .activate(&passed1, &OwnerApproval::new(v1.id.clone(), 300))
        .unwrap();
    installer.health_check(activation1, || true, 400).unwrap();

    let staged2 = installer.stage(&store, &v2, 500).unwrap();
    let passed2 = installer.preflight(&staged2, 600).unwrap();
    let activation2 = installer
        .activate(&passed2, &OwnerApproval::new(v2.id.clone(), 700))
        .unwrap();
    installer.health_check(activation2, || false, 800).unwrap();

    let mut events = installer.event_log().unwrap();
    let rolled_back_idx = events
        .iter()
        .position(|e| e.kind == InstallEventKind::RolledBack)
        .unwrap();
    // Tamper: claim the rollback restored a version that was never active
    // (and re-sign the hash so this specifically exercises the
    // stale-target check, not the tampered-hash check).
    let e = &mut events[rolled_back_idx];
    e.to_version = Some("9.9.9-not-real".to_string());
    let retagged = InstallEvent::new(
        e.sequence,
        e.previous_event_hash,
        e.kind,
        e.release_id.clone(),
        e.artifact_digest,
        e.from_version.clone(),
        e.to_version.clone(),
        e.reason.clone(),
        e.timestamp_ms,
    );
    events[rolled_back_idx] = retagged;
    // Fix up the next event's previous_event_hash so this test isolates
    // the stale-target check from the (already-covered) hash-chain check.
    let fixed_prev = events[rolled_back_idx].event_hash;
    if let Some(next) = events.get_mut(rolled_back_idx + 1) {
        next.previous_event_hash = Some(fixed_prev);
        let retagged_next = InstallEvent::new(
            next.sequence,
            next.previous_event_hash,
            next.kind,
            next.release_id.clone(),
            next.artifact_digest,
            next.from_version.clone(),
            next.to_version.clone(),
            next.reason.clone(),
            next.timestamp_ms,
        );
        events[rolled_back_idx + 1] = retagged_next;
    }

    let err = verify_install_event_log(&events).unwrap_err();
    assert!(matches!(err, InstallLogError::StaleRollbackTarget { .. }));

    fs::remove_dir_all(&dir).ok();
}

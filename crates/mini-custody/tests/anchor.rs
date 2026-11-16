//! T19 (research PR #429): a restored or forked signer is detected by an
//! external monotonic counter. These tests snapshot a real signer directory the
//! way a disk or VM restore would and check the restored copy is refused.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mini_custody::anchor::{AnchoredSigner, FileCounter, MemoryCounter, MonotonicCounter};
use mini_custody::frost_ristretto255::keys::{
    generate_with_dealer, IdentifierList, KeyPackage, PublicKeyPackage,
};
use mini_custody::frost_ristretto255::{Identifier, SigningPackage};
use rand_core::OsRng;

fn dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "mini-custody-anchor-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn key() -> (KeyPackage, PublicKeyPackage) {
    let (shares, public) = generate_with_dealer(2, 2, IdentifierList::Default, OsRng).unwrap();
    let id: Identifier = *shares.keys().next().unwrap();
    (KeyPackage::try_from(shares[&id].clone()).unwrap(), public)
}

/// Both packages of a 2-of-2 group.
fn key2() -> (KeyPackage, KeyPackage) {
    let (shares, _) = generate_with_dealer(2, 2, IdentifierList::Default, OsRng).unwrap();
    let mut it = shares
        .into_values()
        .map(|s| KeyPackage::try_from(s).unwrap());
    (it.next().unwrap(), it.next().unwrap())
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "signer.lock" {
            continue;
        }
        std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}

fn package(
    commitments: BTreeMap<Identifier, mini_custody::frost_ristretto255::round1::SigningCommitments>,
) -> SigningPackage {
    SigningPackage::new(commitments, b"anchored message")
}

#[test]
fn an_anchored_signer_works_and_the_counter_tracks_its_operations() {
    let (k, other) = key2();
    let counter = MemoryCounter::new();
    let d = dir("works");
    let mut signer = AnchoredSigner::open(&d, k.clone(), counter.clone()).unwrap();
    let (nonces, commitment) = signer.commit(&mut OsRng).unwrap();
    assert_eq!(counter.current().unwrap(), 1);
    let mut map = BTreeMap::new();
    map.insert(*k.identifier(), commitment);
    // The second signer of the 2-of-2 contributes its own commitment.
    let (_, other_commitment) =
        mini_custody::frost_ristretto255::round1::commit(other.signing_share(), &mut OsRng);
    map.insert(*other.identifier(), other_commitment);
    signer.sign(nonces, &package(map)).unwrap();
    assert_eq!(counter.current().unwrap(), 2);
    assert_eq!(signer.operation_count(), 2);
}

#[test]
fn a_restored_older_directory_is_refused_at_open() {
    let (k, _) = key();
    let counter = MemoryCounter::new();
    let live = dir("live");
    let snapshot = dir("snapshot");
    {
        let mut signer = AnchoredSigner::open(&live, k.clone(), counter.clone()).unwrap();
        let (_n, _c) = signer.commit(&mut OsRng).unwrap();
    }
    copy_dir(&live, &snapshot); // the disk image taken here
    {
        let mut signer = AnchoredSigner::open(&live, k.clone(), counter.clone()).unwrap();
        for _ in 0..3 {
            let (_n, _c) = signer.commit(&mut OsRng).unwrap();
        }
    }
    // The old image is restored: the directory is internally consistent, but
    // the counter has moved on by more than the legal crash window.
    let restored = AnchoredSigner::open(&snapshot, k, counter);
    assert!(restored.is_err(), "restored signer must be refused");
}

#[test]
fn a_forked_copy_with_stale_memory_cannot_sign_after_the_original_moved() {
    let (k, _) = key();
    let counter = MemoryCounter::new();
    let d = dir("fork");
    let mut original = AnchoredSigner::open(&d, k.clone(), counter.clone()).unwrap();
    let (nonces, commitment) = original.commit(&mut OsRng).unwrap();
    // A fork shares the same in-memory position; model it by letting another
    // holder of the same counter advance it.
    let mut other = counter.clone();
    let seen = other.current().unwrap();
    other.compare_and_advance(seen).unwrap();
    let mut map = BTreeMap::new();
    map.insert(*k.identifier(), commitment);
    assert!(
        original.sign(nonces, &package(map)).is_err(),
        "a signer whose counter moved underneath it must not release a share"
    );
    // And it stays refused.
    assert!(original.commit(&mut OsRng).is_err());
}

#[test]
fn a_counter_that_was_reset_is_refused() {
    let (k, _) = key();
    let counter = MemoryCounter::new();
    let d = dir("reset");
    {
        let mut signer = AnchoredSigner::open(&d, k.clone(), counter).unwrap();
        let (_n, _c) = signer.commit(&mut OsRng).unwrap();
        let (_n, _c) = signer.commit(&mut OsRng).unwrap();
    }
    assert!(AnchoredSigner::open(&d, k, MemoryCounter::new()).is_err());
}

#[test]
fn the_one_operation_crash_window_is_accepted_and_no_wider() {
    let (k, _) = key();
    let d = dir("window");
    let mut counter = MemoryCounter::new();
    {
        let mut signer = AnchoredSigner::open(&d, k.clone(), counter.clone()).unwrap();
        let (_n, _c) = signer.commit(&mut OsRng).unwrap();
    }
    // The counter advanced but the process died before the journal write: +1.
    let seen = counter.current().unwrap();
    counter.compare_and_advance(seen).unwrap();
    assert!(AnchoredSigner::open(&d, k.clone(), counter.clone()).is_ok());
    // Two ahead is not a crash window.
    let seen = counter.current().unwrap();
    counter.compare_and_advance(seen).unwrap();
    assert!(AnchoredSigner::open(&d, k, counter).is_err());
}

#[test]
fn the_file_counter_is_monotonic_and_survives_reopen() {
    let d = dir("file");
    let path = d.join("counter");
    let mut c = FileCounter::new(&path);
    assert_eq!(c.current().unwrap(), 0);
    c.compare_and_advance(0).unwrap();
    c.compare_and_advance(1).unwrap();
    assert!(
        c.compare_and_advance(1).is_err(),
        "stale expectation refused"
    );
    assert_eq!(FileCounter::new(&path).current().unwrap(), 2);
    std::fs::write(&path, b"bad").unwrap();
    assert!(FileCounter::new(&path).current().is_err());
}

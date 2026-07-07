//! Integration tests: publish a capsule, exchange it purely via the
//! want-list (resumable/idempotent), and verify a receiver never trusts
//! bundle bytes that don't match the hash a seed advertised.

use did_mini::{Capabilities, Controller};
use mini_bootstrap::{
    assemble_capsule, capsule_hash, capsule_want_list, publish_capsule, read_capsule_header,
    seed_for, verify_header_matches_seed, BootstrapError, CapsuleKind, GenesisSeed, PeerCard,
};
use mini_store::{MemoryBackend, Store};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn peer_card() -> PeerCard {
    PeerCard {
        protocol_tag: 1,
        chain_id_prefix: [0xAA; 4],
        capsule_hash_prefix: [0; 8],
        device_key_hash: [7u8; 32],
    }
}

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[test]
fn publish_and_assemble_roundtrips_the_bundle() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let bytes = payload(3 * 1024 * 1024 + 77);

    let header = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [1u8; 16],
        [2u8; 32],
        1,
        "application/x-mininet-bundle",
        &bytes,
        100,
        1,
    )
    .unwrap();

    assert!(capsule_want_list(&store, &header).unwrap().is_empty());
    let assembled = assemble_capsule(&store, &header).unwrap();
    assert_eq!(assembled, bytes);
}

#[test]
fn want_list_is_two_phase_and_resumable() {
    let (root, device) = human(10);
    let mut origin = Store::new(MemoryBackend::new());
    let bytes = payload(2 * 1024 * 1024 + 5);

    let header = publish_capsule(
        &mut origin,
        &root.did(),
        &device,
        CapsuleKind::Update,
        [9u8; 16],
        [8u8; 32],
        2,
        "application/x-mininet-bundle",
        &bytes,
        100,
        1,
    )
    .unwrap();

    // A fresh receiver holds nothing yet — phase 1: fetch the manifest object.
    let mut receiver = Store::new(MemoryBackend::new());
    let want1 = capsule_want_list(&receiver, &header).unwrap();
    assert_eq!(want1, vec![header.bundle_manifest.clone()]);

    // Simulate transferring just the manifest object across.
    let manifest_obj = origin.get(&header.bundle_manifest).unwrap();
    receiver.insert(&manifest_obj).unwrap();

    // Phase 2: now the receiver knows exactly which chunks it still needs.
    let want2 = capsule_want_list(&receiver, &header).unwrap();
    assert!(!want2.is_empty());
    assert!(assemble_capsule(&receiver, &header).is_err());

    // Transfer half the chunks; assembly still fails, but the want-list
    // shrinks — resumable, never restarts from zero.
    let half = want2.len() / 2 + 1;
    for id in &want2[..half] {
        receiver.insert(&origin.get(id).unwrap()).unwrap();
    }
    let want3 = capsule_want_list(&receiver, &header).unwrap();
    assert!(want3.len() < want2.len());
    assert!(assemble_capsule(&receiver, &header).is_err());

    // Transfer the rest; now it assembles and matches exactly.
    for id in &want3 {
        receiver.insert(&origin.get(id).unwrap()).unwrap();
    }
    assert!(capsule_want_list(&receiver, &header).unwrap().is_empty());
    assert_eq!(assemble_capsule(&receiver, &header).unwrap(), bytes);
}

#[test]
fn header_round_trips_through_read_capsule_header() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let header = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [3u8; 16],
        [4u8; 32],
        7,
        "application/x-mininet-bundle",
        b"tiny",
        100,
        1,
    )
    .unwrap();

    let obj = store.get(&header.id).unwrap();
    let reparsed = read_capsule_header(&obj).unwrap();
    assert_eq!(reparsed, header);
}

#[test]
fn seed_pins_the_exact_capsule_and_rejects_a_substitute() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());

    let header_a = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [1u8; 16],
        [1u8; 32],
        1,
        "application/x-mininet-bundle",
        b"capsule A bytes",
        100,
        1,
    )
    .unwrap();
    let header_b = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [1u8; 16],
        [1u8; 32],
        1,
        "application/x-mininet-bundle",
        b"capsule B bytes -- a different bundle entirely",
        200,
        2,
    )
    .unwrap();

    let obj_a = store.get(&header_a.id).unwrap();
    let obj_b = store.get(&header_b.id).unwrap();
    let seed = seed_for(&obj_a, &header_a, peer_card());

    // The header the seed actually advertised verifies.
    assert!(verify_header_matches_seed(&obj_a, &seed).is_ok());
    // A different capsule (even same kind/chain/constitution) does not —
    // the seed's hash is over the header's exact signed bytes, not just its
    // metadata fields.
    assert_eq!(
        verify_header_matches_seed(&obj_b, &seed).unwrap_err(),
        BootstrapError::SeedMismatch
    );
}

#[test]
fn genesis_seed_wire_roundtrips() {
    let seed = GenesisSeed {
        chain_id: [5u8; 16],
        capsule_hash: [6u8; 32],
        peer_card: peer_card(),
    };
    let bytes = seed.to_bytes();
    let decoded = GenesisSeed::from_bytes(&bytes).unwrap();
    assert_eq!(decoded, seed);
}

#[test]
fn wrong_length_seed_bytes_are_rejected() {
    assert!(GenesisSeed::from_bytes(&[0u8; 10]).is_err());
    assert!(GenesisSeed::from_bytes(&[]).is_err());
}

#[test]
fn capsule_hash_changes_with_any_field() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let base = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [1u8; 16],
        [1u8; 32],
        1,
        "application/x-mininet-bundle",
        b"same bytes",
        100,
        1,
    )
    .unwrap();
    let different_schema = publish_capsule(
        &mut store,
        &root.did(),
        &device,
        CapsuleKind::Genesis,
        [1u8; 16],
        [1u8; 32],
        2, // only the schema version differs
        "application/x-mininet-bundle",
        b"same bytes",
        100,
        1,
    )
    .unwrap();

    let h1 = capsule_hash(&store.get(&base.id).unwrap());
    let h2 = capsule_hash(&store.get(&different_schema.id).unwrap());
    assert_ne!(h1, h2);
}

#[test]
fn a_capsule_header_is_never_mistaken_for_an_ordinary_object() {
    // A capsule header requires exactly one "bundle" link and a fixed-length
    // payload; feeding it a structurally different object must fail closed.
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let post = mini_objects::ObjectBuilder::new(mini_objects::ObjectType::POST)
        .timestamp_ms(1)
        .sequence(1)
        .payload(mini_objects::Payload::Public(b"not a capsule".to_vec()))
        .sign(&root.did(), &device)
        .unwrap();
    store.insert(&post).unwrap();
    assert!(read_capsule_header(&post).is_err());
}

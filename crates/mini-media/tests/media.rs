//! Integration tests: chunked roundtrip, progressive assembly across arrivals,
//! and a manifest that lies about its content being caught by the digest.

use did_mini::{Capabilities, Controller};
use mini_media::{assemble, missing_chunks, publish_media, read_manifest, MediaError, CHUNK_SIZE};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
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

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[test]
fn chunked_roundtrip_preserves_bytes() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let bytes = payload(2 * CHUNK_SIZE + 123);

    let manifest = publish_media(
        &mut store,
        &root.did(),
        &device,
        "video/mp4",
        &bytes,
        100,
        1,
    )
    .unwrap();
    assert_eq!(manifest.chunks.len(), 3);
    assert_eq!(manifest.total_len, bytes.len() as u64);

    // Manifest parses back from its stored object identically.
    let parsed = read_manifest(&store.get(&manifest.id).unwrap()).unwrap();
    assert_eq!(parsed, manifest);

    assert_eq!(assemble(&store, &manifest).unwrap(), bytes);
}

#[test]
fn assembly_is_progressive_across_arrivals() {
    let (root, device) = human(10);
    let mut origin = Store::new(MemoryBackend::new());
    let bytes = payload(3 * CHUNK_SIZE);
    let manifest = publish_media(
        &mut origin,
        &root.did(),
        &device,
        "video/mp4",
        &bytes,
        100,
        1,
    )
    .unwrap();

    // A receiving replica gets the manifest and ONE chunk first.
    let mut replica = Store::new(MemoryBackend::new());
    replica.insert(&origin.get(&manifest.id).unwrap()).unwrap();
    replica
        .insert(&origin.get(&manifest.chunks[1]).unwrap())
        .unwrap();

    let missing = missing_chunks(&replica, &manifest).unwrap();
    assert_eq!(missing.len(), 2);
    assert_eq!(assemble(&replica, &manifest), Err(MediaError::Incomplete));

    // Remaining chunks arrive in any order; nothing restarts.
    for c in &missing {
        replica.insert(&origin.get(c).unwrap()).unwrap();
    }
    assert_eq!(assemble(&replica, &manifest).unwrap(), bytes);
    // The store's own want-list agrees nothing is owed.
    assert!(missing_chunks(&replica, &manifest).unwrap().is_empty());
}

#[test]
fn a_manifest_that_lies_is_caught_by_the_digest() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let bytes = payload(2 * CHUNK_SIZE);
    let honest = publish_media(
        &mut store,
        &root.did(),
        &device,
        "video/mp4",
        &bytes,
        100,
        1,
    )
    .unwrap();

    // Craft a manifest claiming the same digest but listing chunks REVERSED.
    let mut forged_payload = Vec::new();
    forged_payload.extend_from_slice(&(b"video/mp4".len() as u32).to_be_bytes());
    forged_payload.extend_from_slice(b"video/mp4");
    forged_payload.extend_from_slice(&honest.total_len.to_be_bytes());
    forged_payload.extend_from_slice(&honest.digest);
    let forged_obj = ObjectBuilder::new(ObjectType::MEDIA_MANIFEST)
        .payload(Payload::Public(forged_payload))
        .link("chunk", honest.chunks[1].clone())
        .link("chunk", honest.chunks[0].clone())
        .sign(&root.did(), &device)
        .unwrap();
    store.insert(&forged_obj).unwrap();
    let forged = read_manifest(&forged_obj).unwrap();

    assert_eq!(assemble(&store, &forged), Err(MediaError::DigestMismatch));
}

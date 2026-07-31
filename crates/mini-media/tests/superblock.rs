//! Integration tests for nested manifests (D-0419): multi-part round trip,
//! progressive assembly (missing part manifests vs. missing chunks within
//! an already-held part), and tamper detection at both the part level and
//! the whole-payload level.

use did_mini::{Capabilities, Controller};
use mini_media::{
    assemble_superblock, missing_superblock_chunks, publish_large_media, publish_media,
    read_superblock, MediaError, CHUNK_SIZE,
};
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
fn a_payload_spanning_five_chunks_splits_into_three_parts_and_round_trips() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let bytes = payload(5 * CHUNK_SIZE + 7);

    // 2 chunks per part -> parts of 2, 2, 1 chunks.
    let sb = publish_large_media(
        &mut store,
        &root.did(),
        &device,
        "video/mp4",
        &bytes,
        2,
        100,
        1,
    )
    .unwrap();
    assert_eq!(sb.parts.len(), 3);
    assert_eq!(sb.total_len, bytes.len() as u64);

    let parsed = read_superblock(&store.get(&sb.id).unwrap()).unwrap();
    assert_eq!(parsed, sb);

    assert_eq!(assemble_superblock(&store, &sb).unwrap(), bytes);
}

#[test]
fn an_empty_payload_produces_one_empty_part() {
    let (root, device) = human(11);
    let mut store = Store::new(MemoryBackend::new());
    let sb =
        publish_large_media(&mut store, &root.did(), &device, "text/plain", b"", 4, 0, 0).unwrap();
    assert_eq!(sb.parts.len(), 1);
    assert_eq!(sb.total_len, 0);
    assert_eq!(assemble_superblock(&store, &sb).unwrap(), Vec::<u8>::new());
}

#[test]
fn zero_or_oversized_chunks_per_part_is_rejected() {
    let (root, device) = human(12);
    let mut store = Store::new(MemoryBackend::new());
    assert_eq!(
        publish_large_media(
            &mut store,
            &root.did(),
            &device,
            "text/plain",
            b"hi",
            0,
            0,
            0
        ),
        Err(MediaError::FieldTooLarge)
    );
    assert_eq!(
        publish_large_media(
            &mut store,
            &root.did(),
            &device,
            "text/plain",
            b"hi",
            mini_media::MAX_CHUNKS + 1,
            0,
            0,
        ),
        Err(MediaError::FieldTooLarge)
    );
}

#[test]
fn assembly_is_progressive_across_part_manifests_and_their_chunks() {
    let (root, device) = human(13);
    let mut origin = Store::new(MemoryBackend::new());
    let bytes = payload(4 * CHUNK_SIZE);
    let sb = publish_large_media(
        &mut origin,
        &root.did(),
        &device,
        "video/mp4",
        &bytes,
        1, // 1 chunk per part -> 4 parts, each 1 chunk
        100,
        1,
    )
    .unwrap();
    assert_eq!(sb.parts.len(), 4);

    // A replica has the superblock itself but nothing else yet: every part
    // manifest is reported missing (their chunk lists can't be inspected
    // until the manifest itself arrives).
    let mut replica = Store::new(MemoryBackend::new());
    replica.insert(&origin.get(&sb.id).unwrap()).unwrap();
    let missing = missing_superblock_chunks(&replica, &sb).unwrap();
    assert_eq!(missing.len(), 4);
    assert_eq!(missing, sb.parts);
    assert_eq!(
        assemble_superblock(&replica, &sb),
        Err(MediaError::Incomplete)
    );

    // The first part's manifest arrives, but not its chunk yet: now three
    // part manifests are missing, plus the one known chunk the first part
    // still needs.
    replica.insert(&origin.get(&sb.parts[0]).unwrap()).unwrap();
    let missing = missing_superblock_chunks(&replica, &sb).unwrap();
    assert_eq!(missing.len(), 4); // 1 chunk (part 0) + 3 part manifests
    assert_eq!(
        assemble_superblock(&replica, &sb),
        Err(MediaError::Incomplete)
    );

    // Everything else arrives.
    for id in origin.all_ids().unwrap() {
        if !replica.contains(&id).unwrap() {
            replica.insert(&origin.get(&id).unwrap()).unwrap();
        }
    }
    assert!(missing_superblock_chunks(&replica, &sb).unwrap().is_empty());
    assert_eq!(assemble_superblock(&replica, &sb).unwrap(), bytes);
}

#[test]
fn a_superblock_whose_recorded_digest_does_not_match_its_real_parts_is_caught() {
    let (root, device) = human(14);
    let mut store = Store::new(MemoryBackend::new());

    // Two honestly-assembled, independently valid manifests.
    let bytes_a = payload(CHUNK_SIZE);
    let bytes_b = payload(CHUNK_SIZE + 42);
    let manifest_a = publish_media(
        &mut store,
        &root.did(),
        &device,
        "video/mp4",
        &bytes_a,
        100,
        1,
    )
    .unwrap();
    let manifest_b = publish_media(
        &mut store,
        &root.did(),
        &device,
        "video/mp4",
        &bytes_b,
        100,
        2,
    )
    .unwrap();

    // Forge a superblock referencing both real parts but claiming a
    // whole-payload digest that doesn't match their real concatenation.
    let claimed_total = manifest_a.total_len + manifest_b.total_len;
    let mut forged_payload = Vec::new();
    forged_payload.extend_from_slice(&(b"video/mp4".len() as u32).to_be_bytes());
    forged_payload.extend_from_slice(b"video/mp4");
    forged_payload.extend_from_slice(&claimed_total.to_be_bytes());
    forged_payload.extend_from_slice(&[0xAAu8; 32]); // wrong digest
    let forged_obj =
        ObjectBuilder::new(ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.to_string()))
            .payload(Payload::Public(forged_payload))
            .link("part", manifest_a.id.clone())
            .link("part", manifest_b.id.clone())
            .sign(&root.did(), &device)
            .unwrap();
    store.insert(&forged_obj).unwrap();
    let forged = read_superblock(&forged_obj).unwrap();

    assert_eq!(
        assemble_superblock(&store, &forged),
        Err(MediaError::DigestMismatch)
    );
}

#[test]
fn a_superblock_with_no_parts_is_rejected_at_parse_time() {
    let (root, device) = human(15);
    let mut payload_bytes = Vec::new();
    payload_bytes.extend_from_slice(&0u32.to_be_bytes()); // empty content type
    payload_bytes.extend_from_slice(&0u64.to_be_bytes()); // total_len = 0
    payload_bytes.extend_from_slice(&[0u8; 32]); // digest
    let obj = ObjectBuilder::new(ObjectType::Custom(mini_media::SUPERBLOCK_TYPE.to_string()))
        .payload(Payload::Public(payload_bytes))
        .sign(&root.did(), &device)
        .unwrap();
    assert_eq!(read_superblock(&obj), Err(MediaError::BadManifest));
}

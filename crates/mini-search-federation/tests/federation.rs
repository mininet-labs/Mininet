//! Integration tests for Track F1/F2: round-trip fidelity for both signed
//! exchange object types, tamper detection at the object-integrity layer,
//! and canonical-form rejection at the index-segment layer.

use did_mini::{Capabilities, Controller};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_search_federation::{
    publish_crawl_observation, publish_index_segment, read_crawl_observation, read_index_segment,
    FederationError, CRAWL_OBSERVATION_TYPE, INDEX_SEGMENT_TYPE,
};
use mini_store::{MemoryBackend, Store};
use mini_web_types::{
    CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus, NormalizedHost,
    ProviderPseudonym, Scheme, WebMediaType,
};

fn human(seed: u8) -> (did_mini::Did, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root.did(), device)
}

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn url(host: &str, path: &str) -> CanonicalUrl {
    CanonicalUrl::new(
        Scheme::Https,
        NormalizedHost::new(host).unwrap(),
        None,
        path,
        None,
    )
    .unwrap()
}

fn sample_observation() -> CrawlObservation {
    CrawlObservation {
        id: CrawlObservationId(digest(b"obs-1")),
        requested_url: url("example.org", "/a"),
        final_url: url("example.org", "/a-final"),
        observed_at_ms: 12_345,
        status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
        content_digest: Some(digest(b"content")),
        media_type: Some(WebMediaType::Html),
        byte_length: Some(4096),
        redirect_chain: vec![url("example.org", "/a"), url("example.org", "/a-final")],
        crawler: ProviderPseudonym(digest(b"crawler-1")),
    }
}

#[test]
fn a_crawl_observation_round_trips_through_publish_and_read() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());
    let obs = sample_observation();

    let id = publish_crawl_observation(&mut store, &root, &device, &obs).unwrap();
    let obj = store.get(&id).unwrap();
    let parsed = read_crawl_observation(&obj).unwrap();
    assert_eq!(parsed, obs);
}

#[test]
fn an_observation_with_no_optional_fields_round_trips() {
    let (root, device) = human(11);
    let mut store = Store::new(MemoryBackend::new());
    let obs = CrawlObservation {
        id: CrawlObservationId(digest(b"obs-2")),
        requested_url: url("example.com", "/"),
        final_url: url("example.com", "/"),
        observed_at_ms: 0,
        status: FetchStatus::Timeout,
        content_digest: None,
        media_type: None,
        byte_length: None,
        redirect_chain: Vec::new(),
        crawler: ProviderPseudonym(digest(b"crawler-2")),
    };

    let id = publish_crawl_observation(&mut store, &root, &device, &obs).unwrap();
    let obj = store.get(&id).unwrap();
    assert_eq!(read_crawl_observation(&obj).unwrap(), obs);
}

#[test]
fn reading_the_wrong_object_type_as_a_crawl_observation_is_rejected() {
    let (root, device) = human(12);
    let obj = ObjectBuilder::new(ObjectType::Custom("mini/something-else".to_string()))
        .payload(Payload::Public(b"irrelevant".to_vec()))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_crawl_observation(&obj),
        Err(FederationError::WrongObjectType)
    );
}

#[test]
fn a_tampered_crawl_observation_object_fails_signature_verification() {
    let (root, device) = human(13);
    let mut store = Store::new(MemoryBackend::new());
    let obs = sample_observation();
    let id = publish_crawl_observation(&mut store, &root, &device, &obs).unwrap();
    let mut obj = store.get(&id).unwrap();

    // Flip a payload byte after signing, then confirm the workspace-wide
    // signature-verification layer (not something this crate reimplements)
    // catches it: `read_crawl_observation` alone only checks well-
    // formedness, so a caller that skips `verify_signature` would
    // otherwise accept a byte-flipped payload the signature no longer
    // covers.
    if let Payload::Public(bytes) = &mut obj.payload {
        if let Some(b) = bytes.last_mut() {
            *b ^= 0xFF;
        }
    }
    // Still decodes fine (well-formed bytes, just different content) --
    // the tamper is only caught by signature verification, not this
    // crate's own decode.
    assert!(read_crawl_observation(&obj).is_ok());
    assert!(obj.verify_signature(&device.kel()).is_err());
}

fn small_segment() -> IndexSegment {
    let mut b = IndexBuilder::new();
    let id = UrlId(digest(b"doc-1"));
    b.add_document(
        id,
        &[
            (Field::Title, "Federated Search"),
            (Field::Body, "distributed index segments"),
        ],
    );
    b.build()
}

#[test]
fn an_index_segment_round_trips_through_publish_and_read() {
    let (root, device) = human(14);
    let mut store = Store::new(MemoryBackend::new());
    let segment = small_segment();

    let (obj_id, segment_id) = publish_index_segment(&mut store, &root, &device, &segment).unwrap();
    assert_eq!(segment_id, segment.segment_id());

    let obj = store.get(&obj_id).unwrap();
    let parsed = read_index_segment(&obj).unwrap();
    assert_eq!(parsed.to_bytes(), segment.to_bytes());
    assert_eq!(parsed.segment_id(), segment.segment_id());
}

#[test]
fn reading_the_wrong_object_type_as_an_index_segment_is_rejected() {
    let (root, device) = human(15);
    let obj = ObjectBuilder::new(ObjectType::Custom(CRAWL_OBSERVATION_TYPE.to_string()))
        .payload(Payload::Public(b"not a segment".to_vec()))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_index_segment(&obj),
        Err(FederationError::WrongObjectType)
    );
}

#[test]
fn a_non_canonical_index_segment_payload_is_rejected_at_read_time() {
    let (root, device) = human(16);
    // Well-formed object, well-typed, but garbage bytes where a canonical
    // `IndexSegment` encoding is expected -- `IndexSegment::from_bytes`'s
    // own canonical-form enforcement must still catch this, not just this
    // crate's own object-type check.
    let obj = ObjectBuilder::new(ObjectType::Custom(INDEX_SEGMENT_TYPE.to_string()))
        .payload(Payload::Public(vec![0xAA; 16]))
        .sign(&root, &device)
        .unwrap();
    assert!(matches!(
        read_index_segment(&obj),
        Err(FederationError::LexicalIndex(_))
    ));
}

#[test]
fn an_encrypted_payload_is_rejected_for_both_object_types() {
    let (root, device) = human(17);
    let obs_obj = ObjectBuilder::new(ObjectType::Custom(CRAWL_OBSERVATION_TYPE.to_string()))
        .payload(Payload::Encrypted(vec![1, 2, 3]))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_crawl_observation(&obs_obj),
        Err(FederationError::NotPublicPayload)
    );

    let seg_obj = ObjectBuilder::new(ObjectType::Custom(INDEX_SEGMENT_TYPE.to_string()))
        .payload(Payload::Encrypted(vec![1, 2, 3]))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_index_segment(&seg_obj),
        Err(FederationError::NotPublicPayload)
    );
}

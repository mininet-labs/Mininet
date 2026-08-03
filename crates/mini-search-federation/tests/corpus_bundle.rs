//! Integration tests for Track F2b: signed corpus/context bundle round
//! trips, tamper detection, and reader bound enforcement -- the same
//! discipline `tests/federation.rs` already applies to F1/F2.

use did_mini::{Capabilities, Controller};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_query::DocumentContext;
use mini_ranker::DocumentMeta;
use mini_search_federation::{
    publish_corpus_bundle, read_corpus_bundle, FederationError, CORPUS_BUNDLE_TYPE,
};
use mini_store::{MemoryBackend, Store};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, IndexSegmentId, NormalizedHost,
    RestrictionReason, Scheme, UnavailabilityReason, UrlId, WebMediaType,
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

fn url_id(seed: &[u8]) -> UrlId {
    UrlId(digest(seed))
}

fn meta(host: &str, availability: AvailabilityState) -> DocumentMeta {
    DocumentMeta {
        url: url(host, "/a"),
        title: format!("{host} title"),
        snippet: format!("{host} snippet"),
        observed_at_ms: 1_000,
        inbound_links: 3,
        content_digest: digest(host.as_bytes()),
        availability,
    }
}

fn context(lang: Option<&str>, media_type: Option<WebMediaType>) -> DocumentContext {
    DocumentContext {
        language: lang.map(str::to_string),
        media_type,
        source_observation: CrawlObservationId(digest(b"obs-1")),
    }
}

fn segment_id() -> IndexSegmentId {
    IndexSegmentId(digest(b"segment-1"))
}

#[test]
fn a_bundle_with_every_availability_and_restriction_variant_round_trips() {
    let (root, device) = human(30);
    let mut store = Store::new(MemoryBackend::new());
    let docs = vec![
        (
            url_id(b"doc-available"),
            meta("a.example", AvailabilityState::Available),
        ),
        (
            url_id(b"doc-unavailable"),
            meta(
                "b.example",
                AvailabilityState::Unavailable(UnavailabilityReason::FetchFailed),
            ),
        ),
        (
            url_id(b"doc-restricted"),
            meta(
                "c.example",
                AvailabilityState::Restricted(RestrictionReason::LegalRestriction {
                    jurisdiction: "EU".to_string(),
                }),
            ),
        ),
        (
            url_id(b"doc-robots"),
            meta(
                "d.example",
                AvailabilityState::Restricted(RestrictionReason::RobotsExcluded),
            ),
        ),
    ];
    let contexts = vec![
        (
            url_id(b"doc-available"),
            context(Some("en"), Some(WebMediaType::Html)),
        ),
        (url_id(b"doc-unavailable"), context(None, None)),
    ];

    let id =
        publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &docs, &contexts).unwrap();
    let obj = store.get(&id).unwrap();
    let bundle = read_corpus_bundle(&obj).unwrap();

    assert_eq!(bundle.index_segment, segment_id());
    assert_eq!(bundle.docs.len(), docs.len());
    assert_eq!(bundle.contexts.len(), contexts.len());
    for (expected, actual) in docs.iter().zip(bundle.docs.iter()) {
        assert_eq!(expected, actual);
    }
    for (expected, actual) in contexts.iter().zip(bundle.contexts.iter()) {
        assert_eq!(expected, actual);
    }
}

#[test]
fn an_empty_bundle_round_trips() {
    let (root, device) = human(31);
    let mut store = Store::new(MemoryBackend::new());
    let id = publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &[], &[]).unwrap();
    let bundle = read_corpus_bundle(&store.get(&id).unwrap()).unwrap();
    assert_eq!(bundle.index_segment, segment_id());
    assert!(bundle.docs.is_empty());
    assert!(bundle.contexts.is_empty());
}

#[test]
fn reading_the_wrong_object_type_as_a_corpus_bundle_is_rejected() {
    let (root, device) = human(32);
    let post = ObjectBuilder::new(ObjectType::POST)
        .payload(Payload::Public(b"not a bundle".to_vec()))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_corpus_bundle(&post).unwrap_err(),
        FederationError::WrongObjectType
    );
}

#[test]
fn an_encrypted_payload_is_rejected() {
    let (root, device) = human(33);
    let obj = ObjectBuilder::new(ObjectType::Custom(CORPUS_BUNDLE_TYPE.to_string()))
        .payload(Payload::Encrypted(b"opaque".to_vec()))
        .sign(&root, &device)
        .unwrap();
    assert_eq!(
        read_corpus_bundle(&obj).unwrap_err(),
        FederationError::NotPublicPayload
    );
}

#[test]
fn publisher_rejects_an_overlong_title_before_encoding() {
    let (root, device) = human(34);
    let mut store = Store::new(MemoryBackend::new());
    let oversized = meta("e.example", AvailabilityState::Available);
    let mut oversized = oversized;
    oversized.title = "x".repeat(4_096);
    let docs = vec![(url_id(b"doc-x"), oversized)];
    assert_eq!(
        publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &docs, &[]).unwrap_err(),
        FederationError::LimitExceeded
    );
}

#[test]
fn publisher_rejects_an_overlong_jurisdiction_before_encoding() {
    let (root, device) = human(35);
    let mut store = Store::new(MemoryBackend::new());
    let restricted = meta(
        "f.example",
        AvailabilityState::Restricted(RestrictionReason::LegalRestriction {
            jurisdiction: "x".repeat(4_096),
        }),
    );
    let docs = vec![(url_id(b"doc-y"), restricted)];
    assert_eq!(
        publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &docs, &[]).unwrap_err(),
        FederationError::LimitExceeded
    );
}

#[test]
fn a_tampered_corpus_bundle_object_fails_signature_verification() {
    let (root, device) = human(36);
    let mut store = Store::new(MemoryBackend::new());
    let docs = vec![(
        url_id(b"doc-z"),
        meta("g.example", AvailabilityState::Available),
    )];
    let id = publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &docs, &[]).unwrap();
    let obj = store.get(&id).unwrap();

    let tampered_bytes = obj.to_bytes();
    let mut tampered_bytes = tampered_bytes;
    // Flip a byte deep enough in the payload to land inside the encoded
    // title string, not the outer envelope framing.
    let flip_at = tampered_bytes.len() - 20;
    tampered_bytes[flip_at] ^= 0xFF;
    let tampered = mini_objects::Object::from_bytes(&tampered_bytes).unwrap();

    // Still decodes fine (well-formed bytes, just different content) -- the
    // tamper is only caught by signature verification, not this crate's own
    // decode, mirroring F1/F2's own tamper-detection tests.
    assert!(read_corpus_bundle(&tampered).is_ok());
    assert!(tampered.verify_signature(&device.kel()).is_err());
}

#[test]
fn doc_and_context_counts_are_independent() {
    // A context entry with no corresponding doc entry (and vice versa) is
    // not itself an error -- the two tables are independently keyed by
    // UrlId, mirroring how mini_ranker::Corpus/mini_query::DocumentContextTable
    // are independently populated in the first place.
    let (root, device) = human(37);
    let mut store = Store::new(MemoryBackend::new());
    let docs = vec![(
        url_id(b"doc-only"),
        meta("h.example", AvailabilityState::Available),
    )];
    let contexts = vec![(url_id(b"context-only"), context(Some("fr"), None))];
    let id =
        publish_corpus_bundle(&mut store, &root, &device, &segment_id(), &docs, &contexts).unwrap();
    let bundle = read_corpus_bundle(&store.get(&id).unwrap()).unwrap();
    assert_eq!(bundle.docs.len(), 1);
    assert_eq!(bundle.contexts.len(), 1);
    assert_eq!(bundle.docs[0].0, url_id(b"doc-only"));
    assert_eq!(bundle.contexts[0].0, url_id(b"context-only"));
}

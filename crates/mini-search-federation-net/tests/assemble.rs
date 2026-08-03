//! `assemble_federation_source`'s own error paths -- no network involved,
//! this only exercises how it reads back already-stored F2/F2b objects.

use did_mini::{Capabilities, Controller};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, UrlId};
use mini_ranker::DocumentMeta;
use mini_search_federation::{publish_corpus_bundle, publish_index_segment};
use mini_search_federation_net::{assemble_federation_source, NetError};
use mini_store::{MemoryBackend, Store};
use mini_web_types::{AvailabilityState, CanonicalUrl, NormalizedHost, ProviderPseudonym, Scheme};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn url(host: &str) -> CanonicalUrl {
    CanonicalUrl::new(
        Scheme::Https,
        NormalizedHost::new(host).unwrap(),
        None,
        "/",
        None,
    )
    .unwrap()
}

fn provider() -> ProviderPseudonym {
    ProviderPseudonym(digest(b"provider"))
}

#[test]
fn assembly_fails_closed_with_no_index_segment() {
    let (root, device) = human(150);
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());
    // A bundle with no segment at all in the trusted set.
    let bundle_id = publish_corpus_bundle(
        &mut store,
        &root.did(),
        &device,
        &mini_web_types::IndexSegmentId(digest(b"seg")),
        &[],
        &[],
    )
    .unwrap();

    let err = assemble_federation_source(&store, &[bundle_id], provider()).unwrap_err();
    assert_eq!(err, NetError::NoIndexSegment);
}

#[test]
fn assembly_fails_closed_with_more_than_one_index_segment() {
    let (root, device) = human(151);
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());

    let mut b1 = IndexBuilder::new();
    b1.add_document(UrlId(digest(b"doc-1")), &[(Field::Title, "one")]);
    let (seg1_obj, _) =
        publish_index_segment(&mut store, &root.did(), &device, &b1.build()).unwrap();

    let mut b2 = IndexBuilder::new();
    b2.add_document(UrlId(digest(b"doc-2")), &[(Field::Title, "two")]);
    let (seg2_obj, _) =
        publish_index_segment(&mut store, &root.did(), &device, &b2.build()).unwrap();

    let err = assemble_federation_source(&store, &[seg1_obj, seg2_obj], provider()).unwrap_err();
    assert_eq!(err, NetError::AmbiguousIndexSegment);
}

#[test]
fn assembly_fails_closed_when_no_bundle_matches_the_segment() {
    let (root, device) = human(152);
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());

    let mut b = IndexBuilder::new();
    b.add_document(UrlId(digest(b"doc-1")), &[(Field::Title, "one")]);
    let (seg_obj, seg_id) =
        publish_index_segment(&mut store, &root.did(), &device, &b.build()).unwrap();

    // A bundle for a *different* segment id -- present in the trusted set,
    // but not a match.
    let unrelated_bundle = publish_corpus_bundle(
        &mut store,
        &root.did(),
        &device,
        &mini_web_types::IndexSegmentId(digest(b"a-different-segment")),
        &[],
        &[],
    )
    .unwrap();
    assert_ne!(
        mini_web_types::IndexSegmentId(digest(b"a-different-segment")),
        seg_id
    );

    let err =
        assemble_federation_source(&store, &[seg_obj, unrelated_bundle], provider()).unwrap_err();
    assert_eq!(err, NetError::NoMatchingCorpusBundle);
}

#[test]
fn assembly_succeeds_and_rebuilds_a_queryable_corpus() {
    let (root, device) = human(153);
    let mut store: Store<MemoryBackend> = Store::new(MemoryBackend::new());

    let doc_id = UrlId(digest(b"doc-1"));
    let mut b = IndexBuilder::new();
    b.add_document(doc_id.clone(), &[(Field::Title, "hello")]);
    let segment = b.build();
    let (seg_obj, seg_id) =
        publish_index_segment(&mut store, &root.did(), &device, &segment).unwrap();

    let meta = DocumentMeta {
        url: url("example.org"),
        title: "hello".to_string(),
        snippet: "hello".to_string(),
        observed_at_ms: 0,
        inbound_links: 0,
        content_digest: digest(b"content"),
        availability: AvailabilityState::Available,
    };
    let bundle_obj = publish_corpus_bundle(
        &mut store,
        &root.did(),
        &device,
        &seg_id,
        &[(doc_id.clone(), meta)],
        &[],
    )
    .unwrap();

    let owned = assemble_federation_source(&store, &[seg_obj, bundle_obj], provider()).unwrap();
    assert_eq!(owned.index_segment, seg_id);
    assert_eq!(owned.segment, segment);
    assert!(owned.corpus.get(&doc_id).is_some());
    assert_eq!(owned.contexts.len(), 0);
}

//! End-to-end exchange test from a caller's view: a provider builds and
//! publishes an index segment; a separate consumer receives only the bytes
//! and must decide whether to trust them, using nothing but this crate's
//! public API.

use mini_crypto::{HashAlgorithm, Multihash, SigningKey};
use mini_index_exchange::{accept_published_segment, provider_pseudonym, SegmentPublication};
use mini_lexical_index::{Field, IndexBuilder, UrlId};

fn url(seed: &[u8]) -> UrlId {
    UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
}

/// The provider side: build a real segment and publish it under a key.
fn provider_publishes() -> (Vec<u8>, Vec<u8>, SigningKey) {
    let key = SigningKey::from_seed(&[42u8; 32]);
    let mut b = IndexBuilder::new();
    b.add_document(
        url(b"https://docs.example/intro"),
        &[
            (Field::Title, "Getting Started"),
            (Field::Body, "an introduction to the system and its ideas"),
            (Field::Url, "docs.example intro"),
        ],
    );
    b.add_document(
        url(b"https://docs.example/guide"),
        &[
            (Field::Title, "User Guide"),
            (Field::Body, "a guide to using the system in practice"),
            (Field::Url, "docs.example guide"),
        ],
    );
    let segment = b.build();
    let publication = SegmentPublication::publish(segment.manifest(), &key);
    (segment.to_bytes(), publication.to_bytes(), key)
}

#[test]
fn a_consumer_accepts_a_genuine_publication_from_bytes_alone() {
    let (segment_bytes, publication_bytes, key) = provider_publishes();

    // The consumer has only bytes; it verifies both legs of trust.
    let (segment, verified) = accept_published_segment(&segment_bytes, &publication_bytes).unwrap();

    // The verified segment is queryable, and the provider is who signed it.
    assert!(!segment.term_documents("guide").is_empty());
    assert_eq!(verified.provider, provider_pseudonym(&key.verifying_key()));
}

#[test]
fn a_consumer_rejects_a_publication_paired_with_swapped_segment_bytes() {
    let (_genuine_segment, publication_bytes, _key) = provider_publishes();

    // An attacker keeps the genuine publication but swaps the segment bytes
    // for a different segment. The content-address check must reject it.
    let mut b = IndexBuilder::new();
    b.add_document(
        url(b"https://evil.example/x"),
        &[(Field::Body, "malicious payload")],
    );
    let forged = b.build();

    let result = accept_published_segment(&forged.to_bytes(), &publication_bytes);
    assert!(result.is_err());
}

#[test]
fn a_consumer_rejects_corrupted_publication_bytes() {
    let (segment_bytes, mut publication_bytes, _key) = provider_publishes();
    // Flip a byte in the publication's signature region (near the end).
    let last = publication_bytes.len() - 1;
    publication_bytes[last] ^= 0xFF;
    let result = accept_published_segment(&segment_bytes, &publication_bytes);
    assert!(result.is_err());
}

#[test]
fn the_same_segment_from_two_providers_yields_two_distinct_verified_publications() {
    // Plurality: two independent providers publish the identical segment.
    let mut b = IndexBuilder::new();
    b.add_document(
        url(b"https://shared/doc"),
        &[(Field::Body, "shared content")],
    );
    let segment = b.build();
    let segment_bytes = segment.to_bytes();

    let key_a = SigningKey::from_seed(&[1u8; 32]);
    let key_b = SigningKey::from_seed(&[2u8; 32]);
    let pub_a = SegmentPublication::publish(segment.manifest(), &key_a).to_bytes();
    let pub_b = SegmentPublication::publish(segment.manifest(), &key_b).to_bytes();

    let (_s1, v1) = accept_published_segment(&segment_bytes, &pub_a).unwrap();
    let (_s2, v2) = accept_published_segment(&segment_bytes, &pub_b).unwrap();

    // Same content-addressed segment, two different providers -- exactly the
    // no-monopoly property D-0312 requires.
    assert_eq!(v1.manifest.segment_id, v2.manifest.segment_id);
    assert_ne!(v1.provider, v2.provider);
}

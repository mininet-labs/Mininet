//! End-to-end tests for the lexical index as a caller uses it: build a
//! small corpus, query it, and round-trip a segment through its bytes and
//! back, confirming the content address survives.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};

fn url(seed: &[u8]) -> UrlId {
    UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
}

fn corpus() -> IndexSegment {
    let mut b = IndexBuilder::new();
    b.add_document(
        url(b"https://example.org/rust"),
        &[
            (Field::Title, "The Rust Programming Language"),
            (
                Field::Body,
                "Rust is a systems programming language focused on safety",
            ),
            (Field::Url, "example.org rust"),
        ],
    );
    b.add_document(
        url(b"https://example.org/python"),
        &[
            (Field::Title, "The Python Programming Language"),
            (Field::Body, "Python is a high level programming language"),
            (Field::Url, "example.org python"),
        ],
    );
    b.add_document(
        url(b"https://other.net/safety"),
        &[
            (Field::Title, "Memory Safety"),
            (
                Field::Body,
                "Rust brings memory safety without a garbage collector",
            ),
            (Field::Url, "other.net safety"),
        ],
    );
    b.build()
}

#[test]
fn a_shared_term_finds_every_matching_document() {
    let seg = corpus();
    // "programming" appears in the two language pages.
    let hits = seg.term_documents("programming");
    assert_eq!(hits.len(), 2);
    assert!(hits.contains(&url(b"https://example.org/rust")));
    assert!(hits.contains(&url(b"https://example.org/python")));

    // "rust" appears in the rust page (title/body/url) and the safety page
    // (body), so two distinct documents.
    let rust = seg.term_documents("Rust");
    assert_eq!(rust.len(), 2);
    assert!(rust.contains(&url(b"https://other.net/safety")));
}

#[test]
fn a_phrase_matches_only_where_words_are_adjacent() {
    let seg = corpus();
    // "programming language" is adjacent in both language titles/bodies.
    let pl = seg.phrase_documents("programming language");
    assert_eq!(pl.len(), 2);

    // "memory safety" is adjacent only on the safety page.
    assert_eq!(
        seg.phrase_documents("Memory Safety"),
        vec![url(b"https://other.net/safety")]
    );

    // "rust safety" is never adjacent anywhere.
    assert!(seg.phrase_documents("rust safety").is_empty());
}

#[test]
fn querying_by_a_url_word_works() {
    let seg = corpus();
    // The Url field tokenizes host/path words, so a domain word is findable.
    let hits = seg.term_documents("other");
    assert_eq!(hits, vec![url(b"https://other.net/safety")]);
}

#[test]
fn a_segment_survives_storage_as_bytes() {
    let seg = corpus();
    let id_before = seg.segment_id();

    // Simulate storing and reloading the segment from a blob store.
    let stored = seg.to_bytes();
    let reloaded = IndexSegment::from_bytes(&stored).unwrap();

    assert_eq!(reloaded.segment_id(), id_before);
    assert_eq!(reloaded.phrase_documents("programming language").len(), 2);
    // Re-serializing the reloaded segment yields identical bytes.
    assert_eq!(reloaded.to_bytes(), stored);
}

#[test]
fn two_independent_builders_agree_on_the_segment_id() {
    // The plurality property: a second party given the same documents
    // builds a byte-identical segment with the same id, without any shared
    // state or trust.
    let a = corpus().segment_id();
    let b = corpus().segment_id();
    assert_eq!(a, b);
}

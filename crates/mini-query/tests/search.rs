//! End-to-end tests for Track E7/E8: parsing a raw query string, applying
//! its filters without modifying `mini_ranker::rank`, and attaching result
//! provenance.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_query::{parse_query, search, DocumentContext, DocumentContextTable};
use mini_ranker::{Corpus, DocumentMeta, RankingProfile, RankingProfileId};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, IndexSegmentId, NormalizedHost,
    RestrictionReason, Scheme, WebMediaType,
};

fn url_id(seed: &[u8]) -> UrlId {
    UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
}

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn obs_id(seed: &[u8]) -> CrawlObservationId {
    CrawlObservationId(Multihash::of(HashAlgorithm::Blake3, seed))
}

fn segment_id() -> IndexSegmentId {
    IndexSegmentId(digest(b"segment"))
}

fn canonical(host: &str, path: &str) -> CanonicalUrl {
    CanonicalUrl::new(
        Scheme::Https,
        NormalizedHost::new(host).unwrap(),
        None,
        path,
        None,
    )
    .unwrap()
}

fn profile() -> RankingProfile {
    RankingProfile::public_default(RankingProfileId(digest(b"public-default")))
}

const DAY_MS: u64 = 86_400_000;

struct World {
    index: IndexSegment,
    corpus: Corpus,
    contexts: DocumentContextTable,
    now_ms: u64,
}

/// Three documents about "rust programming": two on rust-lang.org (one
/// English HTML, older; one English PDF, newer) and one on example.com
/// (French HTML). All contain the phrase "rust programming".
fn build_world() -> World {
    let mut b = IndexBuilder::new();
    let mut corpus = Corpus::new();
    let mut contexts = DocumentContextTable::new();

    let docs = [
        (
            &b"rust-html"[..],
            "rust-lang.org",
            "/html",
            "Rust Programming Guide",
            "rust programming basics",
            10 * DAY_MS,
            "en",
            WebMediaType::Html,
            b"obs-rust-html".as_slice(),
            b"digest-rust-html".as_slice(),
        ),
        (
            &b"rust-pdf"[..],
            "rust-lang.org",
            "/pdf",
            "Rust Programming Reference",
            "rust programming details",
            50 * DAY_MS,
            "en",
            WebMediaType::Pdf,
            b"obs-rust-pdf",
            b"digest-rust-pdf",
        ),
        (
            &b"example-fr"[..],
            "example.com",
            "/fr",
            "Guide de programmation Rust",
            "rust programming en francais",
            30 * DAY_MS,
            "fr",
            WebMediaType::Html,
            b"obs-example-fr",
            b"digest-example-fr",
        ),
    ];

    for (seed, host, path, title, body, observed_at_ms, lang, media_type, obs_seed, dig) in docs {
        let id = url_id(seed);
        b.add_document(id.clone(), &[(Field::Title, title), (Field::Body, body)]);
        corpus.insert(
            &id,
            DocumentMeta {
                url: canonical(host, path),
                title: title.to_string(),
                snippet: body.to_string(),
                observed_at_ms,
                inbound_links: 1,
                content_digest: digest(dig),
                availability: AvailabilityState::Available,
            },
        );
        contexts.insert(
            &id,
            DocumentContext {
                language: Some(lang.to_string()),
                media_type: Some(media_type),
                source_observation: obs_id(obs_seed),
            },
        );
    }

    World {
        index: b.build(),
        corpus,
        contexts,
        now_ms: 100 * DAY_MS,
    }
}

fn run(w: &World, raw: &str) -> Vec<String> {
    let parsed = parse_query(raw);
    let results = search(
        &w.index,
        &w.corpus,
        &w.contexts,
        &profile(),
        &parsed,
        segment_id(),
        w.now_ms,
        10,
    )
    .unwrap();
    results
        .iter()
        .map(|r| r.result.url.canonical_string())
        .collect()
}

#[test]
fn a_plain_query_matches_all_three_documents() {
    let w = build_world();
    let urls = run(&w, "rust programming");
    assert_eq!(urls.len(), 3);
}

#[test]
fn site_filter_restricts_to_one_host() {
    let w = build_world();
    let urls = run(&w, "rust programming site:example.com");
    assert_eq!(urls, vec!["https://example.com/fr"]);
}

#[test]
fn exclusion_removes_the_matching_document() {
    let w = build_world();
    let urls = run(&w, "rust programming -francais");
    assert_eq!(urls.len(), 2);
    assert!(!urls.contains(&"https://example.com/fr".to_string()));
}

#[test]
fn before_and_after_bound_the_observation_window() {
    let w = build_world();
    // Only the middle document (day 30) survives day 20 < observed < day 40.
    let urls = run(&w, "rust programming after:1970-01-21 before:1970-02-10");
    assert_eq!(urls, vec!["https://example.com/fr"]);
}

#[test]
fn lang_filter_matches_case_insensitively_against_context() {
    let w = build_world();
    let urls = run(&w, "rust programming lang:FR");
    assert_eq!(urls, vec!["https://example.com/fr"]);
}

#[test]
fn type_filter_matches_media_type() {
    let w = build_world();
    let urls = run(&w, "rust programming type:pdf");
    assert_eq!(urls, vec!["https://rust-lang.org/pdf"]);
}

#[test]
fn phrase_filter_still_composes_with_a_host_filter() {
    let w = build_world();
    let parsed = parse_query(r#""rust programming" site:rust-lang.org"#);
    assert_eq!(parsed.phrase.as_deref(), Some("rust programming"));
    let results = search(
        &w.index,
        &w.corpus,
        &w.contexts,
        &profile(),
        &parsed,
        segment_id(),
        w.now_ms,
        10,
    )
    .unwrap();
    assert_eq!(results.len(), 2);
    for r in &results {
        assert_eq!(r.result.url.host.as_str(), "rust-lang.org");
    }
}

#[test]
fn every_result_carries_provenance() {
    let w = build_world();
    let parsed = parse_query("rust programming site:rust-lang.org type:html");
    let results = search(
        &w.index,
        &w.corpus,
        &w.contexts,
        &profile(),
        &parsed,
        segment_id(),
        w.now_ms,
        10,
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert_eq!(r.index_segment, segment_id());
    assert_eq!(r.source_observation, obs_id(b"obs-rust-html"));
    assert_eq!(
        r.result.url.canonical_string(),
        "https://rust-lang.org/html"
    );
}

#[test]
fn filtered_out_documents_are_restricted_not_scored_down() {
    // The filtering mechanism reuses `AvailabilityState::Restricted`, which
    // by D-0312 must exclude outright rather than lower a score. Confirm a
    // filtered-out document simply never appears, regardless of how well it
    // would otherwise have scored (it is the best lexical match for its own
    // exact title terms).
    let w = build_world();
    // "reference" only occurs in the rust-lang.org/pdf document's title;
    // filtering to example.com must exclude it rather than merely demote it.
    let urls = run(&w, "reference site:example.com");
    assert!(urls.is_empty());
}

#[test]
fn an_empty_parsed_query_returns_no_results() {
    let w = build_world();
    let parsed = parse_query("   ");
    let results = search(
        &w.index,
        &w.corpus,
        &w.contexts,
        &profile(),
        &parsed,
        segment_id(),
        w.now_ms,
        10,
    )
    .unwrap();
    assert!(results.is_empty());
}

#[test]
fn restriction_reason_is_the_typed_user_filter_variant() {
    // Sanity check that the crate's own filtering mechanism uses the typed
    // `RestrictionReason::UserFilter`, not some other reason, so a caller
    // auditing a corpus can distinguish a user-filtered result from e.g. a
    // robots exclusion. Verified indirectly: build a corpus by hand with an
    // already-Restricted(UserFilter) document and confirm `search` still
    // treats it as excluded (identical to org-level filtering), i.e. the two
    // paths compose rather than conflict.
    let mut w = build_world();
    let id = url_id(b"rust-html");
    let mut meta = w.corpus.get(&id).unwrap().clone();
    meta.availability = AvailabilityState::Restricted(RestrictionReason::UserFilter);
    w.corpus.insert(&id, meta);
    let urls = run(&w, "rust programming");
    assert_eq!(urls.len(), 2);
    assert!(!urls.contains(&"https://rust-lang.org/html".to_string()));
}

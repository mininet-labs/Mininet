//! End-to-end ranker tests, focused on the D-0312 doctrine invariants the
//! ranker must uphold: determinism, no pay-to-rank (structural), no silent
//! availability penalty, duplicate removal, domain diversity, and a
//! transparent per-signal explanation.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_ranker::{rank, rescore, Corpus, DocumentMeta, Query, RankingProfile, RankingProfileId};
use mini_web_types::{AvailabilityState, CanonicalUrl, NormalizedHost, RestrictionReason, Scheme};

fn url_id(seed: &[u8]) -> UrlId {
    UrlId(Multihash::of(HashAlgorithm::Blake3, seed))
}

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
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

const WEEK_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

/// A small corpus: three pages about programming languages, plus helpers to
/// tweak one document's metadata per test.
struct World {
    index: IndexSegment,
    corpus: Corpus,
    now_ms: u64,
}

fn build_world() -> World {
    let now_ms = 100 * WEEK_MS;
    let mut b = IndexBuilder::new();
    let mut corpus = Corpus::new();

    let docs = [
        (
            &b"https://rust-lang.org/"[..],
            "rust-lang.org",
            "/",
            "The Rust Programming Language",
            "Rust is a systems programming language",
            1u64,
            50u32,
            b"digest-rust".as_slice(),
        ),
        (
            &b"https://python.org/"[..],
            "python.org",
            "/",
            "The Python Programming Language",
            "Python is a programming language",
            1,
            40,
            b"digest-python",
        ),
        (
            &b"https://example.com/prog"[..],
            "example.com",
            "/prog",
            "Programming Basics",
            "an introduction to programming",
            1,
            2,
            b"digest-example",
        ),
    ];

    for (seed, host, path, title, body, week, links, dig) in docs {
        let id = url_id(seed);
        b.add_document(
            id.clone(),
            &[
                (Field::Title, title),
                (Field::Body, body),
                (Field::Url, &format!("{host} {path}")),
            ],
        );
        corpus.insert(
            &id,
            DocumentMeta {
                url: canonical(host, path),
                title: title.to_string(),
                snippet: body.to_string(),
                observed_at_ms: week * WEEK_MS,
                inbound_links: links,
                content_digest: digest(dig),
                availability: AvailabilityState::Available,
            },
        );
    }

    World {
        index: b.build(),
        corpus,
        now_ms,
    }
}

#[test]
fn ranking_is_deterministic() {
    let w = build_world();
    let q = Query::new(["programming", "language"]);
    let a = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    let b = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b.iter()) {
        assert_eq!(x.url, y.url);
        assert_eq!(x.relevance_score_bps, y.relevance_score_bps);
    }
}

#[test]
fn coverage_beats_a_single_term_match() {
    // "programming language" (both terms) matches rust and python fully;
    // example.com has "programming" but not "language", so it ranks below.
    let w = build_world();
    let q = Query::new(["programming", "language"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    assert_eq!(results.len(), 3);
    // The example.com page (one of two terms) must be last.
    assert_eq!(results.last().unwrap().url.host.as_str(), "example.com");
}

#[test]
fn every_result_explains_its_score() {
    let w = build_world();
    let q = Query::new(["programming"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    assert!(!results.is_empty());
    for r in &results {
        // The explanation's components are populated and the profile id is
        // carried, so the score is auditable.
        let e = &r.explanation;
        assert!(e.lexical_bps.value() > 0);
        assert_eq!(r.ranking_profile, profile().id);
    }
}

#[test]
fn a_restricted_document_is_excluded_not_demoted() {
    // Mark the otherwise strong rust page as robots-restricted. It must
    // vanish from results entirely -- never appear with a low score.
    let mut w = build_world();
    let rust = url_id(b"https://rust-lang.org/");
    let mut meta = w.corpus.get(&rust).unwrap().clone();
    meta.availability = AvailabilityState::Restricted(RestrictionReason::RobotsExcluded);
    w.corpus.insert(&rust, meta);

    let q = Query::new(["programming", "language"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    assert!(results
        .iter()
        .all(|r| r.url.host.as_str() != "rust-lang.org"));
}

#[test]
fn exact_duplicates_are_removed() {
    // Add a fourth page with identical content_digest to the python page,
    // at a different URL. Only one of the two should survive.
    let mut w = build_world();
    let dup = url_id(b"https://mirror.example/python");
    let mut b = IndexBuilder::new();
    // Rebuild the index including the duplicate so it is a candidate.
    for (seed, host, path, title, body) in [
        (
            &b"https://rust-lang.org/"[..],
            "rust-lang.org",
            "/",
            "The Rust Programming Language",
            "Rust is a systems programming language",
        ),
        (
            &b"https://python.org/"[..],
            "python.org",
            "/",
            "The Python Programming Language",
            "Python is a programming language",
        ),
        (
            &b"https://example.com/prog"[..],
            "example.com",
            "/prog",
            "Programming Basics",
            "an introduction to programming",
        ),
        (
            &b"https://mirror.example/python"[..],
            "mirror.example",
            "/python",
            "The Python Programming Language",
            "Python is a programming language",
        ),
    ] {
        b.add_document(
            url_id(seed),
            &[
                (Field::Title, title),
                (Field::Body, body),
                (Field::Url, &format!("{host} {path}")),
            ],
        );
    }
    w.index = b.build();
    w.corpus.insert(
        &dup,
        DocumentMeta {
            url: canonical("mirror.example", "/python"),
            title: "The Python Programming Language".to_string(),
            snippet: "Python is a programming language".to_string(),
            observed_at_ms: 5 * WEEK_MS, // later than the python.org original
            inbound_links: 100,
            content_digest: digest(b"digest-python"), // same content
            availability: AvailabilityState::Available,
        },
    );

    let q = Query::new(["programming", "language"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    // Exactly one of the two python-content hosts appears, and it is the
    // earlier-observed original (python.org), not the mirror.
    let py_hosts: Vec<_> = results
        .iter()
        .map(|r| r.url.host.as_str())
        .filter(|h| *h == "python.org" || *h == "mirror.example")
        .collect();
    assert_eq!(py_hosts, vec!["python.org"]);
}

#[test]
fn domain_diversity_demotes_a_repeated_host() {
    // Two strong pages from the same host and one from another. The second
    // same-host page must be demoted below the other host, even if its raw
    // content score would tie.
    let mut b = IndexBuilder::new();
    let mut corpus = Corpus::new();
    let entries = [
        (
            &b"https://big.example/a"[..],
            "big.example",
            "/a",
            "widgets guide",
        ),
        (
            &b"https://big.example/b"[..],
            "big.example",
            "/b",
            "widgets guide",
        ),
        (
            &b"https://small.example/x"[..],
            "small.example",
            "/x",
            "widgets guide",
        ),
    ];
    for (seed, host, path, body) in entries {
        let id = url_id(seed);
        b.add_document(
            id.clone(),
            &[(Field::Body, body), (Field::Url, &format!("{host} {path}"))],
        );
        corpus.insert(
            &id,
            DocumentMeta {
                url: canonical(host, path),
                title: "Widgets".to_string(),
                snippet: body.to_string(),
                observed_at_ms: WEEK_MS,
                inbound_links: 10,
                content_digest: digest(seed), // distinct content
                availability: AvailabilityState::Available,
            },
        );
    }
    let index = b.build();
    let q = Query::new(["widgets"]);
    let results = rank(&index, &corpus, &profile(), &q, 100 * WEEK_MS, 10).unwrap();

    let hosts: Vec<&str> = results.iter().map(|r| r.url.host.as_str()).collect();
    assert_eq!(hosts.len(), 3);
    // small.example must not be last: it should outrank the *second*
    // big.example page thanks to the diversity penalty.
    assert!(hosts[1] == "small.example");
    assert_eq!(hosts[2], "big.example");
}

#[test]
fn an_empty_query_returns_nothing() {
    let w = build_world();
    let q = Query::new(Vec::<String>::new());
    assert!(rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn max_results_bounds_the_output() {
    let w = build_world();
    let q = Query::new(["programming"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 1).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn a_missing_corpus_entry_is_surfaced() {
    // Index a document the corpus does not describe.
    let mut b = IndexBuilder::new();
    b.add_document(url_id(b"ghost"), &[(Field::Body, "orphan document")]);
    let index = b.build();
    let corpus = Corpus::new();
    let q = Query::new(["orphan"]);
    assert!(rank(&index, &corpus, &profile(), &q, WEEK_MS, 10).is_err());
}

#[test]
fn a_phrase_query_boosts_an_adjacent_match() {
    // "programming language" as a phrase should rank a page where the words
    // are adjacent above one where only the individual words appear.
    let w = build_world();
    let term_only = Query::new(["programming", "language"]);
    let phrase = Query::new(Vec::<String>::new()).with_phrase("programming language");

    let t = rank(&w.index, &w.corpus, &profile(), &term_only, w.now_ms, 10).unwrap();
    let p = rank(&w.index, &w.corpus, &profile(), &phrase, w.now_ms, 10).unwrap();

    // Both return results; the phrase query gives the adjacent pages a
    // strictly higher score than the term-only query does (phrase signal
    // fires).
    let top_term = t.first().unwrap().relevance_score_bps.value();
    let top_phrase = p.first().unwrap().relevance_score_bps.value();
    assert!(top_phrase > top_term);
}

#[test]
fn rescore_reproduces_the_original_score_under_the_same_profile() {
    let w = build_world();
    let q = Query::new(["programming", "language"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    for r in &results {
        let recomputed = rescore(&r.explanation, &profile()).unwrap();
        assert_eq!(recomputed, r.relevance_score_bps);
    }
}

#[test]
fn rescore_under_a_lexical_only_profile_differs_from_the_public_default() {
    let w = build_world();
    let q = Query::new(["programming", "language"]);
    let results = rank(&w.index, &w.corpus, &profile(), &q, w.now_ms, 10).unwrap();
    let lexical_only = RankingProfile {
        id: RankingProfileId(digest(b"lexical-only")),
        version: 1,
        lexical_weight: mini_web_types::WeightBps::new(10_000).unwrap(),
        phrase_weight: mini_web_types::WeightBps::ZERO,
        link_weight: mini_web_types::WeightBps::ZERO,
        freshness_weight: mini_web_types::WeightBps::ZERO,
        originality_weight: mini_web_types::WeightBps::ZERO,
        diversity_weight: mini_web_types::WeightBps::ZERO,
        personalization: mini_web_types::PersonalizationPolicy::None,
    };
    let top = results.first().unwrap();
    let rescored = rescore(&top.explanation, &lexical_only).unwrap();
    // Under a lexical-only profile the score collapses to exactly the
    // lexical signal, which for the public-default (mixed) profile is
    // never the entire score -- so this must differ from the original.
    assert_eq!(rescored, top.explanation.lexical_bps);
    assert_ne!(rescored, top.relevance_score_bps);
}

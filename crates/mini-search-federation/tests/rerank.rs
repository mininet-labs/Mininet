//! Integration tests for Track F4: local re-ranking of an already-merged
//! result set under a caller's own profile, with no re-query.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_query::{parse_query, DocumentContext, DocumentContextTable};
use mini_ranker::{Corpus, DocumentMeta};
use mini_search_federation::{federate_query, local_rerank, FederationSource};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, IndexSegmentId, NormalizedHost,
    PersonalizationPolicy, ProviderPseudonym, RankingProfile, RankingProfileId, Scheme, WeightBps,
};

fn digest(seed: &[u8]) -> Multihash {
    Multihash::of(HashAlgorithm::Blake3, seed)
}

fn url_id(seed: &[u8]) -> UrlId {
    UrlId(digest(seed))
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

fn lexical_only_profile() -> RankingProfile {
    RankingProfile {
        id: RankingProfileId(digest(b"lexical-only")),
        version: 1,
        lexical_weight: WeightBps::new(10_000).unwrap(),
        phrase_weight: WeightBps::ZERO,
        link_weight: WeightBps::ZERO,
        freshness_weight: WeightBps::ZERO,
        originality_weight: WeightBps::ZERO,
        diversity_weight: WeightBps::ZERO,
        personalization: PersonalizationPolicy::None,
    }
}

fn link_only_profile() -> RankingProfile {
    RankingProfile {
        id: RankingProfileId(digest(b"link-only")),
        version: 1,
        lexical_weight: WeightBps::ZERO,
        phrase_weight: WeightBps::ZERO,
        link_weight: WeightBps::new(10_000).unwrap(),
        freshness_weight: WeightBps::ZERO,
        originality_weight: WeightBps::ZERO,
        diversity_weight: WeightBps::ZERO,
        personalization: PersonalizationPolicy::None,
    }
}

struct Provider {
    id: ProviderPseudonym,
    index: IndexSegment,
    corpus: Corpus,
    contexts: DocumentContextTable,
    segment: IndexSegmentId,
}

fn build_provider(
    provider_seed: &[u8],
    doc_seed: &[u8],
    host: &str,
    title: &str,
    body: &str,
    inbound_links: u32,
) -> Provider {
    let mut b = IndexBuilder::new();
    let mut corpus = Corpus::new();
    let mut contexts = DocumentContextTable::new();
    let id = url_id(doc_seed);
    b.add_document(id.clone(), &[(Field::Title, title), (Field::Body, body)]);
    corpus.insert(
        &id,
        DocumentMeta {
            url: canonical(host, "/"),
            title: title.to_string(),
            snippet: body.to_string(),
            observed_at_ms: 0,
            inbound_links,
            content_digest: digest(doc_seed),
            availability: AvailabilityState::Available,
        },
    );
    contexts.insert(
        &id,
        DocumentContext {
            language: Some("en".to_string()),
            media_type: None,
            source_observation: CrawlObservationId(digest(provider_seed)),
        },
    );
    Provider {
        id: ProviderPseudonym(digest(provider_seed)),
        index: b.build(),
        corpus,
        contexts,
        segment: IndexSegmentId(digest(provider_seed)),
    }
}

fn sources(providers: &[Provider]) -> Vec<FederationSource<'_>> {
    providers
        .iter()
        .map(|p| FederationSource {
            provider: p.id.clone(),
            index: &p.index,
            corpus: &p.corpus,
            contexts: &p.contexts,
            index_segment: p.segment.clone(),
        })
        .collect()
}

#[test]
fn rerank_updates_scores_and_the_named_profile() {
    let providers = vec![build_provider(
        b"prov-a",
        b"doc-a",
        "a.example",
        "Rust Guide",
        "rust programming",
        10,
    )];
    let parsed = parse_query("rust programming");
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();
    assert_eq!(merged[0].result.result.ranking_profile, profile().id);

    let reranked = local_rerank(&merged, &lexical_only_profile(), 10).unwrap();
    assert_eq!(reranked.len(), 1);
    assert_eq!(
        reranked[0].result.result.ranking_profile,
        lexical_only_profile().id
    );
    // Under a lexical-only profile the score collapses to exactly the
    // lexical signal, which is never the whole public-default score.
    assert_eq!(
        reranked[0].result.result.relevance_score_bps,
        reranked[0].result.result.explanation.lexical_bps
    );
}

#[test]
fn rerank_under_the_same_profile_reproduces_the_original_order() {
    let providers = vec![
        build_provider(
            b"prov-a",
            b"doc-a",
            "a.example",
            "Rust Guide",
            "rust programming",
            10,
        ),
        build_provider(
            b"prov-b",
            b"doc-b",
            "b.example",
            "Rust Handbook",
            "rust programming",
            3,
        ),
    ];
    let parsed = parse_query("rust programming");
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();
    let reranked = local_rerank(&merged, &profile(), 10).unwrap();

    let merged_urls: Vec<String> = merged
        .iter()
        .map(|r| r.result.result.url.canonical_string())
        .collect();
    let reranked_urls: Vec<String> = reranked
        .iter()
        .map(|r| r.result.result.url.canonical_string())
        .collect();
    assert_eq!(merged_urls, reranked_urls);
}

#[test]
fn rerank_can_reorder_results_under_a_different_profile() {
    // Doc "a" has a saturating inbound-link count but only weakly matches
    // the query lexically (one of two terms, once); doc "b" has almost no
    // inbound links but matches both query terms with strong coverage.
    // Querying under a link-only profile must put "a" first; re-ranking
    // the same merged set under a lexical-only profile must flip the
    // order to "b" first -- with no re-query between the two.
    let providers = vec![
        build_provider(b"prov-a", b"doc-a", "a.example", "Guide", "rust", 100_000),
        build_provider(
            b"prov-b",
            b"doc-b",
            "b.example",
            "Rust Programming Rust",
            "rust programming rust programming",
            1,
        ),
    ];
    let parsed = parse_query("rust programming");
    let merged = federate_query(
        &sources(&providers),
        &link_only_profile(),
        &parsed,
        1_000,
        10,
    )
    .unwrap();
    assert_eq!(merged[0].result.result.url.host.as_str(), "a.example");

    let reranked = local_rerank(&merged, &lexical_only_profile(), 10).unwrap();
    assert_eq!(reranked[0].result.result.url.host.as_str(), "b.example");
}

#[test]
fn rerank_max_results_truncates() {
    let providers = vec![
        build_provider(
            b"prov-a",
            b"doc-a",
            "a.example",
            "Rust Guide",
            "rust programming",
            10,
        ),
        build_provider(
            b"prov-b",
            b"doc-b",
            "b.example",
            "Rust Handbook",
            "rust programming",
            9,
        ),
        build_provider(
            b"prov-c",
            b"doc-c",
            "c.example",
            "Rust Notes",
            "rust programming",
            8,
        ),
    ];
    let parsed = parse_query("rust programming");
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();
    let reranked = local_rerank(&merged, &profile(), 2).unwrap();
    assert_eq!(reranked.len(), 2);
}

#[test]
fn rerank_of_an_empty_list_is_empty() {
    let reranked = local_rerank(&[], &profile(), 10).unwrap();
    assert!(reranked.is_empty());
}

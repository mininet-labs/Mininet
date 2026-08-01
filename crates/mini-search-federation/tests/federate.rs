//! Integration tests for Track F3: federated query merging across
//! multiple local providers, preserving per-result provenance.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_lexical_index::{Field, IndexBuilder, IndexSegment, UrlId};
use mini_query::{parse_query, DocumentContext, DocumentContextTable};
use mini_ranker::{Corpus, DocumentMeta};
use mini_search_federation::{federate_query, FederationSource};
use mini_web_types::{
    AvailabilityState, CanonicalUrl, CrawlObservationId, IndexSegmentId, NormalizedHost,
    ProviderPseudonym, RankingProfile, RankingProfileId, Scheme,
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

fn provider(seed: &[u8]) -> ProviderPseudonym {
    ProviderPseudonym(digest(seed))
}

fn segment_id(seed: &[u8]) -> IndexSegmentId {
    IndexSegmentId(digest(seed))
}

/// A minimal single-document provider: one document with `title`/`body`
/// text, `inbound_links` controlling its score, on `host`.
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
        id: provider(provider_seed),
        index: b.build(),
        corpus,
        contexts,
        segment: segment_id(provider_seed),
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
fn results_from_every_provider_are_merged_and_tagged() {
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
            5,
        ),
    ];
    let parsed = parse_query("rust programming");
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();

    assert_eq!(merged.len(), 2);
    let hosts: Vec<&str> = merged
        .iter()
        .map(|r| r.result.result.url.host.as_str())
        .collect();
    assert!(hosts.contains(&"a.example"));
    assert!(hosts.contains(&"b.example"));
    for r in &merged {
        if r.result.result.url.host.as_str() == "a.example" {
            assert_eq!(r.provider, providers[0].id);
        } else {
            assert_eq!(r.provider, providers[1].id);
        }
    }
}

#[test]
fn a_shared_url_across_providers_keeps_the_higher_scoring_copy() {
    // Both providers publish the SAME URL, but provider "hi" has far more
    // inbound links, so it must win the URL-level dedup deterministically.
    let providers = vec![
        build_provider(
            b"lo",
            b"doc-shared",
            "shared.example",
            "Rust",
            "rust programming",
            1,
        ),
        build_provider(
            b"hi",
            b"doc-shared",
            "shared.example",
            "Rust",
            "rust programming",
            500,
        ),
    ];
    let parsed = parse_query("rust programming");
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].provider, providers[1].id);
}

#[test]
fn merging_is_deterministic_regardless_of_source_order() {
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
            5,
        ),
        build_provider(
            b"prov-c",
            b"doc-c",
            "c.example",
            "Rust Notes",
            "rust programming",
            5,
        ),
    ];
    let parsed = parse_query("rust programming");

    let forward = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 10).unwrap();
    let mut reversed_providers = vec![];
    for p in providers.iter().rev() {
        reversed_providers.push(FederationSource {
            provider: p.id.clone(),
            index: &p.index,
            corpus: &p.corpus,
            contexts: &p.contexts,
            index_segment: p.segment.clone(),
        });
    }
    let backward = federate_query(&reversed_providers, &profile(), &parsed, 1_000, 10).unwrap();

    let forward_urls: Vec<String> = forward
        .iter()
        .map(|r| r.result.result.url.canonical_string())
        .collect();
    let backward_urls: Vec<String> = backward
        .iter()
        .map(|r| r.result.result.url.canonical_string())
        .collect();
    assert_eq!(forward_urls, backward_urls);
}

#[test]
fn max_results_bounds_the_merged_list() {
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
    let merged = federate_query(&sources(&providers), &profile(), &parsed, 1_000, 2).unwrap();
    assert_eq!(merged.len(), 2);
}

#[test]
fn an_empty_source_list_returns_no_results() {
    let parsed = parse_query("rust programming");
    let merged = federate_query(&[], &profile(), &parsed, 1_000, 10).unwrap();
    assert!(merged.is_empty());
}

#[test]
fn each_result_keeps_its_own_provenance() {
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
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].result.index_segment, providers[0].segment);
    assert_eq!(
        merged[0].result.source_observation,
        CrawlObservationId(digest(b"prov-a"))
    );
}

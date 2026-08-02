//! Integration tests for Track F7's bounded, provenance-preserving local
//! history over F1 crawl observations.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_objects::ObjectId;
use mini_search_federation::{
    FederationError, SnapshotIndex, SnapshotInsert, SnapshotLimits, VersionRelation,
};
use mini_web_types::{
    CanonicalUrl, CrawlObservation, CrawlObservationId, FetchStatus, HttpStatus, NormalizedHost,
    ProviderPseudonym, Scheme, WebMediaType,
};

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

fn object_id(seed: &str) -> ObjectId {
    let mh = digest(seed.as_bytes());
    let encoded =
        mini_crypto::encoding::encode(mini_crypto::encoding::BASE58BTC, &mh.to_bytes()).unwrap();
    ObjectId::parse(&encoded).unwrap()
}

fn observation(
    seed: &str,
    final_url: CanonicalUrl,
    observed_at_ms: u64,
    content: Option<&[u8]>,
    crawler_seed: &str,
) -> CrawlObservation {
    CrawlObservation {
        id: CrawlObservationId(digest(format!("observation:{seed}").as_bytes())),
        requested_url: final_url.clone(),
        final_url,
        observed_at_ms,
        status: FetchStatus::Success(HttpStatus::new(200).unwrap()),
        content_digest: content.map(digest),
        media_type: Some(WebMediaType::Html),
        byte_length: content.map(|bytes| bytes.len() as u64),
        redirect_chain: Vec::new(),
        crawler: ProviderPseudonym(digest(format!("crawler:{crawler_seed}").as_bytes())),
    }
}

fn insert(
    index: &mut SnapshotIndex,
    object_seed: &str,
    observation: CrawlObservation,
) -> SnapshotInsert {
    index
        .insert_observation(object_id(object_seed), observation)
        .unwrap()
}

#[test]
fn history_is_empty_for_an_unrecorded_url() {
    let index = SnapshotIndex::new();
    let u = url("example.org", "/");
    assert!(index.history(&u).is_empty());
    assert!(index.latest(&u).is_empty());
    assert!(index.at_or_before(&u, 100).is_empty());
    assert_eq!(index.len(), 0);
    assert!(index.is_empty());
}

#[test]
fn insertion_derives_the_final_url_and_preserves_the_full_observation() {
    let mut index = SnapshotIndex::new();
    let requested = url("alias.example", "/old");
    let final_url = url("example.org", "/new");
    let mut observed = observation("redirected", final_url.clone(), 100, Some(b"body"), "a");
    observed.requested_url = requested.clone();
    observed.redirect_chain = vec![final_url.clone()];
    let expected = observed.clone();

    assert_eq!(
        insert(&mut index, "redirected-object", observed),
        SnapshotInsert::Inserted
    );
    assert!(index.history(&requested).is_empty());
    assert_eq!(index.history(&final_url)[0].observation, expected);
    assert_eq!(index.url_count(), 1);
    assert_eq!(index.len(), 1);
}

#[test]
fn snapshots_are_oldest_first_regardless_of_insertion_order() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        "v3",
        observation("v3", u.clone(), 300, Some(b"c"), "a"),
    );
    insert(
        &mut index,
        "v1",
        observation("v1", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        "v2",
        observation("v2", u.clone(), 200, Some(b"b"), "a"),
    );

    let times: Vec<u64> = index
        .history(&u)
        .iter()
        .map(|s| s.observation.observed_at_ms)
        .collect();
    assert_eq!(times, vec![100, 200, 300]);
}

#[test]
fn inserting_the_same_object_and_observation_is_idempotent() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    let id = object_id("v1");
    let observed = observation("v1", u.clone(), 100, Some(b"a"), "a");

    assert_eq!(
        index
            .insert_observation(id.clone(), observed.clone())
            .unwrap(),
        SnapshotInsert::Inserted
    );
    assert_eq!(
        index.insert_observation(id, observed).unwrap(),
        SnapshotInsert::AlreadyPresent
    );
    assert_eq!(index.history(&u).len(), 1);
    assert_eq!(index.len(), 1);
}

#[test]
fn one_object_id_cannot_be_rebound_to_different_observation_bytes() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    let id = object_id("same-object");
    index
        .insert_observation(
            id.clone(),
            observation("v1", u.clone(), 100, Some(b"a"), "a"),
        )
        .unwrap();

    let err = index
        .insert_observation(id, observation("v2", u, 200, Some(b"b"), "a"))
        .unwrap_err();
    assert_eq!(err, FederationError::ConflictingObjectBinding);
    assert_eq!(index.len(), 1);
}

#[test]
fn one_object_id_cannot_be_rebound_to_another_final_url() {
    let mut index = SnapshotIndex::new();
    let a = url("a.example", "/");
    let b = url("b.example", "/");
    let id = object_id("same-object");
    index
        .insert_observation(
            id.clone(),
            observation("a", a.clone(), 100, Some(b"a"), "a"),
        )
        .unwrap();

    assert_eq!(
        index
            .insert_observation(id, observation("b", b.clone(), 100, Some(b"b"), "a"))
            .unwrap_err(),
        FederationError::ConflictingObjectBinding
    );
    assert_eq!(index.history(&a).len(), 1);
    assert!(index.history(&b).is_empty());
}

#[test]
fn explicit_url_per_url_and_total_limits_fail_closed() {
    let a = url("a.example", "/");
    let b = url("b.example", "/");

    let mut url_limited = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 1,
        max_snapshots_per_url: 4,
        max_total_snapshots: 4,
    });
    insert(
        &mut url_limited,
        "a1",
        observation("a1", a.clone(), 100, Some(b"a"), "a"),
    );
    assert_eq!(
        url_limited
            .insert_observation(
                object_id("b1"),
                observation("b1", b.clone(), 100, Some(b"b"), "b")
            )
            .unwrap_err(),
        FederationError::LimitExceeded
    );

    let mut per_url_limited = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 2,
        max_snapshots_per_url: 1,
        max_total_snapshots: 4,
    });
    insert(
        &mut per_url_limited,
        "a1-per-url",
        observation("a1-per-url", a.clone(), 100, Some(b"a"), "a"),
    );
    assert_eq!(
        per_url_limited
            .insert_observation(
                object_id("a2-per-url"),
                observation("a2-per-url", a.clone(), 200, Some(b"b"), "a")
            )
            .unwrap_err(),
        FederationError::LimitExceeded
    );

    let mut total_limited = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 2,
        max_snapshots_per_url: 2,
        max_total_snapshots: 1,
    });
    insert(
        &mut total_limited,
        "a1-total",
        observation("a1-total", a, 100, Some(b"a"), "a"),
    );
    assert_eq!(
        total_limited
            .insert_observation(
                object_id("b1-total"),
                observation("b1-total", b, 100, Some(b"b"), "b")
            )
            .unwrap_err(),
        FederationError::LimitExceeded
    );
}

#[test]
fn zero_limits_reject_the_first_insertion() {
    let mut index = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 0,
        max_snapshots_per_url: 0,
        max_total_snapshots: 0,
    });
    let u = url("example.org", "/");
    assert_eq!(
        index
            .insert_observation(
                object_id("blocked"),
                observation("blocked", u, 100, Some(b"a"), "a")
            )
            .unwrap_err(),
        FederationError::LimitExceeded
    );
}

#[test]
fn latest_and_at_or_before_return_the_whole_equal_timestamp_group() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        "early",
        observation("early", u.clone(), 100, Some(b"old"), "a"),
    );
    insert(
        &mut index,
        "late-a",
        observation("late-a", u.clone(), 200, Some(b"new"), "a"),
    );
    insert(
        &mut index,
        "late-b",
        observation("late-b", u.clone(), 200, Some(b"new"), "b"),
    );

    assert_eq!(index.latest(&u).len(), 2);
    assert_eq!(index.at_or_before(&u, 250).len(), 2);
    assert_eq!(index.at_or_before(&u, 150).len(), 1);
    assert_eq!(
        index.at_or_before(&u, 150)[0].observation.observed_at_ms,
        100
    );
    assert!(index.at_or_before(&u, 50).is_empty());
}

#[test]
fn between_bounds_are_inclusive_lower_exclusive_upper() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for (seed, time, body) in [("v1", 100, b"a"), ("v2", 200, b"b"), ("v3", 300, b"c")] {
        insert(
            &mut index,
            seed,
            observation(seed, u.clone(), time, Some(body), "a"),
        );
    }

    let times = |items: Vec<&mini_search_federation::Snapshot>| {
        items
            .into_iter()
            .map(|s| s.observation.observed_at_ms)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        times(index.between(&u, Some(100), Some(300))),
        vec![100, 200]
    );
    assert_eq!(times(index.between(&u, None, Some(200))), vec![100]);
    assert_eq!(times(index.between(&u, Some(200), None)), vec![200, 300]);
}

#[test]
fn unknown_fetches_do_not_create_false_version_boundaries() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        "v1",
        observation("v1", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        "unknown",
        observation("unknown", u.clone(), 200, None, "a"),
    );
    insert(
        &mut index,
        "v1-again",
        observation("v1-again", u.clone(), 300, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        "v2",
        observation("v2", u.clone(), 400, Some(b"b"), "a"),
    );
    insert(
        &mut index,
        "v2-again",
        observation("v2-again", u.clone(), 500, Some(b"b"), "a"),
    );

    let relations: Vec<VersionRelation> = index
        .history(&u)
        .iter()
        .map(|s| s.version_relation)
        .collect();
    assert_eq!(
        relations,
        vec![
            VersionRelation::Baseline,
            VersionRelation::Unknown,
            VersionRelation::Unchanged,
            VersionRelation::Changed,
            VersionRelation::Unchanged,
        ]
    );
    let version_times: Vec<u64> = index
        .distinct_versions(&u)
        .iter()
        .map(|s| s.observation.observed_at_ms)
        .collect();
    assert_eq!(version_times, vec![100, 400]);
}

#[test]
fn same_timestamp_digest_disagreement_is_not_promoted_to_a_change() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        "disagree-a",
        observation("disagree-a", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        "disagree-b",
        observation("disagree-b", u.clone(), 100, Some(b"b"), "b"),
    );

    assert_eq!(index.disagreements(&u).len(), 2);
    assert!(index
        .history(&u)
        .iter()
        .all(|s| s.version_relation == VersionRelation::SameTimestampDisagreement));
    assert!(index.distinct_versions(&u).is_empty());

    insert(
        &mut index,
        "later",
        observation("later", u.clone(), 200, Some(b"c"), "c"),
    );
    assert_eq!(
        index.history(&u)[2].version_relation,
        VersionRelation::Baseline
    );
}

#[test]
fn same_timestamp_corroboration_collapses_to_one_version_representative() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        "same-a",
        observation("same-a", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        "same-b",
        observation("same-b", u.clone(), 100, Some(b"a"), "b"),
    );

    assert_eq!(index.latest(&u).len(), 2);
    assert!(index.disagreements(&u).is_empty());
    assert_eq!(index.distinct_versions(&u).len(), 1);
}

#[test]
fn different_final_urls_have_independent_histories() {
    let mut index = SnapshotIndex::new();
    let a = url("a.example", "/");
    let b = url("b.example", "/");
    insert(
        &mut index,
        "a1",
        observation("a1", a.clone(), 100, Some(b"x"), "a"),
    );
    insert(
        &mut index,
        "b1",
        observation("b1", b.clone(), 100, Some(b"y"), "b"),
    );

    assert_eq!(index.history(&a).len(), 1);
    assert_eq!(index.history(&b).len(), 1);
    assert_ne!(
        index.history(&a)[0].object_id,
        index.history(&b)[0].object_id
    );
}

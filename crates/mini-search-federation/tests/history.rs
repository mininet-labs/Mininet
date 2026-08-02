//! Integration tests for Track F7's bounded, provenance-preserving local
//! history over canonical F1 crawl-observation objects.

use did_mini::{Capabilities, Controller};
use mini_crypto::{HashAlgorithm, Multihash};
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_search_federation::{
    publish_crawl_observation, FederationError, SnapshotIndex, SnapshotInsert, SnapshotLimits,
    VersionRelation, CRAWL_OBSERVATION_TYPE,
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

fn published(root: &did_mini::Did, device: &Controller, observation: &CrawlObservation) -> Object {
    let mut store = Store::new(MemoryBackend::new());
    let id = publish_crawl_observation(&mut store, root, device, observation).unwrap();
    store.get(&id).unwrap()
}

fn insert(
    index: &mut SnapshotIndex,
    root: &did_mini::Did,
    device: &Controller,
    observation: CrawlObservation,
) -> SnapshotInsert {
    let object = published(root, device, &observation);
    index.insert_observation(&object).unwrap()
}

fn count_limits(
    max_urls: usize,
    max_snapshots_per_url: usize,
    max_total_snapshots: usize,
) -> SnapshotLimits {
    SnapshotLimits {
        max_urls,
        max_snapshots_per_url,
        max_total_snapshots,
        max_snapshot_wire_bytes: usize::MAX,
        max_total_snapshot_wire_bytes: usize::MAX,
    }
}

#[test]
fn history_is_empty_for_an_unrecorded_url() {
    let index = SnapshotIndex::new();
    let u = url("example.org", "/");
    assert!(index.history(&u).is_empty());
    assert!(index.latest(&u).is_empty());
    assert!(index.at_or_before(&u, 100).is_empty());
    assert_eq!(index.len(), 0);
    assert_eq!(index.total_wire_bytes(), 0);
    assert!(index.is_empty());
}

#[test]
fn insertion_derives_identity_and_fields_from_the_canonical_object() {
    let (root, device) = human(30);
    let mut index = SnapshotIndex::new();
    let requested = url("alias.example", "/old");
    let final_url = url("example.org", "/new");
    let mut observed = observation("redirected", final_url.clone(), 100, Some(b"body"), "a");
    observed.requested_url = requested.clone();
    observed.redirect_chain = vec![final_url.clone()];
    let expected = observed.clone();
    let object = published(&root, &device, &observed);

    assert_eq!(
        index.insert_observation(&object).unwrap(),
        SnapshotInsert::Inserted
    );
    assert!(index.history(&requested).is_empty());
    let snapshot = &index.history(&final_url)[0];
    assert_eq!(&snapshot.object_id, object.id());
    assert_eq!(snapshot.observation, expected);
    assert!(snapshot.wire_bytes > 0);
    assert_eq!(index.total_wire_bytes(), snapshot.wire_bytes);
    assert_eq!(index.url_count(), 1);
    assert_eq!(index.len(), 1);
}

#[test]
fn an_in_memory_object_mutated_after_signing_cannot_keep_its_stale_id() {
    let (root, device) = human(31);
    let u = url("example.org", "/");
    let mut object = published(
        &root,
        &device,
        &observation("tamper", u, 100, Some(b"body"), "a"),
    );
    if let Payload::Public(bytes) = &mut object.payload {
        bytes.push(0);
    }

    let mut index = SnapshotIndex::new();
    assert!(matches!(
        index.insert_observation(&object),
        Err(FederationError::Object(_))
    ));
    assert!(index.is_empty());
}

#[test]
fn insertion_reuses_f1_object_type_and_visibility_checks() {
    let (root, device) = human(32);
    let wrong_type = ObjectBuilder::new(ObjectType::Custom("mini/not-observation".to_string()))
        .payload(Payload::Public(Vec::new()))
        .sign(&root, &device)
        .unwrap();
    let encrypted = ObjectBuilder::new(ObjectType::Custom(CRAWL_OBSERVATION_TYPE.to_string()))
        .payload(Payload::Encrypted(vec![1, 2, 3]))
        .sign(&root, &device)
        .unwrap();

    let mut index = SnapshotIndex::new();
    assert_eq!(
        index.insert_observation(&wrong_type),
        Err(FederationError::WrongObjectType)
    );
    assert_eq!(
        index.insert_observation(&encrypted),
        Err(FederationError::NotPublicPayload)
    );
    assert!(index.is_empty());
}

#[test]
fn snapshots_are_oldest_first_regardless_of_insertion_order() {
    let (root, device) = human(33);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        &root,
        &device,
        observation("v3", u.clone(), 300, Some(b"c"), "a"),
    );
    insert(
        &mut index,
        &root,
        &device,
        observation("v1", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        &root,
        &device,
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
fn inserting_the_same_canonical_object_is_idempotent() {
    let (root, device) = human(34);
    let u = url("example.org", "/");
    let object = published(
        &root,
        &device,
        &observation("v1", u.clone(), 100, Some(b"a"), "a"),
    );
    let mut index = SnapshotIndex::new();

    assert_eq!(
        index.insert_observation(&object).unwrap(),
        SnapshotInsert::Inserted
    );
    let wire_bytes = index.total_wire_bytes();
    assert_eq!(
        index.insert_observation(&object).unwrap(),
        SnapshotInsert::AlreadyPresent
    );
    assert_eq!(index.history(&u).len(), 1);
    assert_eq!(index.len(), 1);
    assert_eq!(index.total_wire_bytes(), wire_bytes);
}

#[test]
fn explicit_url_per_url_and_total_count_limits_fail_closed() {
    let (root, device) = human(35);
    let a = url("a.example", "/");
    let b = url("b.example", "/");

    let mut url_limited = SnapshotIndex::with_limits(count_limits(1, 4, 4));
    insert(
        &mut url_limited,
        &root,
        &device,
        observation("a1", a.clone(), 100, Some(b"a"), "a"),
    );
    let b1 = published(
        &root,
        &device,
        &observation("b1", b.clone(), 100, Some(b"b"), "b"),
    );
    assert_eq!(
        url_limited.insert_observation(&b1),
        Err(FederationError::LimitExceeded)
    );

    let mut per_url_limited = SnapshotIndex::with_limits(count_limits(2, 1, 4));
    insert(
        &mut per_url_limited,
        &root,
        &device,
        observation("a1-per-url", a.clone(), 100, Some(b"a"), "a"),
    );
    let a2 = published(
        &root,
        &device,
        &observation("a2-per-url", a.clone(), 200, Some(b"b"), "a"),
    );
    assert_eq!(
        per_url_limited.insert_observation(&a2),
        Err(FederationError::LimitExceeded)
    );

    let mut total_limited = SnapshotIndex::with_limits(count_limits(2, 2, 1));
    insert(
        &mut total_limited,
        &root,
        &device,
        observation("a1-total", a, 100, Some(b"a"), "a"),
    );
    let b_total = published(
        &root,
        &device,
        &observation("b1-total", b, 100, Some(b"b"), "b"),
    );
    assert_eq!(
        total_limited.insert_observation(&b_total),
        Err(FederationError::LimitExceeded)
    );
}

#[test]
fn per_snapshot_and_total_wire_byte_limits_fail_closed() {
    let (root, device) = human(36);
    let u = url("example.org", "/");
    let first = published(
        &root,
        &device,
        &observation("first", u.clone(), 100, Some(b"a"), "a"),
    );
    let second = published(
        &root,
        &device,
        &observation("second", u.clone(), 200, Some(b"b"), "b"),
    );

    let mut probe = SnapshotIndex::new();
    probe.insert_observation(&first).unwrap();
    let one_observation_bytes = probe.history(&u)[0].wire_bytes;
    assert!(one_observation_bytes > 1);

    let mut per_snapshot_limited = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 1,
        max_snapshots_per_url: 2,
        max_total_snapshots: 2,
        max_snapshot_wire_bytes: one_observation_bytes - 1,
        max_total_snapshot_wire_bytes: usize::MAX,
    });
    assert_eq!(
        per_snapshot_limited.insert_observation(&first),
        Err(FederationError::LimitExceeded)
    );
    assert!(per_snapshot_limited.is_empty());

    let mut total_limited = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 1,
        max_snapshots_per_url: 2,
        max_total_snapshots: 2,
        max_snapshot_wire_bytes: usize::MAX,
        max_total_snapshot_wire_bytes: one_observation_bytes,
    });
    total_limited.insert_observation(&first).unwrap();
    let retained_bytes = total_limited.total_wire_bytes();
    assert_eq!(
        total_limited.insert_observation(&second),
        Err(FederationError::LimitExceeded)
    );
    assert_eq!(total_limited.len(), 1);
    assert_eq!(total_limited.total_wire_bytes(), retained_bytes);
}

#[test]
fn zero_limits_reject_the_first_insertion() {
    let (root, device) = human(37);
    let mut index = SnapshotIndex::with_limits(SnapshotLimits {
        max_urls: 0,
        max_snapshots_per_url: 0,
        max_total_snapshots: 0,
        max_snapshot_wire_bytes: 0,
        max_total_snapshot_wire_bytes: 0,
    });
    let object = published(
        &root,
        &device,
        &observation(
            "blocked",
            url("example.org", "/"),
            100,
            Some(b"a"),
            "a",
        ),
    );
    assert_eq!(
        index.insert_observation(&object),
        Err(FederationError::LimitExceeded)
    );
}

#[test]
fn latest_and_at_or_before_return_the_whole_equal_timestamp_group() {
    let (root, device) = human(38);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for observed in [
        observation("early", u.clone(), 100, Some(b"old"), "a"),
        observation("late-a", u.clone(), 200, Some(b"new"), "a"),
        observation("late-b", u.clone(), 200, Some(b"new"), "b"),
    ] {
        insert(&mut index, &root, &device, observed);
    }

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
    let (root, device) = human(39);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for (seed, time, body) in [("v1", 100, b"a"), ("v2", 200, b"b"), ("v3", 300, b"c")] {
        insert(
            &mut index,
            &root,
            &device,
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
    let (root, device) = human(40);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for observed in [
        observation("v1", u.clone(), 100, Some(b"a"), "a"),
        observation("unknown", u.clone(), 200, None, "a"),
        observation("v1-again", u.clone(), 300, Some(b"a"), "a"),
        observation("v2", u.clone(), 400, Some(b"b"), "a"),
    ] {
        insert(&mut index, &root, &device, observed);
    }

    let history = index.history(&u);
    assert_eq!(history[0].version_relation, VersionRelation::Baseline);
    assert_eq!(history[1].version_relation, VersionRelation::Unknown);
    assert_eq!(history[2].version_relation, VersionRelation::Unchanged);
    assert_eq!(history[3].version_relation, VersionRelation::Changed);
    assert_eq!(
        index
            .distinct_versions(&u)
            .iter()
            .map(|s| s.observation.observed_at_ms)
            .collect::<Vec<_>>(),
        vec![100, 400]
    );
}

#[test]
fn same_timestamp_disagreement_is_visible_and_not_a_fake_change_order() {
    let (root, device) = human(41);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for observed in [
        observation("base", u.clone(), 100, Some(b"a"), "a"),
        observation("disagree-a", u.clone(), 200, Some(b"b"), "a"),
        observation("disagree-b", u.clone(), 200, Some(b"c"), "b"),
    ] {
        insert(&mut index, &root, &device, observed);
    }

    assert_eq!(index.disagreements(&u).len(), 2);
    assert!(index.history(&u)[1..]
        .iter()
        .all(|s| s.version_relation == VersionRelation::SameTimestampDisagreement));
    assert_eq!(index.distinct_versions(&u).len(), 1);
}

#[test]
fn same_timestamp_corroboration_collapses_to_one_version_representative() {
    let (root, device) = human(42);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    insert(
        &mut index,
        &root,
        &device,
        observation("same-a", u.clone(), 100, Some(b"a"), "a"),
    );
    insert(
        &mut index,
        &root,
        &device,
        observation("same-b", u.clone(), 100, Some(b"a"), "b"),
    );

    assert!(index
        .history(&u)
        .iter()
        .all(|s| s.version_relation == VersionRelation::Baseline));
    assert_eq!(index.distinct_versions(&u).len(), 1);
    assert!(index.disagreements(&u).is_empty());
}

#[test]
fn a_later_agreed_digest_compares_to_the_last_earlier_agreed_digest() {
    let (root, device) = human(43);
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    for observed in [
        observation("base", u.clone(), 100, Some(b"a"), "a"),
        observation("disagree-a", u.clone(), 200, Some(b"b"), "a"),
        observation("disagree-b", u.clone(), 200, Some(b"c"), "b"),
        observation("later", u.clone(), 300, Some(b"b"), "a"),
    ] {
        insert(&mut index, &root, &device, observed);
    }

    assert_eq!(
        index.history(&u).last().unwrap().version_relation,
        VersionRelation::Changed
    );
}

#[test]
fn different_final_urls_have_independent_histories() {
    let (root, device) = human(44);
    let mut index = SnapshotIndex::new();
    let a = url("a.example", "/");
    let b = url("b.example", "/");
    insert(
        &mut index,
        &root,
        &device,
        observation("a1", a.clone(), 100, Some(b"x"), "a"),
    );
    insert(
        &mut index,
        &root,
        &device,
        observation("b1", b.clone(), 100, Some(b"y"), "b"),
    );

    assert_eq!(index.history(&a).len(), 1);
    assert_eq!(index.history(&b).len(), 1);
    assert_ne!(
        index.history(&a)[0].object_id,
        index.history(&b)[0].object_id
    );
}

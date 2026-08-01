//! Integration tests for Track F7: local snapshot history indexing over
//! observations recorded via F1.

use mini_crypto::{HashAlgorithm, Multihash};
use mini_objects::ObjectId;
use mini_search_federation::SnapshotIndex;
use mini_web_types::{CanonicalUrl, NormalizedHost, Scheme};

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

#[test]
fn history_is_empty_for_an_unrecorded_url() {
    let index = SnapshotIndex::new();
    assert!(index.history(&url("example.org", "/")).is_empty());
    assert!(index.latest(&url("example.org", "/")).is_none());
}

#[test]
fn snapshots_are_returned_oldest_first_regardless_of_insertion_order() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    index.insert_observation(&u, object_id("v3"), 300, Some(digest(b"c")));
    index.insert_observation(&u, object_id("v1"), 100, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v2"), 200, Some(digest(b"b")));

    let history = index.history(&u);
    let times: Vec<u64> = history.iter().map(|s| s.observed_at_ms).collect();
    assert_eq!(times, vec![100, 200, 300]);
}

#[test]
fn inserting_the_same_object_id_twice_is_idempotent() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    let id = object_id("v1");
    index.insert_observation(&u, id.clone(), 100, Some(digest(b"a")));
    index.insert_observation(&u, id, 100, Some(digest(b"a")));
    assert_eq!(index.history(&u).len(), 1);
}

#[test]
fn latest_returns_the_most_recent_snapshot() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    index.insert_observation(&u, object_id("v1"), 100, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v2"), 200, Some(digest(b"b")));
    assert_eq!(index.latest(&u).unwrap().observed_at_ms, 200);
}

#[test]
fn at_or_before_finds_the_state_at_a_point_in_time() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    index.insert_observation(&u, object_id("v1"), 100, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v2"), 200, Some(digest(b"b")));
    index.insert_observation(&u, object_id("v3"), 300, Some(digest(b"c")));

    assert_eq!(index.at_or_before(&u, 250).unwrap().observed_at_ms, 200);
    assert_eq!(index.at_or_before(&u, 300).unwrap().observed_at_ms, 300);
    assert!(index.at_or_before(&u, 50).is_none());
}

#[test]
fn between_bounds_are_inclusive_lower_exclusive_upper() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    index.insert_observation(&u, object_id("v1"), 100, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v2"), 200, Some(digest(b"b")));
    index.insert_observation(&u, object_id("v3"), 300, Some(digest(b"c")));

    let in_range = index.between(&u, Some(100), Some(300));
    let times: Vec<u64> = in_range.iter().map(|s| s.observed_at_ms).collect();
    assert_eq!(times, vec![100, 200]);

    let no_lower = index.between(&u, None, Some(200));
    assert_eq!(
        no_lower
            .iter()
            .map(|s| s.observed_at_ms)
            .collect::<Vec<_>>(),
        vec![100]
    );

    let no_upper = index.between(&u, Some(200), None);
    assert_eq!(
        no_upper
            .iter()
            .map(|s| s.observed_at_ms)
            .collect::<Vec<_>>(),
        vec![200, 300]
    );
}

#[test]
fn distinct_versions_skips_repeat_fetches_of_unchanged_content() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    // v1: baseline. v2: identical content (re-fetch, no change). v3: real
    // change. v4: identical to v3 (another unchanged re-fetch).
    index.insert_observation(&u, object_id("v1"), 100, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v2"), 200, Some(digest(b"a")));
    index.insert_observation(&u, object_id("v3"), 300, Some(digest(b"b")));
    index.insert_observation(&u, object_id("v4"), 400, Some(digest(b"b")));

    let versions = index.distinct_versions(&u);
    let times: Vec<u64> = versions.iter().map(|s| s.observed_at_ms).collect();
    assert_eq!(times, vec![100, 300]);

    let full = index.history(&u);
    assert!(full[0].content_changed);
    assert!(!full[1].content_changed);
    assert!(full[2].content_changed);
    assert!(!full[3].content_changed);
}

#[test]
fn two_consecutive_unknown_digests_are_not_treated_as_a_change() {
    let mut index = SnapshotIndex::new();
    let u = url("example.org", "/");
    index.insert_observation(&u, object_id("v1"), 100, None);
    index.insert_observation(&u, object_id("v2"), 200, None);

    let full = index.history(&u);
    assert!(full[0].content_changed); // first snapshot always counts
    assert!(!full[1].content_changed); // None == None, no signal
}

#[test]
fn different_urls_have_independent_histories() {
    let mut index = SnapshotIndex::new();
    let a = url("a.example", "/");
    let b = url("b.example", "/");
    index.insert_observation(&a, object_id("a1"), 100, Some(digest(b"x")));
    index.insert_observation(&b, object_id("b1"), 100, Some(digest(b"y")));

    assert_eq!(index.history(&a).len(), 1);
    assert_eq!(index.history(&b).len(), 1);
    assert_ne!(
        index.history(&a)[0].object_id,
        index.history(&b)[0].object_id
    );
}

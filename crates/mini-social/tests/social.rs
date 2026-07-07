//! Integration tests: profile editing through heads, follow/unfollow LWW in any
//! order, the reverse graph, and a feed that is chronological, explainable, and
//! never silently drops followed speech.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_store::{MemoryBackend, Store};
use mini_social::{
    feed, followers, following, publish_profile, resolve_profile, set_follow, FeedFilter,
    FeedReason,
};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary()).unwrap();
    (root, device)
}

fn post(store: &mut Store<MemoryBackend>, h: &Did, d: &Controller, text: &[u8], ts: u64, seq: u64) -> mini_objects::Object {
    let o = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(ts)
        .sequence(seq)
        .payload(Payload::Public(text.to_vec()))
        .sign(h, d)
        .unwrap();
    store.insert(&o).unwrap();
    o
}

#[test]
fn profile_publishes_and_edits_resolve_latest() {
    let (root, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());

    publish_profile(&mut store, &root.did(), &device, "Ada", "first bio", None, 100, 1).unwrap();
    publish_profile(&mut store, &root.did(), &device, "Ada L.", "second bio", None, 200, 2).unwrap();

    let p = resolve_profile(&store, &root.did()).unwrap().unwrap();
    assert_eq!(p.display_name, "Ada L.");
    assert_eq!(p.bio, "second bio");
    assert_eq!(p.human.as_str(), root.did().as_str());

    // Unknown human: no profile, no error.
    let (other, _) = human(90);
    assert!(resolve_profile(&store, &other.did()).unwrap().is_none());
}

#[test]
fn follow_unfollow_is_lww_in_any_order() {
    let (a_root, a_dev) = human(10);
    let (b_root, _) = human(50);
    let a = a_root.did();
    let b = b_root.did();

    // Two replicas receive the same edges in different orders.
    let mut s1 = Store::new(MemoryBackend::new());
    let mut s2 = Store::new(MemoryBackend::new());
    let f1 = set_follow(&mut s1, &a, &a_dev, &b, true, 100, 1).unwrap();
    let f2 = set_follow(&mut s1, &a, &a_dev, &b, false, 200, 2).unwrap();
    // Replay onto s2 in reverse arrival order.
    s2.insert(&f2).unwrap();
    s2.insert(&f1).unwrap();

    assert!(following(&s1, &a).unwrap().is_empty());
    assert!(following(&s2, &a).unwrap().is_empty());

    // Re-follow with a later edge wins everywhere.
    let f3 = set_follow(&mut s1, &a, &a_dev, &b, true, 300, 3).unwrap();
    s2.insert(&f3).unwrap();
    assert_eq!(following(&s1, &a).unwrap().len(), 1);
    assert_eq!(following(&s2, &a).unwrap().len(), 1);
}

#[test]
fn followers_is_the_reverse_graph() {
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let (c_root, c_dev) = human(90);
    let mut store = Store::new(MemoryBackend::new());

    set_follow(&mut store, &a_root.did(), &a_dev, &c_root.did(), true, 1, 1).unwrap();
    set_follow(&mut store, &b_root.did(), &b_dev, &c_root.did(), true, 2, 1).unwrap();
    set_follow(&mut store, &c_root.did(), &c_dev, &a_root.did(), true, 3, 1).unwrap();

    let f = followers(&store, &c_root.did()).unwrap();
    assert_eq!(f.len(), 2);
    assert!(f.iter().any(|d| d.as_str() == a_root.did().as_str()));
    assert!(f.iter().any(|d| d.as_str() == b_root.did().as_str()));
    assert_eq!(followers(&store, &a_root.did()).unwrap().len(), 1);
}

#[test]
fn feed_is_chronological_explainable_and_follows_scoped() {
    let (a_root, a_dev) = human(10); // viewer
    let (b_root, b_dev) = human(50); // followed
    let (c_root, c_dev) = human(90); // NOT followed
    let mut store = Store::new(MemoryBackend::new());

    set_follow(&mut store, &a_root.did(), &a_dev, &b_root.did(), true, 1, 1).unwrap();
    post(&mut store, &a_root.did(), &a_dev, b"mine", 300, 1);
    post(&mut store, &b_root.did(), &b_dev, b"followed old", 100, 1);
    post(&mut store, &b_root.did(), &b_dev, b"followed new", 400, 2);
    post(&mut store, &c_root.did(), &c_dev, b"stranger", 500, 1);

    let items = feed(&store, &a_root.did(), FeedFilter::Chronological, 10).unwrap();
    // Stranger content is out of scope (no follow edge), not hidden by ranking.
    assert_eq!(items.len(), 3);
    // Newest first.
    assert_eq!(items[0].timestamp_ms, 400);
    assert_eq!(items[1].timestamp_ms, 300);
    assert_eq!(items[2].timestamp_ms, 100);
    // Every item explains itself.
    assert_eq!(items[0].reason, FeedReason::Followed);
    assert_eq!(items[1].reason, FeedReason::Own);
    // Limit truncates deterministically.
    let top1 = feed(&store, &a_root.did(), FeedFilter::Chronological, 1).unwrap();
    assert_eq!(top1.len(), 1);
    assert_eq!(top1[0].id, items[0].id);
}

#[test]
fn filter_reorders_but_never_drops_followed_speech() {
    // The [FREEZE] property in miniature: with a large enough limit, the filter
    // returns EVERY followed/own post — filters are total orderings.
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let mut store = Store::new(MemoryBackend::new());
    set_follow(&mut store, &a_root.did(), &a_dev, &b_root.did(), true, 1, 1).unwrap();
    for i in 0..20 {
        post(&mut store, &b_root.did(), &b_dev, format!("b{i}").as_bytes(), 100 + i, i);
    }
    let items = feed(&store, &a_root.did(), FeedFilter::Chronological, usize::MAX).unwrap();
    assert_eq!(items.len(), 20);
}

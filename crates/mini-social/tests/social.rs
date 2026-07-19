//! Integration tests: profile editing through heads, follow/unfollow LWW in any
//! order, the reverse graph, and a feed that is chronological, explainable, and
//! never silently drops followed speech.

use did_mini::{Capabilities, Controller, Did};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_social::{
    comments, community_members, feed, followers, following, publish_comment, publish_community,
    publish_profile, publish_wall, publish_wall_linkage, reaction_counts, resolve_community,
    resolve_profile, resolve_wall, resolve_wall_linkage, set_follow, set_membership, set_reaction,
    FeedFilter, FeedReason, MembershipMode, ReactionKind, SocialError, VisibilityPolicy,
};
use mini_store::{MemoryBackend, Store};

fn human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// A human whose only delegated device holds the *secondary* capability set
/// (no `VOTE`, no `MANAGE_DEVICES`) — used to prove wall publication never
/// needs governance authority.
fn human_no_vote(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::secondary())
        .unwrap();
    (root, device)
}

fn post(
    store: &mut Store<MemoryBackend>,
    h: &Did,
    d: &Controller,
    text: &[u8],
    ts: u64,
    seq: u64,
) -> mini_objects::Object {
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

    publish_profile(
        &mut store,
        &root.did(),
        &device,
        "Ada",
        "first bio",
        None,
        100,
        1,
    )
    .unwrap();
    publish_profile(
        &mut store,
        &root.did(),
        &device,
        "Ada L.",
        "second bio",
        None,
        200,
        2,
    )
    .unwrap();

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
fn comments_are_threaded_and_reactions_converge() {
    let (author, author_device) = human(10);
    let (reply_author, reply_device) = human(50);
    let mut store = Store::new(MemoryBackend::new());
    let root = post(&mut store, &author.did(), &author_device, b"hello", 1, 1);

    let first = publish_comment(
        &mut store,
        &reply_author.did(),
        &reply_device,
        root.id(),
        "first",
        20,
        1,
    )
    .unwrap();
    publish_comment(
        &mut store,
        &author.did(),
        &author_device,
        first.id(),
        "nested",
        30,
        2,
    )
    .unwrap();
    assert_eq!(comments(&store, root.id()).unwrap()[0].text, "first");
    assert_eq!(comments(&store, first.id()).unwrap()[0].text, "nested");

    set_reaction(
        &mut store,
        &reply_author.did(),
        &reply_device,
        root.id(),
        ReactionKind::Like,
        true,
        40,
        1,
    )
    .unwrap();
    set_reaction(
        &mut store,
        &reply_author.did(),
        &reply_device,
        root.id(),
        ReactionKind::Like,
        false,
        50,
        2,
    )
    .unwrap();
    set_reaction(
        &mut store,
        &author.did(),
        &author_device,
        root.id(),
        ReactionKind::Love,
        true,
        60,
        1,
    )
    .unwrap();
    assert_eq!(
        reaction_counts(&store, root.id()).unwrap(),
        vec![(ReactionKind::Love, 1)]
    );
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
        post(
            &mut store,
            &b_root.did(),
            &b_dev,
            format!("b{i}").as_bytes(),
            100 + i,
            i,
        );
    }
    let items = feed(&store, &a_root.did(), FeedFilter::Chronological, usize::MAX).unwrap();
    assert_eq!(items.len(), 20);
}

#[test]
fn communities_threads_and_reactions_form_one_composable_surface() {
    let (a_root, a_dev) = human(10);
    let (b_root, b_dev) = human(50);
    let mut store = Store::new(MemoryBackend::new());

    let community = publish_community(
        &mut store,
        &a_root.did(),
        &a_dev,
        "Rust builders",
        "Share practical systems work.",
        MembershipMode::Open,
        1,
        1,
    )
    .unwrap();
    assert_eq!(
        resolve_community(&store, community.id()).unwrap().name,
        "Rust builders"
    );

    set_membership(
        &mut store,
        &b_root.did(),
        &b_dev,
        community.id(),
        true,
        2,
        1,
    )
    .unwrap();
    set_membership(
        &mut store,
        &b_root.did(),
        &b_dev,
        community.id(),
        false,
        3,
        2,
    )
    .unwrap();
    assert!(community_members(&store, community.id())
        .unwrap()
        .is_empty());

    let post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(10)
        .sequence(1)
        .link("community", community.id().clone())
        .payload(Payload::Public(b"a post".to_vec()))
        .sign(&a_root.did(), &a_dev)
        .unwrap();
    store.insert(&post).unwrap();
    let comment = publish_comment(
        &mut store,
        &b_root.did(),
        &b_dev,
        post.id(),
        "a useful reply",
        20,
        1,
    )
    .unwrap();
    assert_eq!(comments(&store, post.id()).unwrap()[0].id, *comment.id());

    set_reaction(
        &mut store,
        &b_root.did(),
        &b_dev,
        post.id(),
        ReactionKind::Upvote,
        true,
        30,
        1,
    )
    .unwrap();
    assert_eq!(
        reaction_counts(&store, post.id()).unwrap(),
        vec![(ReactionKind::Upvote, 1)]
    );
}

#[test]
fn public_wall_publishes_and_edits_resolve_latest_with_no_human_root_field() {
    let (owner, device) = human(10);
    let mut store = Store::new(MemoryBackend::new());

    publish_wall(
        &mut store,
        &owner.did(),
        &device,
        "Ada's Wall",
        "first bio",
        None,
        &["https://example.org"],
        &[],
        VisibilityPolicy::Public,
        100,
        1,
    )
    .unwrap();
    publish_wall(
        &mut store,
        &owner.did(),
        &device,
        "Ada's Wall v2",
        "second bio",
        None,
        &["https://example.org", "https://example.net"],
        &[],
        VisibilityPolicy::Unlisted,
        200,
        2,
    )
    .unwrap();

    let w = resolve_wall(&store, &owner.did()).unwrap().unwrap();
    assert_eq!(w.display_name, "Ada's Wall v2");
    assert_eq!(w.bio, "second bio");
    assert_eq!(w.public_links.len(), 2);
    assert_eq!(w.visibility, VisibilityPolicy::Unlisted);
    assert_eq!(w.owner.as_str(), owner.did().as_str());
    // `PublicWall` has no human-root field at all — the only DID it carries
    // is the DID it was *published under*, which is the wall's own identity.

    // A wall owner is never auto-linked to anything: no linkage has been
    // published, so resolving one yields nothing.
    assert!(resolve_wall_linkage(&store, &owner.did())
        .unwrap()
        .is_none());
}

#[test]
fn public_wall_reveals_human_root_only_via_explicit_signed_linkage() {
    // The wall is published under an independent pseudonym root, distinct
    // from the "human_root" identity the user separately controls.
    let (pseudonym, pseudonym_device) = human(20);
    let (human_root, _) = human(21);
    let mut store = Store::new(MemoryBackend::new());

    publish_wall(
        &mut store,
        &pseudonym.did(),
        &pseudonym_device,
        "Anon",
        "just a pseudonym",
        None,
        &[],
        &[],
        VisibilityPolicy::Public,
        100,
        1,
    )
    .unwrap();

    // By default, nothing connects the wall to the human root.
    assert!(resolve_wall_linkage(&store, &pseudonym.did())
        .unwrap()
        .is_none());

    // The user explicitly, voluntarily publishes the linkage themselves.
    publish_wall_linkage(
        &mut store,
        &pseudonym.did(),
        &pseudonym_device,
        &human_root.did(),
        150,
        1,
    )
    .unwrap();

    let linked = resolve_wall_linkage(&store, &pseudonym.did())
        .unwrap()
        .unwrap();
    assert_eq!(linked.as_str(), human_root.did().as_str());
}

#[test]
fn public_wall_never_needs_or_implies_a_vote_capability() {
    // A device holding only the secondary capability set (no VOTE, no
    // MANAGE_DEVICES) can still fully publish a wall — proving wall
    // publication requires nothing beyond ordinary POST authority and can
    // never itself grant or require governance standing.
    let (owner, device) = human_no_vote(30);
    let mut store = Store::new(MemoryBackend::new());

    let result = publish_wall(
        &mut store,
        &owner.did(),
        &device,
        "No Vote Needed",
        "",
        None,
        &[],
        &[],
        VisibilityPolicy::Public,
        100,
        1,
    );
    assert!(result.is_ok());
    assert!(resolve_wall(&store, &owner.did()).unwrap().is_some());
}

#[test]
fn multiple_walls_are_unlinkable_by_default_and_unknown_walls_are_not_registered() {
    // One human privately runs two independent pseudonym roots and publishes
    // a wall under each. Nothing in the protocol connects them.
    let (wall_a, dev_a) = human(40);
    let (wall_b, dev_b) = human(41);
    let mut store = Store::new(MemoryBackend::new());

    publish_wall(
        &mut store,
        &wall_a.did(),
        &dev_a,
        "Wall A",
        "",
        None,
        &[],
        &[],
        VisibilityPolicy::Public,
        100,
        1,
    )
    .unwrap();
    publish_wall(
        &mut store,
        &wall_b.did(),
        &dev_b,
        "Wall B",
        "",
        None,
        &[],
        &[],
        VisibilityPolicy::Public,
        100,
        1,
    )
    .unwrap();

    let a = resolve_wall(&store, &wall_a.did()).unwrap().unwrap();
    let b = resolve_wall(&store, &wall_b.did()).unwrap().unwrap();
    assert_ne!(a.wall_id.as_str(), b.wall_id.as_str());
    assert_ne!(a.owner.as_str(), b.owner.as_str());
    // Neither wall's resolved data references the other's DID anywhere.
    assert!(resolve_wall_linkage(&store, &wall_a.did())
        .unwrap()
        .is_none());
    assert!(resolve_wall_linkage(&store, &wall_b.did())
        .unwrap()
        .is_none());

    // A DID that never published a wall simply resolves to nothing — there
    // is no implicit registration/creation of "human status" on lookup.
    let (never_published, _) = human(42);
    assert!(resolve_wall(&store, &never_published.did())
        .unwrap()
        .is_none());
}

#[test]
fn resolve_wall_rejects_a_wall_head_pointing_at_a_non_wall_object() {
    // A confused/adversarial "wall" head names the right subject string but
    // targets an ordinary POST object, not a WALL object. resolve_wall must
    // reject this rather than silently misparsing the POST's bytes as wall
    // fields.
    let (owner, device) = human(60);
    let mut store = Store::new(MemoryBackend::new());

    let decoy = post(&mut store, &owner.did(), &device, b"not a wall", 100, 1);
    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(200)
        .sequence(1)
        .link("target", decoy.id().clone())
        .payload(Payload::Public(b"wall".to_vec()))
        .sign(&owner.did(), &device)
        .unwrap();
    store.apply_head(&head).unwrap();

    assert!(matches!(
        resolve_wall(&store, &owner.did()),
        Err(SocialError::BadWall)
    ));
}

#[test]
fn resolve_wall_linkage_rejects_a_linkage_head_pointing_at_a_non_linkage_object() {
    let (owner, device) = human(61);
    let mut store = Store::new(MemoryBackend::new());

    let decoy = post(&mut store, &owner.did(), &device, b"not a linkage", 100, 1);
    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(200)
        .sequence(1)
        .link("target", decoy.id().clone())
        .payload(Payload::Public(b"wall-linkage".to_vec()))
        .sign(&owner.did(), &device)
        .unwrap();
    store.apply_head(&head).unwrap();

    assert!(matches!(
        resolve_wall_linkage(&store, &owner.did()),
        Err(SocialError::BadWall)
    ));
}

#[test]
fn resolve_wall_rejects_a_wall_head_pointing_at_a_profile_object() {
    // Same confusion, but against another WELL_KNOWN type (PROFILE) that also
    // decodes as {name, bio, avatar} — close enough to a WALL payload shape
    // that a naive implementation might "successfully" misparse it. The
    // object_type guard must still catch it.
    let (owner, device) = human(62);
    let mut store = Store::new(MemoryBackend::new());

    publish_profile(
        &mut store,
        &owner.did(),
        &device,
        "Ada",
        "bio",
        None,
        100,
        1,
    )
    .unwrap();
    let profile_target = store
        .resolve_head(&owner.did(), "profile")
        .unwrap()
        .unwrap();

    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(200)
        .sequence(1)
        .link("target", profile_target)
        .payload(Payload::Public(b"wall".to_vec()))
        .sign(&owner.did(), &device)
        .unwrap();
    store.apply_head(&head).unwrap();

    assert!(matches!(
        resolve_wall(&store, &owner.did()),
        Err(SocialError::BadWall)
    ));
}

#[test]
fn resolve_profile_rejects_wrong_type_cross_author_and_trailing_payload() {
    let (owner, owner_device) = human(63);
    let (other, other_device) = human(64);
    let mut store = Store::new(MemoryBackend::new());

    let decoy = post(
        &mut store,
        &owner.did(),
        &owner_device,
        b"not a profile",
        100,
        1,
    );
    let wrong_type_head = ObjectBuilder::new(ObjectType::HEAD)
        .sequence(1)
        .link("target", decoy.id().clone())
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&owner.did(), &owner_device)
        .unwrap();
    store.apply_head(&wrong_type_head).unwrap();
    assert!(matches!(
        resolve_profile(&store, &owner.did()),
        Err(SocialError::BadProfile)
    ));

    let other_profile = publish_profile(
        &mut store,
        &other.did(),
        &other_device,
        "Other",
        "bio",
        None,
        200,
        1,
    )
    .unwrap();
    let cross_author_head = ObjectBuilder::new(ObjectType::HEAD)
        .sequence(2)
        .link("target", other_profile.id().clone())
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&owner.did(), &owner_device)
        .unwrap();
    store.apply_head(&cross_author_head).unwrap();
    assert!(matches!(
        resolve_profile(&store, &owner.did()),
        Err(SocialError::BadProfile)
    ));

    let mut payload = Vec::new();
    for field in ["Owner", "bio", ""] {
        payload.extend_from_slice(&(field.len() as u32).to_be_bytes());
        payload.extend_from_slice(field.as_bytes());
    }
    payload.push(0xff);
    let trailing = ObjectBuilder::new(ObjectType::PROFILE)
        .payload(Payload::Public(payload))
        .sign(&owner.did(), &owner_device)
        .unwrap();
    store.insert(&trailing).unwrap();
    let trailing_head = ObjectBuilder::new(ObjectType::HEAD)
        .sequence(3)
        .link("target", trailing.id().clone())
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(&owner.did(), &owner_device)
        .unwrap();
    store.apply_head(&trailing_head).unwrap();
    assert!(matches!(
        resolve_profile(&store, &owner.did()),
        Err(SocialError::BadProfile)
    ));
}

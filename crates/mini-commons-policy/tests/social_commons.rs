//! Track C3 integration proof (research doctrine §26, "PR C3 — Public
//! profile and social rights: Ensure public view/post/comment/reply/react
//! paths do not require payment").
//!
//! [`crate::commons_policy_for`] already proves, in isolation, that this
//! crate's own [`crate::PublicCommonsPolicy`] never varies with wallet
//! balance. This file closes the other half: it drives `mini-social`'s
//! real public-facing functions -- the ones `create_public_profile`,
//! `publish_public_object`, `reply_publicly`, `comment_publicly`, and
//! `react_publicly` describe -- for an identity with no funded wallet at
//! all, and shows they succeed. None of `mini-social`'s public functions
//! accept a balance, price, or payment parameter in the first place (see
//! their signatures below); this test is the operational demonstration
//! that absence is real, not merely an omission nobody exercised.

use did_mini::{Capabilities, Controller};
use mini_commons_policy::{commons_policy_for, Entitlement, WalletStanding};
use mini_objects::{ObjectBuilder, ObjectType, Payload};
use mini_social::{
    publish_comment, publish_profile, publish_wall, resolve_wall, set_reaction, ReactionKind,
    VisibilityPolicy,
};
use mini_store::{MemoryBackend, Store};

fn penniless_human(seed: u8) -> (Controller, Controller) {
    let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    let device =
        Controller::incept_device_single_from_seeds(&root.did(), &[seed + 2; 32], &[seed + 3; 32])
            .unwrap();
    root.delegate_device(&device.did(), Capabilities::primary())
        .unwrap();
    (root, device)
}

/// This test never constructs a `mini-value`/`mini-treasury` wallet, funds
/// one, or checks a balance anywhere -- the identity below has never held
/// a single micro-MINI. If any `mini-social` call below required payment,
/// it would have nothing to pay with and this test would fail to compile
/// (no such parameter exists) or fail at runtime (no such check could
/// pass).
#[test]
fn create_public_profile_publish_object_reply_comment_and_react_all_succeed_with_zero_mini() {
    let policy = commons_policy_for(WalletStanding {
        balance_micro: 0,
        governance_weight: 0,
    });
    assert_eq!(policy.create_public_profile, Entitlement::FreeProtocolRight);
    assert_eq!(policy.publish_public_object, Entitlement::FreeProtocolRight);
    assert_eq!(policy.reply_publicly, Entitlement::FreeProtocolRight);
    assert_eq!(policy.comment_publicly, Entitlement::FreeProtocolRight);
    assert_eq!(policy.react_publicly, Entitlement::FreeProtocolRight);

    let (root, device) = penniless_human(200);
    let mut store = Store::new(MemoryBackend::new());

    // create_public_profile
    publish_profile(
        &mut store,
        &root.did(),
        &device,
        "Penniless Ada",
        "a public profile, funded by nothing",
        None,
        1_000,
        1,
    )
    .expect("profile publication must not require a funded wallet");

    // publish_public_object (a wall, and an ordinary post)
    publish_wall(
        &mut store,
        &root.did(),
        &device,
        "Penniless Ada",
        "a public wall, funded by nothing",
        None,
        &[],
        &[],
        VisibilityPolicy::Public,
        1_100,
        2,
    )
    .expect("wall publication must not require a funded wallet");
    assert!(resolve_wall(&store, &root.did()).unwrap().is_some());

    let post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(1_200)
        .sequence(3)
        .payload(Payload::Public(b"hello from a zero-balance root".to_vec()))
        .sign(&root.did(), &device)
        .expect("post signing must not require a funded wallet");
    store
        .insert(&post)
        .expect("post publication must not require a funded wallet");

    // reply_publicly / comment_publicly (mini-social exposes one function,
    // publish_comment, for both a top-level reply and a nested comment --
    // the target's kind is what the doctrine calls the axis, not the call)
    let reply = publish_comment(
        &mut store,
        &root.did(),
        &device,
        post.id(),
        "a reply, funded by nothing",
        1_300,
        4,
    )
    .expect("replying must not require a funded wallet");
    publish_comment(
        &mut store,
        &root.did(),
        &device,
        reply.id(),
        "a nested comment, funded by nothing",
        1_400,
        5,
    )
    .expect("commenting must not require a funded wallet");

    // react_publicly
    set_reaction(
        &mut store,
        &root.did(),
        &device,
        post.id(),
        ReactionKind::Like,
        true,
        1_500,
        6,
    )
    .expect("reacting must not require a funded wallet");
}

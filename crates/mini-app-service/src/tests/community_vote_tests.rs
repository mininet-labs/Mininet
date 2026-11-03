use super::*;
use mini_app_protocol::{Command, ErrorCode, Reply};
use mini_social::{
    build_community, build_post, build_reaction, set_membership, MembershipMode, ReactionKind,
};

#[test]
fn a_community_vote_counts_member_ballots_and_says_standing_was_not_checked() {
    let root = temp_root("community-vote");
    let mut core = Core::open_inner(root, MemoryVault::seeded(), false).unwrap();
    core.handle(Command::UnlockIdentity).unwrap();

    // Build the board directly in the store, authored by two foreign identities.
    let a = did_mini::Controller::incept_single_from_seeds(&[70; 32], &[71; 32]).unwrap();
    let b = did_mini::Controller::incept_single_from_seeds(&[80; 32], &[81; 32]).unwrap();
    let a_dev =
        did_mini::Controller::incept_device_single_from_seeds(&a.did(), &[72; 32], &[73; 32])
            .unwrap();
    let b_dev =
        did_mini::Controller::incept_device_single_from_seeds(&b.did(), &[82; 32], &[83; 32])
            .unwrap();
    let community = build_community(
        &a.did(),
        &a_dev,
        "board",
        "charter",
        MembershipMode::Open,
        1,
        1,
    )
    .unwrap();
    core.store.insert(&community).unwrap();
    let cid = community.id().clone();
    let post = build_post(&a.did(), &a_dev, "proposal", 1, 2).unwrap();
    core.store.insert(&post).unwrap();
    let target = post.id().clone();
    for (who, dev) in [(&a, &a_dev), (&b, &b_dev)] {
        set_membership(&mut core.store, &who.did(), dev, &cid, true, 2, 1).unwrap();
    }
    core.store
        .insert(
            &build_reaction(&a.did(), &a_dev, &target, ReactionKind::Upvote, true, 5, 1).unwrap(),
        )
        .unwrap();
    core.store
        .insert(
            &build_reaction(
                &b.did(),
                &b_dev,
                &target,
                ReactionKind::Downvote,
                true,
                5,
                1,
            )
            .unwrap(),
        )
        .unwrap();

    let reply = core
        .handle(Command::CommunityVote {
            community_id: cid.as_str().into(),
            target_id: target.as_str().into(),
            closes_at_ms: 100,
        })
        .unwrap();
    assert_eq!(
        reply,
        Reply::CommunityVote {
            yes: 1,
            no: 1,
            not_counted: 0,
            standing_checked: false
        }
    );

    let bad = core
        .handle(Command::CommunityVote {
            community_id: "not-an-id".into(),
            target_id: target.as_str().into(),
            closes_at_ms: 100,
        })
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::BadRequest);
}

#[test]
fn a_downvote_is_published_idempotently_and_conflicting_reuse_is_refused() {
    let root = temp_root("downvote");
    let mut core = Core::open_inner(root, MemoryVault::seeded(), false).unwrap();
    core.handle(Command::UnlockIdentity).unwrap();
    let Reply::Published(post) = core
        .handle(Command::PublishPost {
            operation_id: "post:1".into(),
            text: "proposal".into(),
        })
        .unwrap()
    else {
        panic!("post");
    };
    let command = Command::SetDownvote {
        operation_id: "down:1".into(),
        target_id: post.object_id.clone(),
        active: true,
    };
    let first = core.handle(command.clone()).unwrap();
    let again = core.handle(command).unwrap();
    let (Reply::Published(a), Reply::Published(b)) = (first, again) else {
        panic!("published");
    };
    assert_eq!(a.object_id, b.object_id);
    assert!(b.duplicate);
    // A different payload under the same operation ID is refused.
    assert!(core
        .handle(Command::SetDownvote {
            operation_id: "down:1".into(),
            target_id: post.object_id,
            active: false,
        })
        .is_err());
}

//! Community votes: a reddit-style up/down vote on any object (a post, a
//! proposal written as a post), counted for a community at a chosen close time.
//!
//! No new wire format: ballots are the existing `Upvote`/`Like` (yes) and
//! `Downvote` (no) reactions.
//! A ballot counts only if its author is a current member of the community and
//! has **Full** standing when voting closes (see `mini-uniqueness`' `tally`);
//! a member who is not yet Full is reported as *not counted*, never
//! half-weighted. One ballot per identity root, equal weight (P2).
//!
//! Honest limits: this counts identity roots, not proven humans. Reaction
//! timestamps are declared by their author, so a ballot can be backdated to
//! slip in before the close; backdating only helps someone who was already a
//! member and already Full at close, so it cannot add weight, only a late
//! vote. Membership is read from the local store, so two peers with different
//! stores can see different results until they sync.

use did_mini::Did;
use mini_objects::{ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};
use mini_uniqueness::{tally, Standing, Tally};

use crate::{community_members, ReactionKind, Result};

/// The outcome of closing a community vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommunityVote {
    /// Counted yes/no plus how many ballots were not counted and duplicates.
    pub tally: Tally,
    /// Members who voted and were not Full at close, for display only.
    pub not_counted: u32,
}

/// Count the votes on `target` for `community` as of `closes_at_ms`.
///
/// `standing_at_close` supplies each identity root's standing at the close
/// (typically `PersonhoodPolicy::standing` over the caller's records).
pub fn community_vote<B: Backend>(
    store: &Store<B>,
    community: &ObjectId,
    target: &ObjectId,
    closes_at_ms: u64,
    standing_at_close: impl Fn(&Did) -> Standing,
) -> Result<CommunityVote> {
    let members = community_members(store, community)?;
    // Latest ballot per author, across both vote kinds, by (sequence, id).
    let mut latest: Vec<(Did, u64, String, Option<bool>)> = Vec::new();
    for id in store.linking_to(target)? {
        let object = store.get(&id)?;
        if object.object_type != ObjectType::REACTION
            || object.links.len() != 1
            || object.links[0].rel != "target"
            || object.links[0].target != *target
            || object.timestamp_ms > closes_at_ms
            || members
                .binary_search_by(|m| m.as_str().cmp(object.author_human.as_str()))
                .is_err()
        {
            continue;
        }
        let Payload::Public(bytes) = &object.payload else {
            continue;
        };
        if bytes.len() != 2 || bytes[1] > 1 {
            continue;
        }
        let in_favour = match ReactionKind::from_byte(bytes[0]) {
            // The app's ⬆ arrow publishes a Like; both mean "yes".
            Some(ReactionKind::Upvote | ReactionKind::Like) => true,
            Some(ReactionKind::Downvote) => false,
            _ => continue,
        };
        let ballot = (bytes[1] == 1).then_some(in_favour);
        let candidate = (object.sequence, object.id().as_str().to_string());
        match latest.iter_mut().find(|(a, ..)| *a == object.author_human) {
            Some((_, seq, oid, b)) if candidate > (*seq, oid.clone()) => {
                *seq = candidate.0;
                *oid = candidate.1;
                *b = ballot;
            }
            None => latest.push((object.author_human, candidate.0, candidate.1, ballot)),
            _ => {}
        }
    }
    let cast: Vec<(&Did, bool)> = latest
        .iter()
        .filter_map(|(who, _, _, ballot)| ballot.map(|b| (who, b)))
        .collect();
    let t = tally(cast, standing_at_close);
    Ok(CommunityVote {
        not_counted: t.not_counted,
        tally: t,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_community, build_post, set_membership, set_reaction, MembershipMode};
    use did_mini::Controller;
    use mini_store::MemoryBackend;

    struct Person {
        root: Controller,
        device: Controller,
    }
    fn person(seed: u8) -> Person {
        let root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        Person { root, device }
    }

    #[test]
    fn only_full_member_ballots_count_and_late_or_repeat_ballots_do_not() {
        let owner = person(10);
        let a = person(20);
        let b = person(30);
        let outsider = person(40);
        let mut store = Store::new(MemoryBackend::new());
        let community = build_community(
            &owner.root.did(),
            &owner.device,
            "board",
            "charter",
            MembershipMode::Open,
            1,
            1,
        )
        .unwrap();
        store.insert(&community).unwrap();
        let cid = community.id().clone();
        let post = build_post(&owner.root.did(), &owner.device, "proposal", 1, 2).unwrap();
        store.insert(&post).unwrap();
        let target = post.id().clone();
        for p in [&a, &b] {
            set_membership(&mut store, &p.root.did(), &p.device, &cid, true, 2, 1).unwrap();
        }
        // a: up. b: up then switches to down. outsider: up but not a member.
        set_reaction(
            &mut store,
            &a.root.did(),
            &a.device,
            &target,
            ReactionKind::Upvote,
            true,
            5,
            1,
        )
        .unwrap();
        set_reaction(
            &mut store,
            &b.root.did(),
            &b.device,
            &target,
            ReactionKind::Upvote,
            true,
            5,
            1,
        )
        .unwrap();
        set_reaction(
            &mut store,
            &b.root.did(),
            &b.device,
            &target,
            ReactionKind::Downvote,
            true,
            6,
            2,
        )
        .unwrap();
        set_reaction(
            &mut store,
            &outsider.root.did(),
            &outsider.device,
            &target,
            ReactionKind::Upvote,
            true,
            5,
            1,
        )
        .unwrap();
        // A ballot after the close.
        set_reaction(
            &mut store,
            &a.root.did(),
            &a.device,
            &target,
            ReactionKind::Downvote,
            true,
            99,
            3,
        )
        .unwrap();

        let all_full = |_: &Did| Standing::Full;
        let v = community_vote(&store, &cid, &target, 10, all_full).unwrap();
        assert_eq!((v.tally.yes, v.tally.no, v.not_counted), (1, 1, 0));

        let b_did = b.root.did();
        let b_provisional = |d: &Did| {
            if *d == b_did {
                Standing::Provisional
            } else {
                Standing::Full
            }
        };
        let v = community_vote(&store, &cid, &target, 10, b_provisional).unwrap();
        assert_eq!((v.tally.yes, v.tally.no, v.not_counted), (1, 0, 1));
    }
}

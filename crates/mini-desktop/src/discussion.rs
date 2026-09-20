//! Community discussion built from objects that already exist and already
//! replicate: a *thread* is a signed comment whose parent is the community
//! object, a *reply* is a comment on a comment, and an *upvote* is the
//! existing like reaction. No new object type, link relation or wire format
//! — `mini_social::comments` and `reaction_counts` do all the reading, so a
//! thread posted here is readable by any client that can read comments.
//!
//! Ordering is deterministic and local: threads by upvotes then time,
//! replies chronological. Depth and width are bounded so a hostile peer
//! cannot make one community view walk the whole store.

use did_mini::Did;
use mini_objects::ObjectId;
use mini_social::{comments, reaction_counts, resolve_profile, ReactionKind};
use mini_store::{Backend, Store};
use std::collections::HashMap;

/// Deepest reply level rendered; deeper replies are counted, not walked.
pub const MAX_DEPTH: usize = 6;
/// Threads shown per community and replies shown per node.
pub const MAX_THREADS: usize = 100;
pub const MAX_REPLIES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub id: ObjectId,
    pub author: String,
    pub did: String,
    pub text: String,
    pub timestamp_ms: u64,
    pub upvotes: usize,
    pub own: bool,
    pub replies: Vec<Node>,
    /// Direct replies that exist below `MAX_DEPTH` and were not walked
    /// (their own descendants are not counted).
    pub truncated: usize,
}

impl Node {
    /// This node plus every reply beneath it.
    pub fn total_replies(&self) -> usize {
        self.replies
            .iter()
            .map(|reply| 1 + reply.total_replies())
            .sum::<usize>()
            + self.truncated
    }
}

/// How threads are ordered at the top level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Top,
    New,
}

struct Loader<'a, B: Backend> {
    store: &'a Store<B>,
    viewer: &'a Did,
    muted: &'a dyn Fn(&str) -> bool,
    names: HashMap<String, String>,
}

impl<B: Backend> Loader<'_, B> {
    fn name(&mut self, author: &Did) -> Result<String, String> {
        let did = author.as_str().to_owned();
        if let Some(name) = self.names.get(&did) {
            return Ok(name.clone());
        }
        let name = resolve_profile(self.store, author)
            .map_err(|error| error.to_string())?
            .map(|profile| profile.display_name)
            .unwrap_or_else(|| "Mininet participant".into());
        self.names.insert(did, name.clone());
        Ok(name)
    }

    fn upvotes(&self, id: &ObjectId) -> Result<usize, String> {
        Ok(reaction_counts(self.store, id)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|(kind, _)| *kind == ReactionKind::Like)
            .map(|(_, count)| count)
            .sum())
    }

    fn children(
        &mut self,
        parent: &ObjectId,
        depth: usize,
        limit: usize,
    ) -> Result<(Vec<Node>, usize), String> {
        let replies = comments(self.store, parent).map_err(|error| error.to_string())?;
        if depth > MAX_DEPTH {
            return Ok((Vec::new(), replies.len()));
        }
        let mut nodes = Vec::new();
        for reply in replies.into_iter().take(limit) {
            if (self.muted)(reply.author.as_str()) {
                continue;
            }
            let (children, truncated) = self.children(&reply.id, depth + 1, MAX_REPLIES)?;
            nodes.push(Node {
                own: &reply.author == self.viewer,
                author: self.name(&reply.author)?,
                did: reply.author.as_str().to_owned(),
                upvotes: self.upvotes(&reply.id)?,
                id: reply.id,
                text: reply.text,
                timestamp_ms: reply.timestamp_ms,
                replies: children,
                truncated,
            });
        }
        Ok((nodes, 0))
    }
}

/// Load a community's threads with nested replies.
pub fn load<B: Backend>(
    store: &Store<B>,
    community: &ObjectId,
    viewer: &Did,
    order: Order,
    muted: &dyn Fn(&str) -> bool,
) -> Result<Vec<Node>, String> {
    let mut loader = Loader {
        store,
        viewer,
        muted,
        names: HashMap::new(),
    };
    let (mut threads, _) = loader.children(community, 1, MAX_THREADS)?;
    match order {
        Order::Top => threads.sort_by(|a, b| {
            b.upvotes
                .cmp(&a.upvotes)
                .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
                .then_with(|| b.id.as_str().cmp(a.id.as_str()))
        }),
        Order::New => threads.sort_by(|a, b| {
            b.timestamp_ms
                .cmp(&a.timestamp_ms)
                .then_with(|| b.id.as_str().cmp(a.id.as_str()))
        }),
    }
    Ok(threads)
}

/// The first line of a thread is its title; the rest is the body.
pub fn split_title(text: &str) -> (&str, &str) {
    match text.split_once('\n') {
        Some((title, body)) => (title.trim(), body.trim()),
        None => (text.trim(), ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_social::{
        publish_comment, publish_community, publish_profile, set_reaction, MembershipMode,
    };
    use mini_store::MemoryBackend;

    fn person(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn threads_nest_sort_by_upvotes_and_respect_mutes() {
        let mut store = Store::new(MemoryBackend::new());
        let (alice, alice_dev) = person(10);
        let (bob, bob_dev) = person(20);
        let (troll, troll_dev) = person(30);
        publish_profile(&mut store, &bob.did(), &bob_dev, "Bob", "", None, 1, 1).unwrap();
        let community = publish_community(
            &mut store,
            &alice.did(),
            &alice_dev,
            "Rustaceans",
            "be kind",
            MembershipMode::Open,
            10,
            1,
        )
        .unwrap();
        let cid = community.id().clone();
        let t1 = publish_comment(
            &mut store,
            &alice.did(),
            &alice_dev,
            &cid,
            "First\n\nbody",
            100,
            2,
        )
        .unwrap();
        let t2 = publish_comment(&mut store, &bob.did(), &bob_dev, &cid, "Second", 200, 2).unwrap();
        publish_comment(&mut store, &troll.did(), &troll_dev, &cid, "spam", 300, 1).unwrap();
        let r1 =
            publish_comment(&mut store, &bob.did(), &bob_dev, t1.id(), "reply", 150, 3).unwrap();
        publish_comment(
            &mut store,
            &alice.did(),
            &alice_dev,
            r1.id(),
            "nested",
            160,
            3,
        )
        .unwrap();
        set_reaction(
            &mut store,
            &bob.did(),
            &bob_dev,
            t2.id(),
            ReactionKind::Like,
            true,
            400,
            4,
        )
        .unwrap();

        let troll_did = troll.did().as_str().to_owned();
        let muted = move |did: &str| did == troll_did;
        let top = load(&store, &cid, &alice.did(), Order::Top, &muted).unwrap();
        assert_eq!(top.len(), 2, "muted thread hidden");
        assert_eq!(top[0].text, "Second");
        assert_eq!(top[0].upvotes, 1);
        assert_eq!(top[0].author, "Bob");
        assert_eq!(top[1].text, "First\n\nbody");
        assert!(top[1].own);
        assert_eq!(top[1].total_replies(), 2);
        assert_eq!(top[1].replies[0].text, "reply");
        assert_eq!(top[1].replies[0].replies[0].text, "nested");
        assert_eq!(top[1].replies[0].replies[0].author, "Mininet participant");

        let new = load(&store, &cid, &alice.did(), Order::New, &|_| false).unwrap();
        assert_eq!(new.len(), 3);
        assert_eq!(new[0].text, "spam");
        assert_eq!(split_title("First\n\nbody"), ("First", "body"));
        assert_eq!(split_title("Only title "), ("Only title", ""));
    }

    #[test]
    fn depth_is_bounded_and_deeper_replies_are_counted() {
        let mut store = Store::new(MemoryBackend::new());
        let (alice, dev) = person(40);
        let community = publish_community(
            &mut store,
            &alice.did(),
            &dev,
            "Deep",
            "",
            MembershipMode::Open,
            10,
            1,
        )
        .unwrap();
        let mut parent = community.id().clone();
        for level in 0..(MAX_DEPTH as u64 + 3) {
            let comment = publish_comment(
                &mut store,
                &alice.did(),
                &dev,
                &parent,
                &format!("level {level}"),
                100 + level,
                2 + level,
            )
            .unwrap();
            parent = comment.id().clone();
        }
        let threads = load(&store, community.id(), &alice.did(), Order::New, &|_| false).unwrap();
        let mut node = &threads[0];
        let mut depth = 1;
        while let Some(child) = node.replies.first() {
            node = child;
            depth += 1;
        }
        assert_eq!(depth, MAX_DEPTH);
        assert_eq!(node.truncated, 1);
        // Five walked replies plus one counted-but-unwalked direct reply.
        assert_eq!(threads[0].total_replies(), MAX_DEPTH);
    }
}

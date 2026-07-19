//! The personal social layer (SPEC-09 §6.1, UI plan E4/E5): profiles, the
//! follow graph, and feed assembly — all as pure functions over a
//! [`mini_store::Store`], offline-first.
//!
//! ## The feed is a locally computed view [FREEZE]
//!
//! A feed is **not** a stored object and **not** a server's opinion: it is
//! computed on the reader's device from the objects their overlay has seen
//! (SPEC-09 §3/§5). Ranking is a **user-chosen filter** passed explicitly to
//! [`feed`] — never a hidden algorithm — and every item carries a
//! [`FeedReason`] so "why am I seeing this" is always answerable. Filters
//! reorder; they do not silently drop followed speech (personal blocklists are
//! the *user's own* explicit choice and live in the safety layer, E9).
//!
//! ## Profiles and follows are ordinary objects
//!
//! - A **profile** is a `PROFILE` object; the latest version is resolved
//!   through a signed head pointer (`subject = "profile"`), so edits converge
//!   deterministically on every replica (`mini-store` LWW).
//! - A **follow** is a `FOLLOW` object naming a target human, with a state
//!   byte (follow/unfollow) — per (follower, target) the latest wins by
//!   `(sequence, object id)`, the same convergence rule as everywhere. The
//!   graph is derivable by anyone from public objects; private/pseudonymous
//!   graphs come with pairwise identifiers (SPEC-01 §10) later and are noted
//!   honestly, not promised early.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod wall;

pub use wall::{
    publish_wall, publish_wall_linkage, resolve_wall, resolve_wall_linkage, PublicWall,
    VisibilityPolicy, MAX_WALL_BIO_BYTES, MAX_WALL_LINKS, MAX_WALL_LINK_BYTES, MAX_WALL_NAME_BYTES,
    MAX_WALL_PINNED,
};

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Maximum display-name bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// Maximum bio bytes.
pub const MAX_BIO_BYTES: usize = 1024;
/// Maximum UTF-8 bytes in one threaded comment.
pub const MAX_COMMENT_BYTES: usize = 16 * 1024;
/// Maximum community name bytes.
pub const MAX_COMMUNITY_NAME_BYTES: usize = 96;
/// Maximum community charter bytes.
pub const MAX_COMMUNITY_CHARTER_BYTES: usize = 4096;
const MEMBERSHIP_TYPE: &str = "mini/community-membership";

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, SocialError>;

/// Why a social operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SocialError {
    /// A profile field exceeded its limit.
    FieldTooLarge,
    /// Underlying store failure.
    Store(StoreError),
    /// Object build/sign failure.
    Object(mini_objects::ObjectError),
    /// Identity failure.
    Identity(did_mini::IdentityError),
    /// A wall or wall-linkage object was structurally invalid.
    BadWall,
    /// A profile object was structurally invalid or not owned by the requested DID.
    BadProfile,
    /// A comment or reaction object was structurally invalid.
    BadInteraction,
    /// A community or membership object was structurally invalid.
    BadCommunity,
}

impl core::fmt::Display for SocialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SocialError::FieldTooLarge => write!(f, "profile field too large"),
            SocialError::Store(e) => write!(f, "store: {e}"),
            SocialError::Object(e) => write!(f, "object: {e}"),
            SocialError::Identity(e) => write!(f, "identity: {e}"),
            SocialError::BadWall => write!(f, "structurally invalid wall or linkage object"),
            SocialError::BadProfile => write!(f, "structurally invalid profile object"),
            SocialError::BadInteraction => write!(f, "structurally invalid comment or reaction"),
            SocialError::BadCommunity => write!(f, "structurally invalid community object"),
        }
    }
}
impl std::error::Error for SocialError {}
impl From<StoreError> for SocialError {
    fn from(e: StoreError) -> Self {
        SocialError::Store(e)
    }
}
impl From<mini_objects::ObjectError> for SocialError {
    fn from(e: mini_objects::ObjectError) -> Self {
        SocialError::Object(e)
    }
}
impl From<did_mini::IdentityError> for SocialError {
    fn from(e: did_mini::IdentityError) -> Self {
        SocialError::Identity(e)
    }
}

/// How a community admits members. This is a declared policy, not a hidden
/// authority grant; enforcement remains a client-side/community governance
/// concern and cannot delete the author's original objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipMode {
    /// Anyone may publish a signed join object.
    Open,
    /// Admission requires a later community decision.
    ApprovalRequired,
}

impl MembershipMode {
    fn byte(self) -> u8 {
        match self {
            Self::Open => 1,
            Self::ApprovalRequired => 2,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Open),
            2 => Some(Self::ApprovalRequired),
            _ => None,
        }
    }
}

/// A discoverable community card. It is content-addressed so communities can
/// be found through local sync or gossip without a mandatory directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Community {
    /// The community card id.
    pub id: ObjectId,
    /// The publisher of the card.
    pub owner: Did,
    /// Human-readable name.
    pub name: String,
    /// Community charter and norms.
    pub charter: String,
    /// Declared membership mode.
    pub membership: MembershipMode,
}

/// Publish a community card.
pub fn publish_community<B: Backend>(
    store: &mut Store<B>,
    owner: &Did,
    device: &Controller,
    name: &str,
    charter: &str,
    membership: MembershipMode,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if name.is_empty()
        || name.len() > MAX_COMMUNITY_NAME_BYTES
        || charter.len() > MAX_COMMUNITY_CHARTER_BYTES
    {
        return Err(SocialError::FieldTooLarge);
    }
    let mut payload = Vec::new();
    put_str(&mut payload, name);
    put_str(&mut payload, charter);
    payload.push(membership.byte());
    let community = ObjectBuilder::new(ObjectType::COMMUNITY)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(owner, device)?;
    store.insert(&community)?;
    Ok(community)
}

/// Decode and validate a community card.
pub fn resolve_community<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<Community> {
    let object = store.get(id)?;
    if object.object_type != ObjectType::COMMUNITY {
        return Err(SocialError::BadCommunity);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(SocialError::BadCommunity);
    };
    let mut pos = 0;
    let name = get_str(bytes, &mut pos).ok_or(SocialError::BadCommunity)?;
    let charter = get_str(bytes, &mut pos).ok_or(SocialError::BadCommunity)?;
    let mode = MembershipMode::from_byte(*bytes.get(pos).ok_or(SocialError::BadCommunity)?)
        .ok_or(SocialError::BadCommunity)?;
    if pos + 1 != bytes.len()
        || name.is_empty()
        || name.len() > MAX_COMMUNITY_NAME_BYTES
        || charter.len() > MAX_COMMUNITY_CHARTER_BYTES
    {
        return Err(SocialError::BadCommunity);
    }
    Ok(Community {
        id: object.id().clone(),
        owner: object.author_human,
        name,
        charter,
        membership: mode,
    })
}

/// Join or leave a community. Per member/community, the greatest
/// `(sequence, object id)` wins regardless of arrival order.
pub fn set_membership<B: Backend>(
    store: &mut Store<B>,
    member: &Did,
    device: &Controller,
    community: &ObjectId,
    joined: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let object = ObjectBuilder::new(ObjectType::Custom(MEMBERSHIP_TYPE.to_string()))
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("community", community.clone())
        .payload(Payload::Public(vec![u8::from(joined)]))
        .sign(member, device)?;
    store.insert(&object)?;
    Ok(object)
}

/// Return active members, deterministically resolved per member.
pub fn community_members<B: Backend>(store: &Store<B>, community: &ObjectId) -> Result<Vec<Did>> {
    let membership_type = ObjectType::Custom(MEMBERSHIP_TYPE.to_string());
    let mut latest: Vec<(Did, u64, String, bool)> = Vec::new();
    for id in store.linking_to(community)? {
        let object = store.get(&id)?;
        if object.object_type != membership_type
            || object.links.len() != 1
            || object.links[0].rel != "community"
            || object.links[0].target != *community
        {
            continue;
        }
        let Payload::Public(bytes) = &object.payload else {
            continue;
        };
        if bytes.len() != 1 || bytes[0] > 1 {
            continue;
        }
        let candidate = (object.sequence, object.id().as_str().to_string());
        match latest
            .iter_mut()
            .find(|(did, ..)| *did == object.author_human)
        {
            Some((_, sequence, id, state)) if candidate > (*sequence, id.clone()) => {
                *sequence = candidate.0;
                *id = candidate.1;
                *state = bytes[0] == 1;
            }
            None => latest.push((object.author_human, candidate.0, candidate.1, bytes[0] == 1)),
            _ => {}
        }
    }
    let mut members: Vec<Did> = latest
        .into_iter()
        .filter(|(_, _, _, state)| *state)
        .map(|(did, ..)| did)
        .collect();
    members.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(members)
}

/// A resolved profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The human this profile belongs to.
    pub human: Did,
    /// Display name (impersonation-proof only together with the DID — names
    /// are labels, identity is the DID).
    pub display_name: String,
    /// Short bio.
    pub bio: String,
    /// Optional avatar (a media object id).
    pub avatar: Option<ObjectId>,
}

/// Publish (or edit) a profile: writes the `PROFILE` object and moves the
/// `"profile"` head. Returns the new profile object.
#[allow(clippy::too_many_arguments)]
pub fn publish_profile<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    display_name: &str,
    bio: &str,
    avatar: Option<&ObjectId>,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if display_name.len() > MAX_NAME_BYTES || bio.len() > MAX_BIO_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let mut payload = Vec::new();
    put_str(&mut payload, display_name);
    put_str(&mut payload, bio);
    put_str(&mut payload, avatar.map(|a| a.as_str()).unwrap_or(""));

    let profile = ObjectBuilder::new(ObjectType::PROFILE)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    store.insert(&profile)?;

    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("target", profile.id().clone())
        .payload(Payload::Public(b"profile".to_vec()))
        .sign(human, device)?;
    store.apply_head(&head)?;
    Ok(profile)
}

/// Resolve the latest profile of `human`, if any.
pub fn resolve_profile<B: Backend>(store: &Store<B>, human: &Did) -> Result<Option<Profile>> {
    let target = match store.resolve_head(human, "profile")? {
        Some(t) => t,
        None => return Ok(None),
    };
    let obj = store.get(&target)?;
    if obj.object_type != ObjectType::PROFILE || obj.author_human.as_str() != human.as_str() {
        return Err(SocialError::BadProfile);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(SocialError::BadProfile),
    };
    let mut pos = 0usize;
    let display_name = get_str(bytes, &mut pos).ok_or(SocialError::BadProfile)?;
    let bio = get_str(bytes, &mut pos).ok_or(SocialError::BadProfile)?;
    let avatar_str = get_str(bytes, &mut pos).ok_or(SocialError::BadProfile)?;
    if pos != bytes.len() || display_name.len() > MAX_NAME_BYTES || bio.len() > MAX_BIO_BYTES {
        return Err(SocialError::BadProfile);
    }
    let avatar = if avatar_str.is_empty() {
        None
    } else {
        Some(ObjectId::parse(&avatar_str).map_err(|_| SocialError::BadProfile)?)
    };
    Ok(Some(Profile {
        human: obj.author_human.clone(),
        display_name,
        bio,
        avatar,
    }))
}

/// Publish a follow (or unfollow) of `target`. Per (follower, target) the
/// latest by `(sequence, id)` wins on every replica.
pub fn set_follow<B: Backend>(
    store: &mut Store<B>,
    follower: &Did,
    device: &Controller,
    target: &Did,
    follow: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let mut payload = Vec::new();
    payload.push(u8::from(follow));
    put_str(&mut payload, target.as_str());
    let obj = ObjectBuilder::new(ObjectType::FOLLOW)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(follower, device)?;
    store.insert(&obj)?;
    Ok(obj)
}

fn follow_edge(obj: &Object) -> Option<(bool, Did)> {
    if obj.object_type != ObjectType::FOLLOW {
        return None;
    }
    let bytes = match &obj.payload {
        Payload::Public(b) if !b.is_empty() => b,
        _ => return None,
    };
    let state = match bytes[0] {
        0 => false,
        1 => true,
        _ => return None,
    };
    let mut pos = 1usize;
    let target = get_str(bytes, &mut pos)?;
    if pos != bytes.len() {
        return None;
    }
    Did::parse(&target).ok().map(|d| (state, d))
}

/// The humans `who` currently follows (LWW-resolved, id-ordered).
pub fn following<B: Backend>(store: &Store<B>, who: &Did) -> Result<Vec<Did>> {
    let mut best: Vec<(Did, u64, String, bool)> = Vec::new();
    for id in store.by_author(who)? {
        let obj = store.get(&id)?;
        if let Some((state, target)) = follow_edge(&obj) {
            let cand = (obj.sequence, obj.id().as_str().to_string());
            match best
                .iter_mut()
                .find(|(t, ..)| t.as_str() == target.as_str())
            {
                Some((_, s, i, st)) => {
                    if (cand.0, cand.1.as_str()) > (*s, i.as_str()) {
                        *s = cand.0;
                        *i = cand.1;
                        *st = state;
                    }
                }
                None => best.push((target, cand.0, cand.1, state)),
            }
        }
    }
    let mut out: Vec<Did> = best
        .into_iter()
        .filter(|(_, _, _, st)| *st)
        .map(|(t, ..)| t)
        .collect();
    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(out)
}

/// The humans currently following `target` (LWW-resolved, id-ordered).
pub fn followers<B: Backend>(store: &Store<B>, target: &Did) -> Result<Vec<Did>> {
    // (follower, seq, id, state) LWW per follower.
    let mut best: Vec<(Did, u64, String, bool)> = Vec::new();
    for id in store.by_type(&ObjectType::FOLLOW)? {
        let obj = store.get(&id)?;
        if let Some((state, t)) = follow_edge(&obj) {
            if t.as_str() != target.as_str() {
                continue;
            }
            let follower = obj.author_human.clone();
            let cand = (obj.sequence, obj.id().as_str().to_string());
            match best
                .iter_mut()
                .find(|(f, ..)| f.as_str() == follower.as_str())
            {
                Some((_, s, i, st)) => {
                    if (cand.0, cand.1.as_str()) > (*s, i.as_str()) {
                        *s = cand.0;
                        *i = cand.1;
                        *st = state;
                    }
                }
                None => best.push((follower, cand.0, cand.1, state)),
            }
        }
    }
    let mut out: Vec<Did> = best
        .into_iter()
        .filter(|(_, _, _, st)| *st)
        .map(|(f, ..)| f)
        .collect();
    out.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    Ok(out)
}

/// A reply attached to a post or another comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The comment's content id.
    pub id: ObjectId,
    /// The author.
    pub author: Did,
    /// The replied-to object.
    pub parent: ObjectId,
    /// Comment text.
    pub text: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
}

/// Publish a threaded comment. The parent can be a post or another comment.
pub fn publish_comment<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    parent: &ObjectId,
    text: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if text.len() > MAX_COMMENT_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let mut payload = Vec::new();
    put_str(&mut payload, text);
    let comment = ObjectBuilder::new(ObjectType::COMMENT)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("re", parent.clone())
        .payload(Payload::Public(payload))
        .sign(human, device)?;
    store.insert(&comment)?;
    Ok(comment)
}

/// Resolve direct replies in deterministic chronological order.
pub fn comments<B: Backend>(store: &Store<B>, parent: &ObjectId) -> Result<Vec<Comment>> {
    let mut out = Vec::new();
    for id in store.linking_to(parent)? {
        let object = store.get(&id)?;
        if object.object_type != ObjectType::COMMENT {
            continue;
        }
        let Some(link) = object.links.iter().find(|link| link.rel == "re") else {
            continue;
        };
        if &link.target != parent
            || object.links.iter().filter(|link| link.rel == "re").count() != 1
        {
            continue;
        }
        let Payload::Public(bytes) = &object.payload else {
            continue;
        };
        let mut pos = 0;
        let Some(text) = get_str(bytes, &mut pos) else {
            continue;
        };
        if pos != bytes.len() || text.len() > MAX_COMMENT_BYTES {
            continue;
        }
        out.push(Comment {
            id: object.id().clone(),
            author: object.author_human,
            parent: parent.clone(),
            text,
            timestamp_ms: object.timestamp_ms,
        });
    }
    out.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    Ok(out)
}

/// Reaction types shared by social, forum, and creator surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum ReactionKind {
    /// Positive acknowledgement.
    Like,
    /// Strong positive acknowledgement.
    Love,
    /// Humour acknowledgement.
    Laugh,
    /// Forum-style positive vote.
    Upvote,
    /// Forum-style negative vote.
    Downvote,
    /// Private-to-the-user save/bookmark marker.
    Save,
}

impl ReactionKind {
    fn byte(self) -> u8 {
        match self {
            Self::Like => 1,
            Self::Love => 2,
            Self::Laugh => 3,
            Self::Upvote => 4,
            Self::Downvote => 5,
            Self::Save => 6,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Like),
            2 => Some(Self::Love),
            3 => Some(Self::Laugh),
            4 => Some(Self::Upvote),
            5 => Some(Self::Downvote),
            6 => Some(Self::Save),
            _ => None,
        }
    }
}

/// Set or clear one reaction. The latest `(sequence, object id)` wins for
/// each author/target/type, independent of arrival order.
pub fn set_reaction<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    target: &ObjectId,
    kind: ReactionKind,
    active: bool,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let reaction = ObjectBuilder::new(ObjectType::REACTION)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("target", target.clone())
        .payload(Payload::Public(vec![kind.byte(), u8::from(active)]))
        .sign(human, device)?;
    store.insert(&reaction)?;
    Ok(reaction)
}

/// Deterministic active reaction totals for any target object.
pub fn reaction_counts<B: Backend>(
    store: &Store<B>,
    target: &ObjectId,
) -> Result<Vec<(ReactionKind, usize)>> {
    let mut latest: Vec<(Did, ReactionKind, u64, String, bool)> = Vec::new();
    for id in store.linking_to(target)? {
        let object = store.get(&id)?;
        if object.object_type != ObjectType::REACTION
            || object.links.len() != 1
            || object.links[0].rel != "target"
            || object.links[0].target != *target
        {
            continue;
        }
        let Payload::Public(bytes) = &object.payload else {
            continue;
        };
        if bytes.len() != 2 {
            continue;
        }
        let Some(kind) = ReactionKind::from_byte(bytes[0]) else {
            continue;
        };
        let active = match bytes[1] {
            0 => false,
            1 => true,
            _ => continue,
        };
        let candidate = (object.sequence, object.id().as_str().to_string());
        match latest.iter_mut().find(|(author, reaction_kind, ..)| {
            author == &object.author_human && *reaction_kind == kind
        }) {
            Some((_, _, sequence, id, state)) if candidate > (*sequence, id.clone()) => {
                *sequence = candidate.0;
                *id = candidate.1;
                *state = active;
            }
            None => latest.push((object.author_human, kind, candidate.0, candidate.1, active)),
            _ => {}
        }
    }
    let kinds = [
        ReactionKind::Like,
        ReactionKind::Love,
        ReactionKind::Laugh,
        ReactionKind::Upvote,
        ReactionKind::Downvote,
        ReactionKind::Save,
    ];
    Ok(kinds
        .into_iter()
        .filter_map(|kind| {
            let count = latest
                .iter()
                .filter(|(_, k, _, _, active)| *k == kind && *active)
                .count();
            (count > 0).then_some((kind, count))
        })
        .collect())
}

/// Why an item is in the feed — always answerable (SPEC-09 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedReason {
    /// The viewer follows this author.
    Followed,
    /// The viewer authored it.
    Own,
}

/// One feed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedItem {
    /// The post's id.
    pub id: ObjectId,
    /// Its author.
    pub author: Did,
    /// Author-claimed time (display hint).
    pub timestamp_ms: u64,
    /// Why it is here.
    pub reason: FeedReason,
    /// Active reactions on this post, exposed so clients can explain support
    /// ordering without an additional hidden server query.
    pub support_count: usize,
}

/// User-chosen ranking filters. Filters are total orderings — they reorder,
/// never silently drop. New filters are Tier-O plugins; there is no hidden
/// default beyond what the user picked. [FREEZE]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeedFilter {
    /// Newest first, ties broken by id — identical on every replica.
    Chronological,
    /// Highest total active reaction count first; ties use newest then id.
    MostSupported,
}

/// Compute the viewer's feed: their own posts plus posts by humans they
/// follow, ordered by the chosen filter, truncated to `limit`. Pure over the
/// store — the same store contents yield the same feed everywhere.
pub fn feed<B: Backend>(
    store: &Store<B>,
    viewer: &Did,
    filter: FeedFilter,
    limit: usize,
) -> Result<Vec<FeedItem>> {
    let mut items: Vec<FeedItem> = Vec::new();
    let mut push_posts = |author: &Did, reason: FeedReason| -> Result<()> {
        for id in store.by_author(author)? {
            let obj = store.get(&id)?;
            if obj.object_type == ObjectType::POST {
                items.push(FeedItem {
                    id: obj.id().clone(),
                    author: obj.author_human.clone(),
                    timestamp_ms: obj.timestamp_ms,
                    reason: reason.clone(),
                    support_count: reaction_counts(store, obj.id())?
                        .into_iter()
                        .map(|(_, count)| count)
                        .sum(),
                });
            }
        }
        Ok(())
    };
    push_posts(viewer, FeedReason::Own)?;
    for followee in following(store, viewer)? {
        push_posts(&followee, FeedReason::Followed)?;
    }

    match filter {
        FeedFilter::Chronological => {
            items.sort_by(|a, b| {
                b.timestamp_ms
                    .cmp(&a.timestamp_ms)
                    .then_with(|| b.id.as_str().cmp(a.id.as_str()))
            });
        }
        FeedFilter::MostSupported => {
            items.sort_by(|a, b| {
                b.support_count
                    .cmp(&a.support_count)
                    .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
                    .then_with(|| b.id.as_str().cmp(a.id.as_str()))
            });
        }
    }
    items.truncate(limit);
    Ok(items)
}

pub(crate) fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

pub(crate) fn get_str(b: &[u8], pos: &mut usize) -> Option<String> {
    if *pos + 4 > b.len() {
        return None;
    }
    let len = u32::from_be_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]) as usize;
    *pos += 4;
    if *pos + len > b.len() || len > 4096 {
        return None;
    }
    let s = String::from_utf8(b[*pos..*pos + len].to_vec()).ok()?;
    *pos += len;
    Some(s)
}

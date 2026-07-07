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

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store, StoreError};

/// Maximum display-name bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// Maximum bio bytes.
pub const MAX_BIO_BYTES: usize = 1024;

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
}

impl core::fmt::Display for SocialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SocialError::FieldTooLarge => write!(f, "profile field too large"),
            SocialError::Store(e) => write!(f, "store: {e}"),
            SocialError::Object(e) => write!(f, "object: {e}"),
            SocialError::Identity(e) => write!(f, "identity: {e}"),
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
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Ok(None),
    };
    let mut pos = 0usize;
    let display_name = get_str(bytes, &mut pos).unwrap_or_default();
    let bio = get_str(bytes, &mut pos).unwrap_or_default();
    let avatar_str = get_str(bytes, &mut pos).unwrap_or_default();
    let avatar = if avatar_str.is_empty() {
        None
    } else {
        ObjectId::parse(&avatar_str).ok()
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
    let state = bytes[0] == 1;
    let mut pos = 1usize;
    let target = get_str(bytes, &mut pos)?;
    Did::parse(&target).ok().map(|d| (state, d))
}

/// The humans `who` currently follows (LWW-resolved, id-ordered).
pub fn following<B: Backend>(store: &Store<B>, who: &Did) -> Result<Vec<Did>> {
    let mut best: Vec<(Did, u64, String, bool)> = Vec::new();
    for id in store.by_author(who)? {
        let obj = store.get(&id)?;
        if let Some((state, target)) = follow_edge(&obj) {
            let cand = (obj.sequence, obj.id().as_str().to_string());
            match best.iter_mut().find(|(t, ..)| t.as_str() == target.as_str()) {
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
}

/// User-chosen ranking filters. Filters are total orderings — they reorder,
/// never silently drop. New filters are Tier-O plugins; there is no hidden
/// default beyond what the user picked. [FREEZE]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FeedFilter {
    /// Newest first, ties broken by id — identical on every replica.
    Chronological,
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
    }
    items.truncate(limit);
    Ok(items)
}

fn put_str(w: &mut Vec<u8>, s: &str) {
    w.extend_from_slice(&(s.len() as u32).to_be_bytes());
    w.extend_from_slice(s.as_bytes());
}

fn get_str(b: &[u8], pos: &mut usize) -> Option<String> {
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

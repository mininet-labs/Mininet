//! Ordinary posts (`POST` objects) as a first-class, bounded, canonical API.
//!
//! Every other object type this crate publishes (`PROFILE`, `WALL`,
//! `COMMENT`, `COMMUNITY`) already has its own `publish_*`/`resolve_*` pair
//! that enforces a length bound before signing. Plain posts did not: every
//! caller (in this workspace, `mini-desktop`) built a raw `ObjectBuilder`
//! directly, so nothing stopped an unbounded post from being signed and
//! inserted, and there was no shared, tested decode path a second client
//! could reuse. [`publish_post`]/[`publish_media_post`]/[`resolve_post`]/
//! [`decode_post`] close that gap without changing the wire format for the
//! shapes they cover: a plain post's payload is still exactly the post's
//! UTF-8 text bytes with zero links, and a media post's payload/link shape
//! matches `mini-desktop`'s own pre-existing hand-built one (one `"media"`
//! link, UTF-8 caption payload) — so every already-published `POST` object
//! in either shape still decodes under [`decode_post`] unchanged. What is
//! new is that *every* producer and reader of `POST` objects in this
//! workspace is required to go through this one bounded, structurally
//! validating path — a raw `ObjectBuilder`/raw payload read is no longer a
//! silent bypass. **Compatibility scope, precisely stated:** a public
//! UTF-8 plain or one-media-link post at or below [`MAX_POST_BYTES`]
//! decodes unchanged; a previously storable encrypted, non-UTF-8,
//! oversized, or multi-/unknown-link `POST` object is intentionally
//! rejected by [`decode_post`], not silently accepted — this crate never
//! claimed those decoded "unchanged" and does not start now.

use did_mini::{Controller, Did};
use mini_objects::{Link, Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::{Result, SocialError};

/// Maximum UTF-8 bytes in one post's text (plain post text, or a media
/// post's caption).
pub const MAX_POST_BYTES: usize = 16 * 1024;

/// The `Link::rel` a media post's single structural link must carry —
/// `mini-desktop`'s pre-existing hand-built media posts already used this
/// exact relation name.
const MEDIA_LINK_REL: &str = "media";

/// What structural shape a decoded post has. New variants are a breaking
/// addition on purpose (`#[non_exhaustive]`): a future post shape needs an
/// explicit decision here, not an unvalidated raw path that happens to
/// slip past whatever `decode_post` currently checks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostKind {
    /// No structural links: the payload is the entire post.
    Plain,
    /// Exactly one `"media"` link; the payload is this post's caption.
    Media {
        /// The linked media manifest's object id.
        media: ObjectId,
    },
}

/// A resolved, structurally validated post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    /// The post's content id.
    pub id: ObjectId,
    /// The author.
    pub author: Did,
    /// Post text (caption, for a media post).
    pub text: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
    /// Which structural shape this post has.
    pub kind: PostKind,
}

/// Build (sign) a plain text post without inserting it anywhere — the
/// signing half of [`publish_post`], split out so a caller that needs to
/// durably persist the exact signed bytes *before* committing them to a
/// store (e.g. a crash-recoverable publish journal, so a retry after a
/// crash reuses the same signature instead of allocating a new
/// sequence/timestamp and signing a second, distinct post) has one real
/// path to do that rather than hand-rolling its own `ObjectBuilder` call.
pub fn build_post(
    human: &Did,
    device: &Controller,
    text: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if text.len() > MAX_POST_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(text.as_bytes().to_vec()))
        .sign(human, device)?;
    Ok(post)
}

/// Publish a plain text post: zero structural links, payload is the post's
/// UTF-8 bytes unmodified, bounded to [`MAX_POST_BYTES`] before signing.
pub fn publish_post<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    text: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let post = build_post(human, device, text, timestamp_ms, sequence)?;
    store.insert(&post)?;
    Ok(post)
}

/// Publish a media post: exactly one `"media"` link to an already-published
/// media manifest object, plus a caption bounded to [`MAX_POST_BYTES`]
/// before signing — the canonical counterpart to what `mini-desktop`
/// previously hand-built directly with no bound at all.
pub fn publish_media_post<B: Backend>(
    store: &mut Store<B>,
    human: &Did,
    device: &Controller,
    media: ObjectId,
    caption: &str,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if caption.len() > MAX_POST_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    let post = ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link(MEDIA_LINK_REL, media)
        .payload(Payload::Public(caption.as_bytes().to_vec()))
        .sign(human, device)?;
    store.insert(&post)?;
    Ok(post)
}

/// Decode and structurally validate an already-fetched `POST` object —
/// pure, no store access, so callers scanning many objects (e.g.
/// [`crate::feed`]) can validate without a second fetch. Rejects: wrong
/// object type, encrypted payload, oversized payload, non-UTF-8 payload,
/// and any link shape other than zero links (`Plain`) or exactly one
/// `"media"` link (`Media`) — an unrecognized/duplicate/extra link is a
/// structurally invalid post, not silently ignored.
pub fn decode_post(object: &Object) -> Result<Post> {
    if object.object_type != ObjectType::POST {
        return Err(SocialError::BadPost);
    }
    let Payload::Public(bytes) = &object.payload else {
        return Err(SocialError::BadPost);
    };
    if bytes.len() > MAX_POST_BYTES {
        return Err(SocialError::BadPost);
    }
    let text = String::from_utf8(bytes.clone()).map_err(|_| SocialError::BadPost)?;

    let kind = match object.links.as_slice() {
        [] => PostKind::Plain,
        [Link { rel, target }] if rel == MEDIA_LINK_REL => PostKind::Media {
            media: target.clone(),
        },
        _ => return Err(SocialError::BadPost),
    };

    Ok(Post {
        id: object.id().clone(),
        author: object.author_human.clone(),
        text,
        timestamp_ms: object.timestamp_ms,
        kind,
    })
}

/// Fetch and [`decode_post`] a stored `POST` object.
pub fn resolve_post<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<Post> {
    let object = store.get(id)?;
    decode_post(&object)
}

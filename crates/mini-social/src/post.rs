//! Ordinary posts (`POST` objects) as a first-class, bounded API.
//!
//! Every other object type this crate publishes (`PROFILE`, `WALL`,
//! `COMMENT`, `COMMUNITY`) already has its own `publish_*`/`resolve_*` pair
//! that enforces a length bound before signing. Plain posts did not: every
//! caller (in this workspace, `mini-desktop`) built a raw `ObjectBuilder`
//! directly, so nothing stopped an unbounded post from being signed and
//! inserted, and there was no shared, tested decode path a second client
//! could reuse. [`publish_post`]/[`resolve_post`] close that gap without
//! changing the wire format: the payload is still exactly the post's UTF-8
//! text bytes (so every already-published `POST` object — including
//! `mini-desktop`'s own hand-built ones — decodes under [`resolve_post`]
//! unchanged), just with the same bound-before-sign / bound-on-decode
//! discipline every other object type in this crate already has.

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::{Result, SocialError};

/// Maximum UTF-8 bytes in one post's text.
pub const MAX_POST_BYTES: usize = 16 * 1024;

/// A resolved post.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    /// The post's content id.
    pub id: ObjectId,
    /// The author.
    pub author: Did,
    /// Post text.
    pub text: String,
    /// Author-claimed creation time.
    pub timestamp_ms: u64,
}

/// Publish a plain text post. The payload is the post's UTF-8 bytes
/// unmodified — the same wire shape every hand-rolled `POST` object in this
/// workspace already used — bounded to [`MAX_POST_BYTES`] before signing.
pub fn publish_post<B: Backend>(
    store: &mut Store<B>,
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
    store.insert(&post)?;
    Ok(post)
}

/// Decode and bounds-check a stored `POST` object.
pub fn resolve_post<B: Backend>(store: &Store<B>, id: &ObjectId) -> Result<Post> {
    let object = store.get(id)?;
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
    Ok(Post {
        id: object.id().clone(),
        author: object.author_human.clone(),
        text,
        timestamp_ms: object.timestamp_ms,
    })
}

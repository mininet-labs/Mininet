//! Public walls (founder decision, 2026-07-07): first-class, voluntary public
//! identity surfaces.
//!
//! ## A wall is a disclosure surface, not the identity root [FREEZE]
//!
//! A [`PublicWall`] is owned by whatever `did:mini` a person chooses to
//! publish it under: their primary root, or an entirely independent root they
//! incepted purely as a pseudonym (`did-mini` makes no claim about how many
//! roots one human runs — SPEC-01 §0). Publishing a wall:
//!
//! - **Never reveals the human-root.** [`PublicWall`] carries only the DID it
//!   was published under — nothing else. A separate, explicit, opt-in
//!   [`publish_wall_linkage`] is the *only* way to bind a wall to another DID
//!   (e.g. a human-root), and its absence is the default.
//! - **Never grants a vote or extra standing.** Publishing a `WALL` object
//!   requires only [`did_mini::Capabilities::POST`] (see
//!   `mini-objects::object::required_capability`) — the same capability as an
//!   ordinary post. It is structurally impossible for a wall to require or
//!   imply [`did_mini::Capabilities::VOTE`], and `mini-social` never links
//!   against `mini-forge` or `mini-reward`, so publishing a wall cannot touch
//!   governance or reward accounting even indirectly.
//! - **Never registers a new "human".** There is no wall registry; an unknown
//!   wall simply resolves to `None`, exactly like an unknown profile.
//!
//! Many walls can belong to one human privately (they just incept many
//! independent roots and publish a wall under each); publicly they are
//! unlinkable by default because nothing in a `WALL` object's bytes, id, or
//! KEL points at any other identity.

use did_mini::{Controller, Did};
use mini_objects::{Object, ObjectBuilder, ObjectId, ObjectType, Payload};
use mini_store::{Backend, Store};

use crate::{get_str, put_str, Result, SocialError};

/// Maximum wall display-name bytes.
pub const MAX_WALL_NAME_BYTES: usize = 64;
/// Maximum wall bio bytes.
pub const MAX_WALL_BIO_BYTES: usize = 1024;
/// Maximum number of public links a wall may list.
pub const MAX_WALL_LINKS: usize = 16;
/// Maximum bytes per public link.
pub const MAX_WALL_LINK_BYTES: usize = 256;
/// Maximum number of pinned objects a wall may declare.
pub const MAX_WALL_PINNED: usize = 32;

/// Who can discover a wall. This governs *indexing/advertisement*, never
/// protocol privilege — a wall's visibility never changes what it may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VisibilityPolicy {
    /// Freely discoverable/indexable.
    Public,
    /// Resolvable by direct id/DID only; not advertised for discovery.
    Unlisted,
}

impl VisibilityPolicy {
    fn to_byte(self) -> u8 {
        match self {
            VisibilityPolicy::Public => 0,
            VisibilityPolicy::Unlisted => 1,
        }
    }
    fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(VisibilityPolicy::Public),
            1 => Some(VisibilityPolicy::Unlisted),
            _ => None,
        }
    }
}

/// A resolved public wall. Deliberately carries **no** field naming a
/// human-root, a vote, a score, or a rank — see the module docs [FREEZE].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicWall {
    /// The wall object's own content id.
    pub wall_id: ObjectId,
    /// The DID this wall was published under (may or may not be a human's
    /// primary root — the protocol cannot tell, by design).
    pub owner: Did,
    /// Display name.
    pub display_name: String,
    /// Bio / description.
    pub bio: String,
    /// Optional avatar (a media object id).
    pub avatar: Option<ObjectId>,
    /// Public links (URLs or other wall ids, as opaque strings).
    pub public_links: Vec<String>,
    /// Objects the owner chose to pin.
    pub pinned: Vec<ObjectId>,
    /// Discovery policy.
    pub visibility: VisibilityPolicy,
}

/// Publish (or edit) a public wall under `owner`. Like [`crate::publish_profile`],
/// this writes a `WALL` object and moves the `"wall"` head so edits converge
/// deterministically (LWW by `(sequence, id)`).
#[allow(clippy::too_many_arguments)]
pub fn publish_wall<B: Backend>(
    store: &mut Store<B>,
    owner: &Did,
    device: &Controller,
    display_name: &str,
    bio: &str,
    avatar: Option<&ObjectId>,
    public_links: &[&str],
    pinned: &[ObjectId],
    visibility: VisibilityPolicy,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let (wall, head) = build_wall(
        owner,
        device,
        display_name,
        bio,
        avatar,
        public_links,
        pinned,
        visibility,
        timestamp_ms,
        sequence,
    )?;
    store.insert(&wall)?;
    store.apply_head(&head)?;
    Ok(wall)
}

/// Build the exact public wall and its separate wall head without store writes.
#[allow(clippy::too_many_arguments)]
pub fn build_wall(
    owner: &Did,
    device: &Controller,
    display_name: &str,
    bio: &str,
    avatar: Option<&ObjectId>,
    public_links: &[&str],
    pinned: &[ObjectId],
    visibility: VisibilityPolicy,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<(Object, Object)> {
    if display_name.len() > MAX_WALL_NAME_BYTES || bio.len() > MAX_WALL_BIO_BYTES {
        return Err(SocialError::FieldTooLarge);
    }
    if public_links.len() > MAX_WALL_LINKS || pinned.len() > MAX_WALL_PINNED {
        return Err(SocialError::FieldTooLarge);
    }
    if public_links.iter().any(|l| l.len() > MAX_WALL_LINK_BYTES) {
        return Err(SocialError::FieldTooLarge);
    }

    let mut payload = Vec::new();
    put_str(&mut payload, display_name);
    put_str(&mut payload, bio);
    put_str(&mut payload, avatar.map(|a| a.as_str()).unwrap_or(""));
    payload.push(visibility.to_byte());
    payload.extend_from_slice(&(public_links.len() as u32).to_be_bytes());
    for link in public_links {
        put_str(&mut payload, link);
    }
    payload.extend_from_slice(&(pinned.len() as u32).to_be_bytes());
    for id in pinned {
        put_str(&mut payload, id.as_str());
    }

    let wall = ObjectBuilder::new(ObjectType::WALL)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(payload))
        .sign(owner, device)?;
    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("target", wall.id().clone())
        .payload(Payload::Public(b"wall".to_vec()))
        .sign(owner, device)?;
    Ok((wall, head))
}

/// Resolve the latest wall published under `owner`, if any. Never resolves,
/// derives, or exposes a human-root: the returned [`PublicWall`] has no such
/// field, by construction.
pub fn resolve_wall<B: Backend>(store: &Store<B>, owner: &Did) -> Result<Option<PublicWall>> {
    let target = match store.resolve_head(owner, "wall")? {
        Some(t) => t,
        None => return Ok(None),
    };
    let obj = store.get(&target)?;
    if obj.object_type != ObjectType::WALL || obj.author_human.as_str() != owner.as_str() {
        return Err(SocialError::BadWall);
    }
    if matches!(&obj.payload, Payload::Encrypted(_)) {
        return Ok(None);
    }
    decode_wall(&obj).map(Some)
}

/// Decode one canonical public wall, without resolving identity or head state.
pub fn decode_wall(obj: &Object) -> Result<PublicWall> {
    if obj.object_type != ObjectType::WALL || !obj.links.is_empty() {
        return Err(SocialError::BadWall);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Err(SocialError::BadWall),
    };
    if bytes.len() > 256 * 1024 {
        return Err(SocialError::BadWall);
    }
    let mut pos = 0usize;
    let display_name = get_str(bytes, &mut pos).ok_or(SocialError::BadWall)?;
    let bio = get_str(bytes, &mut pos).ok_or(SocialError::BadWall)?;
    let avatar_str = get_str(bytes, &mut pos).ok_or(SocialError::BadWall)?;
    let avatar = if avatar_str.is_empty() {
        None
    } else {
        Some(ObjectId::parse(&avatar_str).map_err(|_| SocialError::BadWall)?)
    };
    let visibility_byte = *bytes.get(pos).ok_or(SocialError::BadWall)?;
    pos += 1;
    let visibility = VisibilityPolicy::from_byte(visibility_byte).ok_or(SocialError::BadWall)?;

    let nlinks = read_u32(bytes, &mut pos)? as usize;
    if nlinks > MAX_WALL_LINKS {
        return Err(SocialError::BadWall);
    }
    let mut public_links = Vec::with_capacity(nlinks);
    for _ in 0..nlinks {
        let link = get_str(bytes, &mut pos).ok_or(SocialError::BadWall)?;
        if link.len() > MAX_WALL_LINK_BYTES {
            return Err(SocialError::BadWall);
        }
        public_links.push(link);
    }

    let npinned = read_u32(bytes, &mut pos)? as usize;
    if npinned > MAX_WALL_PINNED {
        return Err(SocialError::BadWall);
    }
    let mut pinned = Vec::with_capacity(npinned);
    for _ in 0..npinned {
        let s = get_str(bytes, &mut pos).ok_or(SocialError::BadWall)?;
        pinned.push(ObjectId::parse(&s).map_err(SocialError::Object)?);
    }
    if pos != bytes.len()
        || display_name.len() > MAX_WALL_NAME_BYTES
        || bio.len() > MAX_WALL_BIO_BYTES
    {
        return Err(SocialError::BadWall);
    }

    Ok(PublicWall {
        wall_id: obj.id().clone(),
        owner: obj.author_human.clone(),
        display_name,
        bio,
        avatar,
        public_links,
        pinned,
        visibility,
    })
}

/// Voluntarily, explicitly publish a signed linkage from `wall_owner` to
/// `linked_did` (e.g. a human-root, or another wall the owner wants to
/// publicly connect). This is the **only** protocol path that can bind a
/// wall to another identity — its absence is the default, and only the
/// wall's own device can create it (it is self-asserted disclosure, not
/// something any third party can impose).
pub fn publish_wall_linkage<B: Backend>(
    store: &mut Store<B>,
    wall_owner: &Did,
    device: &Controller,
    linked_did: &Did,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    let obj = ObjectBuilder::new(ObjectType::WALL_LINKAGE)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(linked_did.as_str().as_bytes().to_vec()))
        .sign(wall_owner, device)?;
    store.insert(&obj)?;

    let head = ObjectBuilder::new(ObjectType::HEAD)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .link("target", obj.id().clone())
        .payload(Payload::Public(b"wall-linkage".to_vec()))
        .sign(wall_owner, device)?;
    store.apply_head(&head)?;
    Ok(obj)
}

/// Resolve the DID `wall_owner` has voluntarily linked their wall to, if any.
/// `None` (the default) means no linkage has ever been published.
pub fn resolve_wall_linkage<B: Backend>(store: &Store<B>, wall_owner: &Did) -> Result<Option<Did>> {
    let target = match store.resolve_head(wall_owner, "wall-linkage")? {
        Some(t) => t,
        None => return Ok(None),
    };
    let obj = store.get(&target)?;
    if obj.object_type != ObjectType::WALL_LINKAGE
        || obj.author_human.as_str() != wall_owner.as_str()
    {
        return Err(SocialError::BadWall);
    }
    let bytes = match &obj.payload {
        Payload::Public(b) => b,
        Payload::Encrypted(_) => return Ok(None),
    };
    let s = String::from_utf8(bytes.clone()).map_err(|_| SocialError::BadWall)?;
    Did::parse(&s).map(Some).map_err(SocialError::Identity)
}

fn read_u32(b: &[u8], pos: &mut usize) -> Result<u32> {
    if *pos + 4 > b.len() {
        return Err(SocialError::BadWall);
    }
    let v = u32::from_be_bytes([b[*pos], b[*pos + 1], b[*pos + 2], b[*pos + 3]]);
    *pos += 4;
    Ok(v)
}

#[cfg(test)]
mod builder_tests {
    use super::*;
    #[test]
    fn store_free_wall_keeps_legacy_modes_and_strict_decoder_shape() {
        let owner = Controller::incept_single_from_seeds(&[91; 32], &[92; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&owner.did(), &[93; 32], &[94; 32])
                .unwrap();
        for visibility in [VisibilityPolicy::Public, VisibilityPolicy::Unlisted] {
            let (wall, head) = build_wall(
                &owner.did(),
                &device,
                "Wall",
                "Bio",
                None,
                &["https://example.test"],
                &[],
                visibility,
                7,
                8,
            )
            .unwrap();
            let mut payload = Vec::new();
            put_str(&mut payload, "Wall");
            put_str(&mut payload, "Bio");
            put_str(&mut payload, "");
            payload.push(visibility.to_byte());
            payload.extend_from_slice(&1u32.to_be_bytes());
            put_str(&mut payload, "https://example.test");
            payload.extend_from_slice(&0u32.to_be_bytes());
            let legacy = ObjectBuilder::new(ObjectType::WALL)
                .timestamp_ms(7)
                .sequence(8)
                .payload(Payload::Public(payload.clone()))
                .sign(&owner.did(), &device)
                .unwrap();
            let legacy_head = ObjectBuilder::new(ObjectType::HEAD)
                .timestamp_ms(7)
                .sequence(8)
                .link("target", legacy.id().clone())
                .payload(Payload::Public(b"wall".to_vec()))
                .sign(&owner.did(), &device)
                .unwrap();
            assert_eq!(wall.to_bytes(), legacy.to_bytes());
            assert_eq!(head.to_bytes(), legacy_head.to_bytes());
            assert_eq!(decode_wall(&wall).unwrap().visibility, visibility);
            let mut store = Store::new(mini_store::MemoryBackend::new());
            assert_eq!(
                publish_wall(
                    &mut store,
                    &owner.did(),
                    &device,
                    "Wall",
                    "Bio",
                    None,
                    &["https://example.test"],
                    &[],
                    visibility,
                    7,
                    8
                )
                .unwrap()
                .to_bytes(),
                wall.to_bytes()
            );
            assert_eq!(
                resolve_wall(&store, &owner.did()).unwrap().unwrap(),
                decode_wall(&wall).unwrap()
            );
            payload.push(0);
            let trailing = ObjectBuilder::new(ObjectType::WALL)
                .payload(Payload::Public(payload))
                .sign(&owner.did(), &device)
                .unwrap();
            assert!(decode_wall(&trailing).is_err());
            let linked = ObjectBuilder::new(ObjectType::WALL)
                .link("extra", wall.id().clone())
                .payload(wall.payload.clone())
                .sign(&owner.did(), &device)
                .unwrap();
            assert!(decode_wall(&linked).is_err());
            let (full, _) = build_wall(
                &owner.did(),
                &device,
                "Full",
                "",
                Some(wall.id()),
                &[],
                &[wall.id().clone()],
                visibility,
                7,
                9,
            )
            .unwrap();
            let decoded = decode_wall(&full).unwrap();
            assert_eq!(decoded.avatar, Some(wall.id().clone()));
            assert_eq!(decoded.pinned, vec![wall.id().clone()]);
        }
    }
}

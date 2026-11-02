//! Automatic eviction of other people's re-fetchable cached content, with a
//! retention the owner controls (short, standard, long or never).
//!
//! What may ever be evicted (all of it must hold):
//! - authored by someone else: the owner's own content is never touched;
//! - one of a short allow-list of social content types (posts, comments,
//!   reactions, media manifests and media chunks) — never keys, vouches,
//!   attestations, releases, heads, profiles, communities or anything else
//!   verification or governance may need;
//! - public (never encrypted/private) and in the default ephemeral cache tier
//!   (pinned, seeded and committed content is kept) — enforced by the store;
//! - older than the retention, measured from when *this device* first saw it,
//!   not from an author-claimed timestamp;
//! - not linked to by any object that stays (a retained reply keeps its
//!   parent) — enforced by the store.
//!
//! Space pressure shortens the retention (Low: at most seven days; Critical:
//! at most one day) but "never" is always honoured: the owner is warned
//! instead. Evicted content can be fetched again from peers.

use crate::ServiceError;
use did_mini::Did;
use mini_durable::space::SpaceLevel;
use mini_objects::{ObjectId, ObjectType, Payload};
use mini_store::{CacheTier, FsBackend, Store};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const DAY_MS: u64 = 86_400_000;
/// Objects examined per pass; the rest are reached on later passes.
const MAX_EXAMINED: usize = 20_000;
/// Objects removed per pass.
const MAX_EVICTED: usize = 2_000;

/// How long other people's cached content is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// 7 days.
    Short,
    /// 30 days (default).
    Standard,
    /// 180 days.
    Long,
    /// Never delete automatically.
    Never,
}

impl Retention {
    pub(crate) fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Retention::Short),
            1 => Some(Retention::Standard),
            2 => Some(Retention::Long),
            3 => Some(Retention::Never),
            _ => None,
        }
    }

    pub(crate) fn to_u8(self) -> u8 {
        match self {
            Retention::Short => 0,
            Retention::Standard => 1,
            Retention::Long => 2,
            Retention::Never => 3,
        }
    }

    /// Effective retention (ms) given device storage pressure; `None` = never.
    pub(crate) fn effective_ms(self, level: SpaceLevel) -> Option<u64> {
        let base = match self {
            Retention::Short => 7 * DAY_MS,
            Retention::Standard => 30 * DAY_MS,
            Retention::Long => 180 * DAY_MS,
            Retention::Never => return None,
        };
        Some(match level {
            SpaceLevel::Ok => base,
            SpaceLevel::Low => base.min(7 * DAY_MS),
            SpaceLevel::Critical => base.min(DAY_MS),
        })
    }
}

fn path(root: &Path) -> PathBuf {
    root.join("app-service").join("cache-retention")
}

/// Load the saved retention; a missing or unreadable file means the default.
pub(crate) fn load(root: &Path) -> Retention {
    std::fs::read(path(root))
        .ok()
        .and_then(|bytes| match bytes.as_slice() {
            [b] => Retention::from_u8(*b),
            _ => None,
        })
        .unwrap_or(Retention::Standard)
}

pub(crate) fn save(root: &Path, retention: Retention) -> Result<(), ServiceError> {
    let file = path(root);
    if let Some(parent) = file.parent() {
        mini_durable::create_dir_all(parent).map_err(io)?;
    }
    mini_durable::atomic_replace(&file, &[retention.to_u8()]).map_err(io)
}

fn io(error: impl std::fmt::Display) -> ServiceError {
    ServiceError::new(mini_app_protocol::ErrorCode::Io, error.to_string())
}

fn evictable_type(kind: &ObjectType) -> bool {
    *kind == ObjectType::POST
        || *kind == ObjectType::COMMENT
        || *kind == ObjectType::REACTION
        || *kind == ObjectType::MEDIA_MANIFEST
        || *kind == ObjectType::Custom(mini_media::CHUNK_TYPE.to_string())
}

/// One pass: remove expired cache under the effective retention. Returns the
/// bytes freed. Errors from individual objects are skipped, never fatal.
pub(crate) fn evict_expired(
    store: &mut Store<FsBackend>,
    owner: &Did,
    retention: Retention,
    level: SpaceLevel,
    now_ms: u64,
) -> u64 {
    let Some(max_age) = retention.effective_ms(level) else {
        return 0;
    };
    let Ok(ids) = store.all_ids() else {
        return 0;
    };
    let mut set: BTreeSet<String> = BTreeSet::new();
    for id in ids.iter().take(MAX_EXAMINED) {
        if set.len() >= MAX_EVICTED {
            break;
        }
        let Ok(object) = store.get(id) else { continue };
        if &object.author_human == owner
            || !evictable_type(&object.object_type)
            || matches!(object.payload, Payload::Encrypted(_))
        {
            continue;
        }
        if store.cache_tier(id).ok() != Some(CacheTier::EphemeralCache) {
            continue;
        }
        let Ok(seen) = store.note_first_seen(id, now_ms) else {
            continue;
        };
        if now_ms.saturating_sub(seen) >= max_age {
            set.insert(id.as_str().to_string());
        }
    }
    // Pull in the media chunks of any evicted manifest (when they are also
    // someone else's cache); then drop every member something outside the set
    // still links to, until stable. The store re-checks all of this.
    let manifests: Vec<ObjectId> = set.iter().filter_map(|s| ObjectId::parse(s).ok()).collect();
    for id in manifests {
        let Ok(object) = store.get(&id) else { continue };
        if object.object_type != ObjectType::MEDIA_MANIFEST {
            continue;
        }
        for link in object.links.iter().filter(|l| l.rel == "chunk") {
            if let Ok(chunk) = store.get(&link.target) {
                if &chunk.author_human != owner
                    && evictable_type(&chunk.object_type)
                    && store.cache_tier(&link.target).ok() == Some(CacheTier::EphemeralCache)
                {
                    set.insert(link.target.as_str().to_string());
                }
            }
        }
    }
    loop {
        let mut removed = false;
        let members: Vec<String> = set.iter().cloned().collect();
        for member in members {
            let Ok(id) = ObjectId::parse(&member) else {
                set.remove(&member);
                continue;
            };
            let blocked = store
                .linking_to(&id)
                .map(|linkers| linkers.iter().any(|l| !set.contains(l.as_str())))
                .unwrap_or(true);
            if blocked {
                set.remove(&member);
                removed = true;
            }
        }
        if !removed {
            break;
        }
    }
    let ids: Vec<ObjectId> = set.iter().filter_map(|s| ObjectId::parse(s).ok()).collect();
    if ids.is_empty() {
        return 0;
    }
    store.evict_cached(&ids).map(|r| r.bytes).unwrap_or(0)
}

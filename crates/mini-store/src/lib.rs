//! Content-addressed local storage for Mininet objects (UI plan E2.S2).
//!
//! The store is **persistence + deterministic indexes**, not a trust boundary:
//!
//! - [`Store::insert`] verifies **integrity** (bytes self-certify their id) and
//!   persists the object plus author/type/link indexes.
//! - Signature and provenance verification (`mini-objects` layers 2–3) belong to
//!   the **ingest pipeline** — the sync layer verifies before insertion. The
//!   store never weakens that; it just doesn't duplicate it per read.
//! - [`Store::apply_head`] implements SPEC-09 §3 **signed head pointers**
//!   (single-author mutable state — profiles, post edits): a head is a normal
//!   signed object of type [`ObjectType::HEAD`] whose payload names the subject
//!   and whose single `"target"` link points at the latest version. Replicas
//!   converge deterministically: highest sequence wins; ties break on the
//!   lexicographically greatest object id — so any two stores that saw the same
//!   heads resolve the same state, in any arrival order.
//! - [`Store::missing_links`] / [`Store::want_list`] are the seed of sync
//!   (E3): what a peer should fetch next.
//!
//! Backends: [`MemoryBackend`] for tests, [`FsBackend`] (atomic tmp+rename
//! writes, fanout directories) for devices. A SQLite backend slots in behind
//! the same [`Backend`] trait at integration (D-0020 stack), changing nothing
//! above it.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod backend;
mod cache;
mod store;

pub use backend::{Backend, FsBackend, MemoryBackend};
pub use cache::{CacheTier, ViewConditions};
pub use store::{HeadState, Store};

use did_mini::IdentityError;
use mini_objects::ObjectError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, StoreError>;

/// Why a store operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StoreError {
    /// Underlying I/O failure (filesystem backend).
    Io(String),
    /// The object failed integrity/decoding.
    Object(ObjectError),
    /// An identity failure.
    Identity(IdentityError),
    /// A head object was structurally invalid (wrong type, links, or subject).
    BadHead,
    /// The requested object is not in the store.
    NotFound,
    /// The backend returned bytes that do not derive the requested id — a
    /// corrupted or malicious backend (content-addressing violated).
    Corrupt,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::Io(e) => write!(f, "store i/o: {e}"),
            StoreError::Object(e) => write!(f, "object: {e}"),
            StoreError::Identity(e) => write!(f, "identity: {e}"),
            StoreError::BadHead => write!(f, "structurally invalid head object"),
            StoreError::NotFound => write!(f, "object not found"),
            StoreError::Corrupt => write!(f, "backend bytes do not match requested id"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<ObjectError> for StoreError {
    fn from(e: ObjectError) -> Self {
        StoreError::Object(e)
    }
}
impl From<IdentityError> for StoreError {
    fn from(e: IdentityError) -> Self {
        StoreError::Identity(e)
    }
}
impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e.to_string())
    }
}

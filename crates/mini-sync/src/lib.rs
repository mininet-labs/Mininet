//! Store-and-forward replication (UI plan E3): reconcile two [`mini_store`]
//! stores over any [`mini_bearer::Bearer`], inside the encrypted channel.
//!
//! ## Protocol (MINI/SYNC1) — pull-based, strictly alternating
//!
//! One *pull* moves objects from server to client:
//!
//! ```text
//! client -> server : RootDigest(my set)
//! server -> client : BucketDigests           (or Done if roots match)
//! client -> server : NeedBuckets(differing)
//! server -> client : Ids(per needed bucket)
//! client -> server : Want(ids I lack)
//! server -> client : Objects(batches) ... Done
//! ```
//!
//! A full sync is two pulls (each side pulls once). Every step is
//! send-then-receive, so the protocol never deadlocks on a half-duplex bearer.
//!
//! **Reconciliation** is bucketed: ids are grouped by a character of their
//! content id, and only differing buckets exchange id lists — cheap when two
//! stores mostly overlap (the common mesh case).
//!
//! **Resume = idempotence.** Objects are content-addressed and insertion is
//! idempotent, so an interrupted sync loses nothing: the next encounter
//! reconciles what remains. No transfer-session state to corrupt (A3
//! store-and-forward model). Large media rides as many small chunk objects
//! (`mini-media`), so per-object transfer stays frame-sized.
//!
//! ## The trust boundary lives here
//!
//! `mini-store` persists; **sync verifies**. Every received object passes the
//! [`Ingest`] pipeline before insertion:
//!
//! 1. **Integrity** — bytes self-certify their id (bounded decode).
//! 2. **KEL carriers first** — identities travel as ordinary objects
//!    ([`KEL_CARRIER`]: payload = a KEL's bytes, self-certifying). Received
//!    batches are ingested carriers-first, so a stranger's identity arrives
//!    with (or before) their content.
//! 3. **Signature + provenance** — with the author's root and device KELs in
//!    the [`KelCache`], each object must pass `verify_provenance` (delegated,
//!    unrevoked, capability-scoped). Objects whose authors are unknown are
//!    **rejected, not quarantined** — the peer that wants you to hold content
//!    must give you the identity that signed it. Strict by default; a relaxed
//!    policy is a caller choice, never a silent one.
//!
//! P5 note: sync runs inside the anonymous encrypted channel; the *transport*
//! learns nothing. What you replicate reveals your interests to the peer you
//! chose to sync with — that is inherent to replication and stated, not hidden.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod ingest;
mod message;
mod protocol;

pub use ingest::{kel_carrier, Ingest, IngestOutcome, KelCache, KEL_CARRIER};
pub use protocol::{serve_pull, sync_bidirectional, IngestReport, SyncRole};

use mini_bearer::BearerError;
use mini_objects::ObjectError;
use mini_store::StoreError;

/// Result alias for this crate.
pub type Result<T> = core::result::Result<T, SyncError>;

/// Why a sync failed (transport/protocol level — bad *objects* are per-object
/// ingest rejections, not sync failures).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyncError {
    /// Transport or channel failure.
    Bearer(BearerError),
    /// Store failure.
    Store(StoreError),
    /// Object decoding failure at the protocol layer.
    Object(ObjectError),
    /// A peer sent a malformed or out-of-order protocol message.
    Protocol,
    /// A peer exceeded a protocol limit (message counts/sizes).
    LimitExceeded,
}

impl core::fmt::Display for SyncError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SyncError::Bearer(e) => write!(f, "bearer: {e}"),
            SyncError::Store(e) => write!(f, "store: {e}"),
            SyncError::Object(e) => write!(f, "object: {e}"),
            SyncError::Protocol => write!(f, "malformed or out-of-order sync message"),
            SyncError::LimitExceeded => write!(f, "sync protocol limit exceeded"),
        }
    }
}
impl std::error::Error for SyncError {}
impl From<BearerError> for SyncError {
    fn from(e: BearerError) -> Self {
        SyncError::Bearer(e)
    }
}
impl From<StoreError> for SyncError {
    fn from(e: StoreError) -> Self {
        SyncError::Store(e)
    }
}
impl From<ObjectError> for SyncError {
    fn from(e: ObjectError) -> Self {
        SyncError::Object(e)
    }
}

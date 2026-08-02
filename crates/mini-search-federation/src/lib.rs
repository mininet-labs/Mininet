//! MiniSearch Track F1/F2/F3/F4/F7 (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29): signed crawl observations (F1), immutable index segments (F2),
//! deterministic federated merging (F3), caller-local re-ranking (F4), and a
//! bounded local history over authenticated observations (F7).
//!
//! ## What's implemented here
//!
//! [`publish_crawl_observation`]/[`read_crawl_observation`] and
//! [`publish_index_segment`]/[`read_index_segment`] wrap already-produced
//! MiniSearch records in signed, content-addressed [`mini_objects::Object`]s.
//! Decode and authenticity remain separate checks: callers verify the wrapping
//! object against the publisher's KEL before treating the decoded payload as
//! authentic. Publishing now applies the same field bounds as reading, so the
//! crate cannot create an observation object its own reader rejects solely for
//! oversized typed fields.
//!
//! [`federate_query`] runs the unmodified `mini_query::search` once per
//! [`FederationSource`] and mechanically merges its already-provenanced output.
//! [`local_rerank`] recombines the existing per-signal explanation under a
//! caller-chosen ranking profile without a new query.
//!
//! [`SnapshotIndex`] preserves the full decoded F1 observation and object id,
//! derives indexing fields from that typed record, applies explicit count and
//! canonical-wire-byte budgets, and keeps uncertainty visible: crawler
//! timestamps are not canonical time, missing digests are not changes, and
//! equally timestamped providers may disagree. See [`VersionRelation`].
//!
//! ## What's deliberately NOT here
//!
//! No network transport, peer discovery, request scheduling, provider payment
//! implementation (F5), or private query transport (F6). F7 is a rebuildable
//! local view, not a signed shared history or a truth oracle. Default local
//! budgets are finite but unbenchmarked; production weakest-device limits still
//! require measurement.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod federate;
mod history;
mod observation;
mod rerank;
mod segment;

pub use error::{FederationError, Result};
pub use federate::{federate_query, FederatedResult, FederationSource};
pub use history::{
    Snapshot, SnapshotIndex, SnapshotInsert, SnapshotLimits, VersionRelation,
    DEFAULT_MAX_SNAPSHOTS_PER_URL, DEFAULT_MAX_SNAPSHOT_URLS,
    DEFAULT_MAX_SNAPSHOT_WIRE_BYTES, DEFAULT_MAX_TOTAL_SNAPSHOTS,
    DEFAULT_MAX_TOTAL_SNAPSHOT_WIRE_BYTES,
};
pub use observation::{publish_crawl_observation, read_crawl_observation, CRAWL_OBSERVATION_TYPE};
pub use rerank::local_rerank;
pub use segment::{publish_index_segment, read_index_segment, INDEX_SEGMENT_TYPE};

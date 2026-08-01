//! MiniSearch Track F1/F2/F3/F4/F7 (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29): the signed, content-addressed exchange format for crawl
//! observations (F1) and index segments (F2), deterministic merging of
//! per-provider query results while preserving provenance (F3), local
//! re-ranking of a merged result set under a caller's own profile with no
//! re-query (F4), and a local index over a URL's observation history (F7).
//!
//! ## What's implemented here
//!
//! [`publish_crawl_observation`]/[`read_crawl_observation`] and
//! [`publish_index_segment`]/[`read_index_segment`] each wrap an
//! already-produced [`mini_web_types::CrawlObservation`] or
//! [`mini_lexical_index::IndexSegment`] in a signed, content-addressed
//! [`mini_objects::Object`] -- the identical pattern `mini-media`'s
//! `publish_media` and `mini-forge`'s `git_import` already use for their
//! own payloads. Both readers reject malformed or non-canonical bytes;
//! neither re-derives signature verification, which stays the caller's
//! job via [`mini_objects::Object::verify_signature`], the same two-step
//! pattern every signed-object reader in this workspace already follows.
//!
//! [`federate_query`] runs the *unmodified* `mini_query::search` once per
//! [`FederationSource`] and deterministically merges the results (see
//! [`crate::federate`]'s own docs for the exact policy) -- it re-ranks
//! nothing and adds no new scoring, only merging.
//!
//! [`local_rerank`] recomputes each already-merged result's score under a
//! different, caller-chosen [`mini_web_types::RankingProfile`] with no
//! re-query, via `mini_ranker::rescore` against the result's own
//! already-attached signal breakdown (see [`crate::rerank`]'s own docs).
//!
//! [`SnapshotIndex`] indexes a URL's observation history (each recorded
//! via F1's `publish_crawl_observation`) by time, so a caller can ask what
//! a page looked like at a given moment or which observations represent a
//! distinct version rather than a repeat fetch of unchanged content (see
//! [`crate::history`]'s own docs).
//!
//! ## What's deliberately NOT here
//!
//! No network transport, no peer discovery, and no scheduling of what to
//! request from whom -- `federate_query` takes already-available local
//! sources, it does not fetch anything over a network. No provider
//! payments (F5) or private query transport (F6). This crate provides the
//! wire format, merge function, local re-ranking, and history index two or
//! more peers' results would need before any of that can be built.

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
pub use history::{Snapshot, SnapshotIndex};
pub use observation::{publish_crawl_observation, read_crawl_observation, CRAWL_OBSERVATION_TYPE};
pub use rerank::local_rerank;
pub use segment::{publish_index_segment, read_index_segment, INDEX_SEGMENT_TYPE};

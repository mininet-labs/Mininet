//! MiniSearch Track F1/F2 (`docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29): the signed, content-addressed exchange format for crawl
//! observations (F1) and index segments (F2).
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
//! ## What's deliberately NOT here
//!
//! No network transport, no peer discovery, no scheduling of what to
//! request from whom, no deduplication policy across peers, and no
//! federated query merging (Track F3, not started). This crate provides
//! the wire format two peers would need to agree on before any of that
//! can be built -- it is not itself the exchange.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod observation;
mod segment;

pub use error::{FederationError, Result};
pub use observation::{publish_crawl_observation, read_crawl_observation, CRAWL_OBSERVATION_TYPE};
pub use segment::{publish_index_segment, read_index_segment, INDEX_SEGMENT_TYPE};

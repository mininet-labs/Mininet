//! MiniSearch lexical index (Track E5 of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! §E5). A deterministic, immutable inverted index over document fields,
//! with phrase positions and a content-addressed segment manifest.
//!
//! ## What's implemented here
//!
//! - [`IndexBuilder`]: accumulate documents ([`UrlId`] + per-[`Field`]
//!   text), then freeze them into an [`IndexSegment`].
//! - [`IndexSegment`]: an inverted index answering structural queries —
//!   [`IndexSegment::term_documents`] and [`IndexSegment::phrase_documents`]
//!   (consecutive terms within a single field, via stored positions) — plus
//!   canonical [`IndexSegment::to_bytes`]/[`IndexSegment::from_bytes`], a
//!   BLAKE3 [`IndexSegment::segment_id`], and an [`IndexManifest`].
//! - [`tokenize`]: the one deterministic tokenizer both indexing and
//!   querying use.
//!
//! ## What's deliberately NOT implemented
//!
//! No ranking, scoring, or ordering by relevance (Track E6). No crawler,
//! fetcher, extractor, or network (Tracks E3/E4). No query parser or CLI
//! (Track E7). No storage backend — a segment is a plain value the caller
//! stores wherever it likes. And, per D-0312, **no payment, provider,
//! ranking-authority, or governance-weight field anywhere**: an index
//! segment records what text exists where, and nothing about what any of it
//! is worth or who paid for it.
//!
//! ## Why determinism is load-bearing
//!
//! [`UrlId`]/[`IndexSegmentId`] are content addresses. A segment's id is
//! the BLAKE3 digest of its canonical bytes, so the same documents always
//! produce the same segment and the same id, regardless of insertion order
//! or host. That is what lets D-0312's plurality work: many participants
//! can build index segments from the same crawl observations, cache and
//! replicate them by id, and compare or merge them without trusting whoever
//! built them — and a ranker (E6) can be given a segment by id and reason
//! about it reproducibly. [`IndexSegment::from_bytes`] enforces canonical
//! form on decode, so the bytes↔segment mapping stays one-to-one and the
//! id means exactly one thing.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod segment;
mod token;

pub use error::{LexicalIndexError, Result};
pub use segment::{IndexBuilder, IndexManifest, IndexSegment, Occurrence, SEGMENT_FORMAT_VERSION};
pub use token::{tokenize, Field, MAX_TOKEN_CHARS};

// Re-exported so callers name documents and segments with the same types
// the rest of MiniSearch uses.
pub use mini_web_types::{IndexSegmentId, UrlId};

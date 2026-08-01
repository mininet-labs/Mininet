//! MiniSearch query parser and result provenance (Track E of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! §E7-E8) -- the last two pieces of the Track E ranking pipeline
//! (`mini-web-types` → `mini-lexical-index` → `mini-ranker` → this crate).
//!
//! ## What's implemented here
//!
//! - **E7, query parsing** ([`parse_query`]): a deterministic, hand-rolled
//!   parser over a fixed token grammar -- exact phrases (`"..."`), single-
//!   term exclusion (`-word`), a host filter (`site:`/`host:`), inclusive
//!   date bounds (`before:`/`after:`, `YYYY-MM-DD`), a language filter
//!   (`lang:`), and a media-type filter (`type:`). See [`parse`] for the
//!   full grammar and its malformed-input posture (drop silently, never
//!   fail the whole query).
//! - **E8, result provenance** ([`search`], [`ResultProvenance`]): each
//!   ranked [`mini_web_types::SearchResult`] is paired with the crawl
//!   observation that produced it and the index segment it was ranked
//!   from. The ranking profile and per-signal score breakdown were already
//!   on `SearchResult` from Track E6 ([`mini_web_types::RankingExplanation`]);
//!   this crate does not duplicate them.
//!
//! ## What's deliberately NOT here
//!
//! No crawler, extractor, storage, or network client. No new ranking
//! signals, and no change to [`mini_ranker::rank`] itself -- this crate's
//! [`search`] composes it by narrowing the [`mini_ranker::Corpus`] it is
//! handed (see [`search`]'s own docs for the mechanism), never by
//! reimplementing scoring or candidate selection. No CLI binary; that is a
//! caller's job, this is the library the CLI would call.
//!
//! ## Unpersonalized mode
//!
//! There is no personalization token in this parser's grammar and no
//! per-user state anywhere in this crate's types, because
//! `mini_ranker::rank` itself takes none (D-0312's "no personalization by
//! default" holds by construction all the way through E7/E8, not by a flag
//! this layer could fail to set).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod context;
mod error;
mod parse;
mod search;

pub use context::{DocumentContext, DocumentContextTable};
pub use error::{QueryError, Result};
pub use parse::{parse_query, ParsedQuery};
pub use search::{search, ResultProvenance};

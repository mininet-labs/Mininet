//! MiniSearch transparent ranker (Track E6 of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! §E6). Turns a query, a lexical index ([`mini_lexical_index::IndexSegment`]),
//! per-document metadata, and a versioned [`RankingProfile`] into a
//! deterministic ordering of displayable results.
//!
//! ## What's implemented here
//!
//! [`rank`] scores each matching document with six transparent signals —
//! lexical relevance, phrase match, a basic link signal, freshness,
//! originality (exact-duplicate removal), and domain diversity — combines
//! them under the profile's declared weights, and returns
//! [`mini_web_types::SearchResult`]s each carrying a
//! [`mini_web_types::RankingExplanation`] that breaks the score down by
//! signal. [`Query`] is the structured query, [`Corpus`]/[`DocumentMeta`]
//! the per-document metadata, and the [`signals`] module the individual
//! scoring functions. [`rescore`] recomputes a final score from an
//! already-computed `RankingExplanation` under a *different* profile's
//! weights, with no index/corpus/query needed — the same weighted-average
//! formula `rank` itself uses, exposed so Track F4's local re-ranking
//! (`mini-search-federation`) never has to re-derive it.
//!
//! ## What's deliberately NOT here
//!
//! No query parser or CLI (Track E7). No result provenance beyond the
//! explanation (Track E8). No crawler, fetcher, extractor, network client,
//! or storage. No learned ranking, click feedback, or link-graph analysis —
//! the link signal is an explicit bounded placeholder.
//!
//! ## The doctrine, enforced structurally (D-0312)
//!
//! - **No pay-to-rank.** [`rank`] has no payment, bid, or provider input in
//!   its signature. Ranking cannot be bought because there is nothing to
//!   buy it with.
//! - **No personalization by default.** The ranker takes no per-user state,
//!   so the public default of no personalization holds by construction, not
//!   by a flag that could be flipped.
//! - **Availability is not a relevance penalty.** Restricted or unavailable
//!   documents are filtered out before scoring, never scored down, so an
//!   availability decision cannot be laundered into the relevance number.
//! - **Deterministic ordering.** Every score is integer, the only time
//!   input is an explicit `now_ms`, and every tie breaks on `UrlId` bytes —
//!   so the same query, index, profile, and time produce byte-identical
//!   results anywhere.
//! - **Explicit, forkable profile.** The weights and version live in the
//!   caller's [`RankingProfile`]; a different community can rank the same
//!   index differently by supplying a different profile, and every result
//!   names the profile that produced it.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod corpus;
mod error;
mod query;
mod rank;
pub mod signals;

pub use corpus::{Corpus, DocumentMeta};
pub use error::{RankerError, Result};
pub use query::Query;
pub use rank::{rank, rescore};

// Re-exported so a caller names profiles, results, and weights with the
// same vocabulary the rest of MiniSearch uses.
pub use mini_web_types::{RankingProfile, RankingProfileId, SearchResult, WeightBps};

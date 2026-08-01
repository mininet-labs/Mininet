//! Compose [`crate::parse_query`]'s filters with the unmodified
//! [`mini_ranker::rank`] pass, and attach Track E8 result provenance.
//!
//! `mini_ranker::rank` already has one, and only one, mechanism for
//! removing a document from consideration without turning that removal into
//! a relevance penalty: `AvailabilityState::Restricted`, checked before any
//! signal is computed (D-0312's "availability is a filter, not a score"
//! rule). This module reuses exactly that mechanism for the parser's own
//! filters (`site:`, `-word`, `before:`/`after:`, `lang:`, `type:`): a
//! document failing a user filter is relabeled `Restricted(UserFilter)` in
//! a cloned corpus before `rank` ever sees it, so a `site:` filter and a
//! robots-exclusion restriction are excluded through the identical path,
//! and `rank` itself is never modified or reimplemented.

use std::collections::{HashMap, HashSet};

use mini_lexical_index::IndexSegment;
use mini_ranker::{rank, Corpus};
use mini_web_types::{
    AvailabilityState, CrawlObservationId, IndexSegmentId, RankingProfile, RestrictionReason,
    SearchResult, UrlId,
};

use crate::context::DocumentContextTable;
use crate::error::{QueryError, Result};
use crate::parse::ParsedQuery;

/// One ranked result plus Track E8 provenance: which crawl observation
/// produced it and which index segment it was ranked from. The ranking
/// profile and per-signal score breakdown are already on `result.explanation`
/// / `result.ranking_profile` -- this wraps rather than duplicates them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultProvenance {
    pub result: SearchResult,
    pub source_observation: CrawlObservationId,
    pub index_segment: IndexSegmentId,
}

/// Parse-and-rank in one call: builds a [`mini_ranker::Query`] from `parsed`,
/// applies its filters by cloning `corpus` and marking non-matching
/// documents `Restricted(UserFilter)`, ranks the result with the unmodified
/// [`mini_ranker::rank`], and attaches provenance from `contexts` plus the
/// caller-supplied `index_segment` identity.
#[allow(clippy::too_many_arguments)]
pub fn search(
    index: &IndexSegment,
    corpus: &Corpus,
    contexts: &DocumentContextTable,
    profile: &RankingProfile,
    parsed: &ParsedQuery,
    index_segment: IndexSegmentId,
    now_ms: u64,
    max_results: usize,
) -> Result<Vec<ResultProvenance>> {
    let mut query = mini_ranker::Query::new(parsed.terms.iter().cloned());
    if let Some(phrase) = &parsed.phrase {
        query = query.with_phrase(phrase.clone());
    }

    let excluded_ids: HashSet<Vec<u8>> = parsed
        .excluded_terms
        .iter()
        .flat_map(|term| index.term_documents(term))
        .map(|id| id.0.to_bytes())
        .collect();

    let mut filtered = Corpus::new();
    let mut canonical_to_id: HashMap<String, UrlId> = HashMap::new();
    for id in index.documents() {
        let Some(meta) = corpus.get(id) else {
            continue;
        };
        canonical_to_id.insert(meta.url.canonical_string(), id.clone());

        let mut meta = meta.clone();
        if !document_matches_filters(&meta, contexts.get(id), id, &excluded_ids, parsed) {
            meta.availability = AvailabilityState::Restricted(RestrictionReason::UserFilter);
        }
        filtered.insert(id, meta);
    }

    let ranked = rank(index, &filtered, profile, &query, now_ms, max_results)?;

    let mut out = Vec::with_capacity(ranked.len());
    for result in ranked {
        let id = canonical_to_id
            .get(&result.url.canonical_string())
            .ok_or(QueryError::MissingDocumentContext)?;
        let ctx = contexts.get(id).ok_or(QueryError::MissingDocumentContext)?;
        out.push(ResultProvenance {
            result,
            source_observation: ctx.source_observation.clone(),
            index_segment: index_segment.clone(),
        });
    }
    Ok(out)
}

/// Whether one document survives every filter present in `parsed`. A filter
/// that was not given in the query always passes. `excluded_ids` is
/// precomputed once per call, not per document.
fn document_matches_filters(
    meta: &mini_ranker::DocumentMeta,
    ctx: Option<&crate::context::DocumentContext>,
    id: &UrlId,
    excluded_ids: &HashSet<Vec<u8>>,
    parsed: &ParsedQuery,
) -> bool {
    if excluded_ids.contains(&id.0.to_bytes()) {
        return false;
    }
    if let Some(host) = &parsed.host_filter {
        if meta.url.host != *host {
            return false;
        }
    }
    if let Some(before_ms) = parsed.before_ms {
        if meta.observed_at_ms >= before_ms {
            return false;
        }
    }
    if let Some(after_ms) = parsed.after_ms {
        if meta.observed_at_ms < after_ms {
            return false;
        }
    }
    if let Some(language) = &parsed.language {
        let matches = ctx
            .and_then(|c| c.language.as_deref())
            .is_some_and(|l| l.eq_ignore_ascii_case(language));
        if !matches {
            return false;
        }
    }
    if let Some(media_type) = &parsed.media_type {
        let matches = ctx.and_then(|c| c.media_type.as_ref()) == Some(media_type);
        if !matches {
            return false;
        }
    }
    true
}

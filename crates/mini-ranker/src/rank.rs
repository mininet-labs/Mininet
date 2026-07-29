//! The ranker: turn a query, a lexical index, document metadata, and a
//! versioned ranking profile into a deterministic ordering of displayable
//! results.
//!
//! Determinism is total. The only time input is the explicit `now_ms`
//! argument; every score is integer; and every ordering tie is broken by
//! `UrlId` bytes. Given the same index, corpus, profile, query, and
//! `now_ms`, `rank` returns byte-identical results on any machine — the
//! reproducibility D-0312 requires.
//!
//! What the ranker structurally cannot do: there is no payment, provider,
//! or bid input anywhere in its signature, so ranking cannot be bought
//! (D-0312's no-pay-to-rank rule is enforced by absence, not by policy).
//! Personalization is never applied — the ranker takes no per-user state —
//! so the public default of no personalization holds by construction.
//! Restricted or unavailable documents are excluded outright, never scored
//! down, so an availability decision is never laundered into a relevance
//! penalty.

use std::collections::{BTreeMap, HashMap};

use mini_lexical_index::IndexSegment;
use mini_web_types::{RankingExplanation, RankingProfile, SearchResult, UrlId, WeightBps};

use crate::corpus::{Corpus, DocumentMeta};
use crate::error::{RankerError, Result};
use crate::query::Query;
use crate::signals;

/// The raw per-signal scores for one document, before profile weighting.
#[derive(Debug, Clone, Copy)]
struct Signals {
    lexical: u16,
    phrase: u16,
    link: u16,
    freshness: u16,
    originality: u16,
}

/// A survivor of candidate selection and duplicate removal, carrying its
/// content signals and the data needed to finish scoring and display.
struct Candidate<'a> {
    id: UrlId,
    id_bytes: Vec<u8>,
    meta: &'a DocumentMeta,
    signals: Signals,
}

/// Rank the documents matching `query` and return up to `max_results`
/// displayable [`SearchResult`]s, best first.
///
/// Errors with [`RankerError::MissingDocumentMetadata`] if the index
/// references a document the corpus does not describe: that means the index
/// and corpus were built from different data, a wiring bug worth surfacing
/// rather than papering over with a blank result.
pub fn rank(
    index: &IndexSegment,
    corpus: &Corpus,
    profile: &RankingProfile,
    query: &Query,
    now_ms: u64,
    max_results: usize,
) -> Result<Vec<SearchResult>> {
    if query.is_empty() || max_results == 0 {
        return Ok(Vec::new());
    }

    // Map each document's UrlId bytes to its index position, so postings
    // (which reference documents by position) can be tied back to metadata.
    let doc_index: HashMap<Vec<u8>, u32> = index
        .documents()
        .iter()
        .enumerate()
        .map(|(i, u)| (u.0.to_bytes(), i as u32))
        .collect();

    // Documents matching the exact phrase, if one was requested.
    let phrase_hits: std::collections::HashSet<Vec<u8>> = match query.phrase() {
        Some(p) => index
            .phrase_documents(p)
            .into_iter()
            .map(|u| u.0.to_bytes())
            .collect(),
        None => std::collections::HashSet::new(),
    };

    // Candidate set: union of the documents containing each query term,
    // deduplicated and kept in canonical (UrlId byte) order via BTreeMap.
    let mut candidate_ids: BTreeMap<Vec<u8>, UrlId> = BTreeMap::new();
    for term in query.terms() {
        for url in index.term_documents(term) {
            candidate_ids.insert(url.0.to_bytes(), url);
        }
    }

    // Resolve metadata, filter to displayable, compute content signals.
    let total_terms = query.terms().len() as u32;
    let mut scored: Vec<Candidate> = Vec::new();
    for (id_bytes, id) in &candidate_ids {
        let meta = corpus.get(id).ok_or(RankerError::MissingDocumentMetadata)?;

        // Availability is a filter, not a signal: non-Available documents
        // are excluded here and never scored, so a restriction cannot be
        // silently expressed as a low relevance score.
        if !meta.availability.is_displayable() {
            continue;
        }

        let (matched, occurrences) = match doc_index.get(id_bytes) {
            Some(&di) => term_stats(index, query, di),
            None => (0, 0),
        };

        let sig = Signals {
            lexical: signals::lexical(matched, total_terms, occurrences),
            phrase: signals::phrase(phrase_hits.contains(id_bytes)),
            link: signals::link(meta.inbound_links),
            freshness: signals::freshness(meta.observed_at_ms, now_ms),
            originality: signals::originality_kept(),
        };

        scored.push(Candidate {
            id: id.clone(),
            id_bytes: id_bytes.clone(),
            meta,
            signals: sig,
        });
    }

    remove_duplicates(&mut scored);

    // Greedy, diversity-aware selection: at each step pick the highest
    // profile-weighted score given how many already-emitted results share
    // the candidate's host, so one domain is demoted (not deleted) as it
    // repeats. Ties break on UrlId bytes, keeping the whole pass
    // deterministic.
    let mut host_count: HashMap<String, u32> = HashMap::new();
    let mut results: Vec<SearchResult> = Vec::new();
    let mut remaining: Vec<Candidate> = scored;

    while results.len() < max_results && !remaining.is_empty() {
        let mut best: Option<(usize, u16, u16)> = None; // (idx, final_score, diversity)
        for (i, cand) in remaining.iter().enumerate() {
            let host = cand.meta.url.host.as_str().to_string();
            let prior = *host_count.get(&host).unwrap_or(&0);
            let diversity = signals::diversity(prior);
            let final_score = combine(&cand.signals, diversity, profile);

            let better = match best {
                None => true,
                Some((bi, bscore, _)) => {
                    final_score > bscore
                        || (final_score == bscore && remaining[i].id_bytes < remaining[bi].id_bytes)
                }
            };
            if better {
                best = Some((i, final_score, diversity));
            }
        }

        let (idx, final_score, diversity) = best.expect("remaining non-empty");
        let cand = remaining.remove(idx);
        let host = cand.meta.url.host.as_str().to_string();
        *host_count.entry(host).or_insert(0) += 1;

        results.push(build_result(&cand, final_score, diversity, profile)?);
    }

    Ok(results)
}

/// Count how many query terms occur in document `di`, and the total number
/// of positions those terms occupy there (across all fields).
fn term_stats(index: &IndexSegment, query: &Query, di: u32) -> (u32, u32) {
    let mut matched = 0u32;
    let mut occurrences = 0u32;
    for term in query.terms() {
        let Some(postings) = index.postings(term) else {
            continue;
        };
        let mut here = 0u32;
        for occ in postings {
            if occ.doc == di {
                here = here.saturating_add(occ.positions.len() as u32);
            }
        }
        if here > 0 {
            matched += 1;
            occurrences = occurrences.saturating_add(here);
        }
    }
    (matched, occurrences)
}

/// Drop exact duplicates: among documents sharing a content digest, keep
/// the original (earliest observed, then smallest UrlId) and remove the
/// rest. Deterministic regardless of input order.
fn remove_duplicates(scored: &mut Vec<Candidate>) {
    // digest bytes -> index of the kept representative in `scored`.
    let mut keep: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut drop: Vec<bool> = vec![false; scored.len()];

    for i in 0..scored.len() {
        let digest = scored[i].meta.content_digest.to_bytes();
        match keep.get(&digest).copied() {
            None => {
                keep.insert(digest, i);
            }
            Some(j) => {
                // Keep whichever is the "original": earlier observation,
                // then smaller UrlId. Drop the other.
                let keep_i = (scored[i].meta.observed_at_ms, &scored[i].id_bytes)
                    < (scored[j].meta.observed_at_ms, &scored[j].id_bytes);
                if keep_i {
                    drop[j] = true;
                    keep.insert(digest, i);
                } else {
                    drop[i] = true;
                }
            }
        }
    }

    let mut idx = 0;
    scored.retain(|_| {
        let d = drop[idx];
        idx += 1;
        !d
    });
}

/// Combine the six signals under the profile's six weights: a weighted
/// average in basis points, normalized by the actual weight sum so a
/// forked profile need not make its weights total 10000.
fn combine(sig: &Signals, diversity: u16, profile: &RankingProfile) -> u16 {
    let terms: [(u64, u64); 6] = [
        (sig.lexical as u64, profile.lexical_weight.value() as u64),
        (sig.phrase as u64, profile.phrase_weight.value() as u64),
        (sig.link as u64, profile.link_weight.value() as u64),
        (
            sig.freshness as u64,
            profile.freshness_weight.value() as u64,
        ),
        (
            sig.originality as u64,
            profile.originality_weight.value() as u64,
        ),
        (diversity as u64, profile.diversity_weight.value() as u64),
    ];
    let weight_sum: u64 = terms.iter().map(|(_, w)| *w).sum();
    if weight_sum == 0 {
        return 0;
    }
    let weighted: u64 = terms.iter().map(|(s, w)| s * w).sum();
    (weighted / weight_sum).min(signals::BPS_MAX as u64) as u16
}

fn build_result(
    cand: &Candidate,
    final_score: u16,
    diversity: u16,
    profile: &RankingProfile,
) -> Result<SearchResult> {
    let explanation = RankingExplanation {
        lexical_bps: WeightBps::new(cand.signals.lexical)?,
        phrase_bps: WeightBps::new(cand.signals.phrase)?,
        link_bps: WeightBps::new(cand.signals.link)?,
        freshness_bps: WeightBps::new(cand.signals.freshness)?,
        originality_bps: WeightBps::new(cand.signals.originality)?,
        diversity_bps: WeightBps::new(diversity)?,
    };
    // The ranker only builds results for `Available` documents, so the
    // displayable constructor is always the correct one here.
    let _ = &cand.id;
    Ok(SearchResult::displayable(
        cand.meta.url.clone(),
        cand.meta.title.clone(),
        cand.meta.snippet.clone(),
        WeightBps::new(final_score)?,
        profile.id.clone(),
        explanation,
    ))
}

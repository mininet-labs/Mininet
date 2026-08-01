//! F4: local re-ranking — "Users apply their chosen profile locally"
//! (`docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §29).
//!
//! [`local_rerank`] takes an already-merged [`crate::FederatedResult`] list
//! (typically [`crate::federate_query`]'s own output, though nothing
//! requires that) and recomputes each result's final score under a
//! *different*, caller-chosen [`RankingProfile`] — with no index, corpus,
//! or network round trip. It does this by calling
//! [`mini_ranker::rescore`] against each result's already-attached
//! [`mini_web_types::RankingExplanation`], which was produced once (at
//! query time, under whatever shared profile [`crate::federate_query`]
//! used) and carries the six per-signal scores that any profile's weights
//! can be recombined against. This is the same weighted-average formula
//! [`mini_ranker::rank`] itself uses internally, exposed rather than
//! reimplemented, so a re-ranked score can never silently drift from what
//! a fresh `rank` call under the same profile would have produced.
//!
//! ## What this does not do
//!
//! It does not recompute the `diversity_bps` signal, which depends on the
//! result set's own ordering at original ranking time, not a raw
//! per-document property — recomputing it under a new order is a
//! materially different operation (re-running the greedy diversity-aware
//! selection loop) this function does not attempt. It reuses the
//! originally-computed diversity signal as-is. A caller wanting diversity
//! re-evaluated under the new order needs a fresh `rank`/`federate_query`
//! call, not this function.

use mini_ranker::rescore;
use mini_web_types::RankingProfile;

use crate::error::Result;
use crate::federate::FederatedResult;

/// Recompute each result's score under `profile` and return the list
/// re-sorted by the new scores (descending, canonical-URL-string
/// tiebreak — the identical tiebreak convention [`crate::federate_query`]
/// uses), truncated to `max_results`. Every result's `ranking_profile`
/// field is updated to `profile.id` so it honestly names the profile that
/// actually produced its displayed score.
pub fn local_rerank(
    results: &[FederatedResult],
    profile: &RankingProfile,
    max_results: usize,
) -> Result<Vec<FederatedResult>> {
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        let new_score = rescore(&r.result.result.explanation, profile)?;
        let mut rescored = r.clone();
        rescored.result.result.relevance_score_bps = new_score;
        rescored.result.result.ranking_profile = profile.id.clone();
        out.push(rescored);
    }
    out.sort_by(|a, b| {
        b.result
            .result
            .relevance_score_bps
            .value()
            .cmp(&a.result.result.relevance_score_bps.value())
            .then_with(|| {
                a.result
                    .result
                    .url
                    .canonical_string()
                    .cmp(&b.result.result.url.canonical_string())
            })
    });
    out.truncate(max_results);
    Ok(out)
}

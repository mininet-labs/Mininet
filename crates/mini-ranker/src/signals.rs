//! The six ranking signals, each a deterministic function producing a
//! basis-point score in `0..=10_000`.
//!
//! Every signal is **integer-only**. No floating point appears anywhere in
//! scoring, because a ranker whose output must be byte-reproducible across
//! machines (D-0312: "the same query, index set, and ranking profile
//! produce deterministic ordering") cannot depend on float rounding that
//! may differ by platform or compiler. Basis points (0..10000) give ample
//! resolution with exact integer arithmetic.
//!
//! Each signal is intentionally simple and documented. A signal is a place
//! a future forkable profile can refine; the point of this first slice is
//! that every number is explainable, not that it is state-of-the-art.

/// Full-scale basis-point value.
pub const BPS_MAX: u16 = 10_000;

/// Lexical relevance: how well the document's text covers the query terms.
///
/// `matched` is the number of distinct query terms that appear in the
/// document; `total` is the number of query terms; `occurrences` is the
/// total number of positions those matched terms occupy in the document.
///
/// Coverage dominates (a document containing more of the query terms is
/// more relevant), and term frequency is a bounded secondary boost, so a
/// page cannot climb by repeating one word — a mild, transparent guard
/// against keyword stuffing rather than a full spam model.
pub fn lexical(matched: u32, total: u32, occurrences: u32) -> u16 {
    if total == 0 || matched == 0 {
        return 0;
    }
    // Coverage worth up to 9000 bps.
    let coverage = (matched as u64 * 9_000) / total as u64;
    // Frequency worth up to 1000 bps, saturating quickly (50 bps per
    // occurrence) so it refines ties without dominating coverage.
    let freq = (occurrences as u64 * 50).min(1_000);
    (coverage + freq).min(BPS_MAX as u64) as u16
}

/// Phrase match: full credit when the query's exact phrase occurs (its
/// tokens adjacent within one field), nothing otherwise. Binary because a
/// phrase either occurs or it does not; partial-phrase credit would blur
/// into lexical coverage, which already accounts for the individual words.
pub fn phrase(matched_phrase: bool) -> u16 {
    if matched_phrase {
        BPS_MAX
    } else {
        0
    }
}

/// Basic link signal: more inbound links is a weak popularity hint,
/// log-scaled so it saturates and a link farm cannot buy unbounded score.
/// 0 links → 0; then roughly +1000 bps per doubling, capped at full scale.
/// "Basic" is deliberate — the research document lists a real link-graph
/// signal as later work; this is a bounded placeholder that cannot dominate.
pub fn link(inbound_links: u32) -> u16 {
    if inbound_links == 0 {
        return 0;
    }
    // ilog2(1)=0 -> 1000; ilog2(2)=1 -> 2000; ... ilog2(512)=9 -> 10000.
    let steps = inbound_links.ilog2() as u64 + 1;
    (steps * 1_000).min(BPS_MAX as u64) as u16
}

/// Freshness: newer documents score higher, computed against an explicit
/// query time so ranking is reproducible. Full scale within the first week,
/// then halving per subsequent week, reaching ~0 after roughly a quarter.
/// A document observed in the future (clock skew) is treated as brand new
/// rather than producing a nonsense age.
pub fn freshness(observed_at_ms: u64, now_ms: u64) -> u16 {
    let age_ms = now_ms.saturating_sub(observed_at_ms);
    let age_weeks = age_ms / (7 * 24 * 60 * 60 * 1_000);
    // Halve per week, saturating to 0 once the shift empties the value
    // (10_000 >> 14 == 0).
    let shift = age_weeks.min(14) as u32;
    (BPS_MAX as u32 >> shift) as u16
}

/// Originality: full scale for a document the ranker kept, because exact
/// duplicates are removed before scoring (the removed copies never reach a
/// score at all). It is a constant among emitted results in this first
/// slice; near-duplicate (non-identical) detection is future work, and this
/// signal is where it would attach.
pub fn originality_kept() -> u16 {
    BPS_MAX
}

/// Domain diversity: the first result from a host scores full; each further
/// result from the same host scores half the previous one. This demotes,
/// but never entirely removes, additional pages from one domain, so a
/// single site cannot monopolize the results while a genuinely more
/// relevant deeper page from that site can still surface.
///
/// `prior_from_host` is how many higher-ranked results already came from
/// this document's host.
pub fn diversity(prior_from_host: u32) -> u16 {
    (BPS_MAX as u32 >> prior_from_host.min(14)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexical_rewards_coverage_over_frequency() {
        // Two of two terms present beats one of two, regardless of how many
        // times the single term repeats.
        let both = lexical(2, 2, 2);
        let one_repeated = lexical(1, 2, 20);
        assert!(both > one_repeated);
        assert!(both <= BPS_MAX);
    }

    #[test]
    fn lexical_is_zero_without_a_match() {
        assert_eq!(lexical(0, 3, 0), 0);
        assert_eq!(lexical(1, 0, 5), 0);
    }

    #[test]
    fn link_is_log_scaled_and_capped() {
        assert_eq!(link(0), 0);
        assert_eq!(link(1), 1_000);
        assert_eq!(link(2), 2_000);
        assert_eq!(link(4), 3_000);
        assert_eq!(link(1_000_000), BPS_MAX);
    }

    #[test]
    fn freshness_halves_each_week_and_never_underflows() {
        let week = 7 * 24 * 60 * 60 * 1_000u64;
        assert_eq!(freshness(1_000, 1_000), BPS_MAX); // same instant
        assert_eq!(freshness(0, week), 5_000); // one week old
        assert_eq!(freshness(0, 2 * week), 2_500); // two weeks
        assert_eq!(freshness(0, 100 * week), 0); // ancient -> 0
        assert_eq!(freshness(week, 0), BPS_MAX); // future -> new
    }

    #[test]
    fn diversity_halves_per_repeat() {
        assert_eq!(diversity(0), BPS_MAX);
        assert_eq!(diversity(1), 5_000);
        assert_eq!(diversity(2), 2_500);
        assert_eq!(diversity(100), 0);
    }
}

//! An open-ended, multi-signal human-status accumulator — the founder's
//! generalization of the whitepaper's fixed three-signal design (SS5):
//! instead of exactly three hardcoded signals, any number of verification
//! methods may each contribute weighted evidence toward one identity
//! root's status, with Mininet's own methods trusted most but third-party
//! or future methods free to add up too.
//!
//! ## The model
//!
//! - Each verification method is a [`SignalSource`] — extensible (an
//!   `External` catch-all variant lets new methods exist without a crate
//!   release) rather than a closed enum of exactly three.
//! - [`TrustWeights`] says how much *we* trust each source — our own
//!   [`SignalSource::PhysicalPresence`] and [`SignalSource::VouchingGraph`]
//!   outweigh anything external by default, matching "us trusting our own
//!   the most."
//! - A [`HumanRecord`] accumulates [`SignalEvidence`] over time from
//!   whichever sources an identity root has satisfied. Only a derived
//!   strength score and a timestamp ever enter this record — the raw data
//!   behind any signal (accelerometer traces, presence details, whatever a
//!   future method uses) stays wherever it was computed and never appears
//!   here (P5: no raw personal data).
//! - [`HumanRecord::status`] fuses all currently-live (non-decayed)
//!   evidence into one score and decides [`HumanStatus`]: an identity
//!   starts `Unverified`, reaches `VouchedHuman` quickly once *any* modest
//!   trusted evidence exists (fast-path, e.g. one social vouch), and is
//!   promoted to `FullHuman` **only automatically**, requiring a high
//!   fused score, evidence from several distinct sources, *and* a minimum
//!   elapsed time since the record's first evidence — never all at once,
//!   however strong a single signal is.
//!
//! ## Why this makes Sybil expensive without needing a single silver bullet
//!
//! No individual signal has to be unbreakable. A farm must satisfy several
//! *independent* verification methods (each with its own real-world cost
//! to fake), sustain them long enough that decay doesn't erase them, and
//! wait out the mandatory minimum age before `FullHuman` is even reachable
//! — stacking one very convincing fake signal is explicitly insufficient
//! by construction ([`PromotionPolicy::full_minimum_distinct_sources`]).
//! This is the same "by the time a fake operation is profitable it is
//! nearly indistinguishable from genuine adoption" property the whitepaper
//! describes (SS11), generalized from three fixed signals to as many as
//! the network ends up supporting.

use std::collections::HashMap;

use crate::confidence::DecayPolicy;

/// Which verification method a piece of evidence came from. `#[non_exhaustive]`
/// plus the `External` catch-all mean new methods never require a breaking
/// change here — they just start out trusted at
/// [`TrustWeights::default_external_weight`] until the founder cohort
/// decides otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SignalSource {
    /// This crate's own social-vouching graph ([`crate::graph`]).
    VouchingGraph,
    /// `mini_presence::PresenceVerdict` physical co-presence attestations.
    PhysicalPresence,
    /// A behavioral/location entropy proof, if a real implementation ever
    /// exists (see [`crate::confidence::BehavioralEntropySource`]'s honest
    /// limit — not implemented today, but the slot exists so it can plug
    /// in as just one more contributing signal rather than a hardcoded
    /// third input).
    BehavioralEntropy,
    /// Any other verification method, identified by a caller-defined tag.
    /// Not one of Mininet's own methods, so trusted least by default.
    External(u32),
}

/// One piece of evidence toward an identity root's human status: a
/// derived strength score from a specific source, recorded at a point in
/// time. Never the raw data behind that score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalEvidence {
    /// Which method produced this evidence.
    pub source: SignalSource,
    /// This piece of evidence's own strength, 0..=100, as judged by
    /// whatever produced it (e.g. a vouching-graph trust percentage, a
    /// presence-recency score).
    pub strength: u32,
    /// When this evidence was recorded (ms).
    pub recorded_at_ms: u64,
}

/// How much each [`SignalSource`] is trusted, 0..=100. Governs both the
/// fused score's weighting and nothing else — trust weight is deliberately
/// not the same axis as governance weight (P1 unaffected either way).
#[derive(Debug, Clone)]
pub struct TrustWeights {
    weights: HashMap<SignalSource, u32>,
    default_external_weight: u32,
}

impl TrustWeights {
    /// Mininet's own methods weighted highest, matching the whitepaper's
    /// physical-presence-is-strongest framing; anything external starts
    /// low-trust by default. Tunable, not frozen — a founder-governed
    /// parameter, the same "left to caller-supplied parameters" stance
    /// this crate already takes for seed sets and fusion weights.
    pub fn founder_default() -> Self {
        let mut weights = HashMap::new();
        weights.insert(SignalSource::PhysicalPresence, 100);
        weights.insert(SignalSource::VouchingGraph, 70);
        weights.insert(SignalSource::BehavioralEntropy, 60);
        TrustWeights {
            weights,
            default_external_weight: 20,
        }
    }

    /// This source's current trust weight.
    pub fn weight_for(&self, source: SignalSource) -> u32 {
        if let Some(w) = self.weights.get(&source) {
            return *w;
        }
        match source {
            SignalSource::External(_) => self.default_external_weight,
            _ => 0,
        }
    }

    /// Set (or override) a specific source's trust weight — e.g. governance
    /// raising a particular external method's standing over time.
    pub fn set_weight(&mut self, source: SignalSource, weight: u32) {
        self.weights.insert(source, weight);
    }
}

/// The automatic-promotion rule from [`HumanStatus::Unverified`] through
/// [`HumanStatus::VouchedHuman`] to [`HumanStatus::FullHuman`]. Tunable —
/// the whitepaper specifies the shape (fast provisional trust, slow full
/// promotion requiring sustained, diverse, aged evidence), not these exact
/// numbers.
#[derive(Debug, Clone, Copy)]
pub struct PromotionPolicy {
    /// Minimum fused score (0..=100) to reach `VouchedHuman`.
    pub vouched_score_threshold: u32,
    /// Minimum fused score (0..=100) to reach `FullHuman`.
    pub full_score_threshold: u32,
    /// Minimum time since this record's first-ever evidence before
    /// `FullHuman` is reachable, regardless of score — the mandatory
    /// re-earning window that makes farming expensive: a fresh identity
    /// cannot buy its way to full status quickly no matter how many
    /// signals it stacks at once.
    pub full_minimum_age_ms: u64,
    /// Minimum number of *currently live* (non-decayed) distinct sources
    /// required for `FullHuman` — diversity of sustained evidence is
    /// itself part of the cost, so no single strong signal alone promotes.
    pub full_minimum_distinct_sources: usize,
}

impl PromotionPolicy {
    /// A month-scale default: fast provisional trust, full status gated on
    /// roughly a month of sustained, diverse evidence — matching the
    /// whitepaper's "confidence... stays high across months rather than at
    /// a single moment" (SS5).
    pub fn whitepaper_default() -> Self {
        PromotionPolicy {
            vouched_score_threshold: 30,
            full_score_threshold: 70,
            full_minimum_age_ms: 30 * 86_400_000,
            full_minimum_distinct_sources: 2,
        }
    }
}

/// The status an identity root's accumulated evidence currently supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HumanStatus {
    /// Not enough evidence yet.
    Unverified,
    /// Provisionally trusted — the fast path, reachable from modest
    /// trusted evidence (e.g. a single genuine vouch).
    VouchedHuman,
    /// Sustained, diverse, sufficiently-aged evidence — reachable only
    /// automatically, never granted directly.
    FullHuman,
}

/// One identity root's accumulated verification evidence across every
/// [`SignalSource`] it has ever satisfied.
#[derive(Debug, Clone, Default)]
pub struct HumanRecord {
    evidence: Vec<SignalEvidence>,
}

impl HumanRecord {
    /// A new, empty record.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new piece of evidence.
    pub fn record(&mut self, evidence: SignalEvidence) {
        self.evidence.push(evidence);
    }

    /// When this record's first-ever evidence was recorded, if any.
    pub fn first_evidence_at_ms(&self) -> Option<u64> {
        self.evidence.iter().map(|e| e.recorded_at_ms).min()
    }

    /// Time elapsed since the first-ever evidence, at `now_ms`. Zero if
    /// this record has no evidence.
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        self.first_evidence_at_ms()
            .map(|first| now_ms.saturating_sub(first))
            .unwrap_or(0)
    }

    /// The fused, trust-weighted, decay-adjusted score (0..=100) across
    /// every source, using each source's single most recent evidence (a
    /// source that has gone stale contributes less as it decays, but does
    /// not get "topped up" by simply re-summing old entries).
    pub fn score(&self, weights: &TrustWeights, decay: &DecayPolicy, now_ms: u64) -> u32 {
        let mut weighted_sum: u64 = 0;
        let mut total_weight: u64 = 0;
        for (source, evidence) in self.latest_per_source() {
            let age = now_ms.saturating_sub(evidence.recorded_at_ms);
            let decayed_strength =
                u64::from(evidence.strength.min(100)) * u64::from(decay.weight_percent(age)) / 100;
            let w = u64::from(weights.weight_for(source));
            weighted_sum += decayed_strength * w;
            total_weight += w;
        }
        if total_weight == 0 {
            return 0;
        }
        (weighted_sum / total_weight) as u32
    }

    /// How many distinct sources currently contribute non-zero (not fully
    /// decayed) evidence at `now_ms`.
    pub fn distinct_live_sources(&self, decay: &DecayPolicy, now_ms: u64) -> usize {
        self.latest_per_source()
            .into_iter()
            .filter(|(_, e)| {
                let age = now_ms.saturating_sub(e.recorded_at_ms);
                decay.weight_percent(age) > 0 && e.strength > 0
            })
            .count()
    }

    /// This record's currently-supported [`HumanStatus`].
    pub fn status(
        &self,
        policy: &PromotionPolicy,
        weights: &TrustWeights,
        decay: &DecayPolicy,
        now_ms: u64,
    ) -> HumanStatus {
        let score = self.score(weights, decay, now_ms);
        if score >= policy.full_score_threshold
            && self.age_ms(now_ms) >= policy.full_minimum_age_ms
            && self.distinct_live_sources(decay, now_ms) >= policy.full_minimum_distinct_sources
        {
            HumanStatus::FullHuman
        } else if score >= policy.vouched_score_threshold {
            HumanStatus::VouchedHuman
        } else {
            HumanStatus::Unverified
        }
    }

    /// Each source's single most recent piece of evidence.
    fn latest_per_source(&self) -> Vec<(SignalSource, SignalEvidence)> {
        let mut latest: HashMap<SignalSource, SignalEvidence> = HashMap::new();
        for e in &self.evidence {
            latest
                .entry(e.source)
                .and_modify(|existing| {
                    if e.recorded_at_ms > existing.recorded_at_ms {
                        *existing = *e;
                    }
                })
                .or_insert(*e);
        }
        latest.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decay() -> DecayPolicy {
        DecayPolicy {
            full_credit_window_ms: 30 * 86_400_000,
            zero_after_ms: 365 * 86_400_000,
        }
    }

    #[test]
    fn empty_record_is_unverified() {
        let record = HumanRecord::new();
        let status = record.status(
            &PromotionPolicy::whitepaper_default(),
            &TrustWeights::founder_default(),
            &decay(),
            0,
        );
        assert_eq!(status, HumanStatus::Unverified);
    }

    #[test]
    fn a_single_modest_vouch_reaches_vouched_human_quickly() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 50,
            recorded_at_ms: 0,
        });
        let status = record.status(
            &PromotionPolicy::whitepaper_default(),
            &TrustWeights::founder_default(),
            &decay(),
            1_000,
        );
        assert_eq!(status, HumanStatus::VouchedHuman);
    }

    #[test]
    fn full_human_is_never_reached_before_the_minimum_age_even_with_a_high_score() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::PhysicalPresence,
            strength: 100,
            recorded_at_ms: 0,
        });
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 100,
            recorded_at_ms: 0,
        });
        // Score and diversity both satisfied immediately, but no time has
        // passed at all.
        let status = record.status(
            &PromotionPolicy::whitepaper_default(),
            &TrustWeights::founder_default(),
            &decay(),
            0,
        );
        assert_eq!(status, HumanStatus::VouchedHuman);
    }

    #[test]
    fn full_human_is_never_reached_from_a_single_source_no_matter_how_strong() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::PhysicalPresence,
            strength: 100,
            recorded_at_ms: 0,
        });
        let policy = PromotionPolicy::whitepaper_default();
        // Plenty of time has passed, score is maxed, but only one source.
        let status = record.status(
            &policy,
            &TrustWeights::founder_default(),
            &decay(),
            policy.full_minimum_age_ms * 2,
        );
        assert_eq!(status, HumanStatus::VouchedHuman);
    }

    #[test]
    fn full_human_is_reached_with_enough_score_age_and_source_diversity() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::PhysicalPresence,
            strength: 100,
            recorded_at_ms: 0,
        });
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 100,
            recorded_at_ms: 0,
        });
        let policy = PromotionPolicy::whitepaper_default();
        let status = record.status(
            &policy,
            &TrustWeights::founder_default(),
            &decay(),
            policy.full_minimum_age_ms + 1,
        );
        assert_eq!(status, HumanStatus::FullHuman);
    }

    #[test]
    fn stale_evidence_decays_and_can_demote_full_human_back_down() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::PhysicalPresence,
            strength: 100,
            recorded_at_ms: 0,
        });
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 100,
            recorded_at_ms: 0,
        });
        let policy = PromotionPolicy::whitepaper_default();
        let d = decay();

        // Promoted once aged and diverse enough.
        let promoted_at = policy.full_minimum_age_ms + 1;
        assert_eq!(
            record.status(&policy, &TrustWeights::founder_default(), &d, promoted_at),
            HumanStatus::FullHuman
        );

        // Long after both signals have fully decayed (no re-vouching, no
        // further presence), status must fall back down -- confidence
        // must be continuously re-earned, not banked forever.
        let long_after = d.zero_after_ms * 2;
        assert_eq!(
            record.status(&policy, &TrustWeights::founder_default(), &d, long_after),
            HumanStatus::Unverified
        );
    }

    #[test]
    fn our_own_sources_outweigh_external_by_default() {
        let weights = TrustWeights::founder_default();
        assert!(
            weights.weight_for(SignalSource::PhysicalPresence)
                > weights.weight_for(SignalSource::External(1))
        );
        assert!(
            weights.weight_for(SignalSource::VouchingGraph)
                > weights.weight_for(SignalSource::External(1))
        );
    }

    #[test]
    fn an_external_source_can_still_contribute_and_be_reweighted() {
        let mut weights = TrustWeights::founder_default();
        assert_eq!(weights.weight_for(SignalSource::External(42)), 20);
        weights.set_weight(SignalSource::External(42), 90);
        assert_eq!(weights.weight_for(SignalSource::External(42)), 90);
    }

    #[test]
    fn later_evidence_from_the_same_source_replaces_the_earlier_one() {
        let mut record = HumanRecord::new();
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 10,
            recorded_at_ms: 0,
        });
        record.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength: 90,
            recorded_at_ms: 1_000,
        });
        let latest = record.latest_per_source();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].1.strength, 90);
        assert_eq!(latest[0].1.recorded_at_ms, 1_000);
    }
}

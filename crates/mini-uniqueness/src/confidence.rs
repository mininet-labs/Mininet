//! Fusing the three whitepaper signals (SS5) into one confidence score that
//! decays over time and must be continuously re-earned.
//!
//! - **Signal (a)** — the vouching graph's trust propagation ([`crate::graph`]).
//! - **Signal (b)** — on-device behavioral/location entropy, proved in
//!   zero-knowledge. **Not implemented here** — see [`BehavioralEntropySource`]'s
//!   honest limit and D-0035 point 5: the whitepaper explicitly requires
//!   human authorship and external audit for this component, not AI-authored
//!   code, because it is genuinely unsolved cryptographic research ("has not
//!   yet been shipped anywhere").
//! - **Signal (c)** — physical-presence attestation, already implemented by
//!   `mini-presence` and named the whitepaper's *strongest* signal.
//!
//! ## Honest limits
//!
//! The fusion weights and decay curve below are a **first-cut, tunable**
//! implementation, not a value the whitepaper specifies — it says signals
//! "fuse" and confidence "decays over time" without naming a formula. This
//! module makes that concrete and testable so it can be calibrated against
//! real network data later, not to claim these particular numbers are final.

use crate::graph::TRUST_SCALE;

/// Linear decay: full credit up to `full_credit_window_ms`, zero at or past
/// `zero_after_ms`, linear in between. All-integer so a given age always
/// produces exactly the same weight on every device.
#[derive(Debug, Clone, Copy)]
pub struct DecayPolicy {
    /// A signal younger than this counts at full (100%) weight.
    pub full_credit_window_ms: u64,
    /// A signal this old or older counts as zero — it must be re-earned.
    pub zero_after_ms: u64,
}

impl DecayPolicy {
    /// A conservative default: full credit for a month, fully decayed by a
    /// year, matching the whitepaper's "confidence... must be continuously
    /// re-earned... across months rather than at a single moment" (SS5).
    pub fn months_scale_default() -> Self {
        DecayPolicy {
            full_credit_window_ms: 30 * 86_400_000,
            zero_after_ms: 365 * 86_400_000,
        }
    }

    /// The percentage (0..=100) of full credit a signal of this age
    /// currently carries.
    pub fn weight_percent(&self, age_ms: u64) -> u32 {
        if age_ms <= self.full_credit_window_ms {
            return 100;
        }
        if age_ms >= self.zero_after_ms {
            return 0;
        }
        let span = self.zero_after_ms - self.full_credit_window_ms;
        let elapsed = age_ms - self.full_credit_window_ms;
        100 - ((elapsed * 100) / span) as u32
    }
}

/// Raw signal values before fusion, each already 0..=100 except the raw
/// vouch trust mass (which is [`TRUST_SCALE`]-scaled).
#[derive(Debug, Clone, Copy)]
pub struct ConfidenceInputs {
    /// This identity root's trust mass from [`crate::graph::trust_scores`].
    pub vouch_trust: u64,
    /// Age of the most recent contributing vouch, in ms.
    pub vouch_age_ms: u64,
    /// A 0..=100 presence-strength score (caller's choice how to derive this
    /// from recent `PresenceVerdict`s — e.g. distinct-counterparty count).
    pub presence_score: u32,
    /// Age of the most recent contributing presence verdict, in ms.
    pub presence_age_ms: u64,
    /// Signal (b), if a platform shell has a real implementation. `None` is
    /// the correct, permanent value for every device today — see
    /// [`BehavioralEntropySource`].
    pub behavioral_score: Option<u32>,
}

/// Relative weight of each signal in the fused score. Whitepaper SS5 names
/// physical presence the strongest signal, hence the higher default weight
/// — but these are tunable, not frozen.
#[derive(Debug, Clone, Copy)]
pub struct ConfidenceWeights {
    /// Weight of the social-vouching graph signal.
    pub vouch: u32,
    /// Weight of the physical-presence signal.
    pub presence: u32,
    /// Weight of the behavioral-entropy signal, when present.
    pub behavioral: u32,
}

impl ConfidenceWeights {
    /// Presence weighted double the other two, reflecting the whitepaper's
    /// "strongest signal" framing for physical-presence attestation.
    pub fn whitepaper_default() -> Self {
        ConfidenceWeights {
            vouch: 1,
            presence: 2,
            behavioral: 1,
        }
    }
}

/// Fuse the three signals into one 0..=100 confidence score, applying decay
/// to each signal by its own evidence age first.
pub fn fuse_confidence(
    inputs: &ConfidenceInputs,
    decay: &DecayPolicy,
    weights: &ConfidenceWeights,
) -> u32 {
    let vouch_pct = ((inputs.vouch_trust.min(TRUST_SCALE) * 100) / TRUST_SCALE) as u32;
    let vouch_pct = vouch_pct * decay.weight_percent(inputs.vouch_age_ms) / 100;

    let presence_pct =
        inputs.presence_score.min(100) * decay.weight_percent(inputs.presence_age_ms) / 100;

    let mut weighted_sum = vouch_pct * weights.vouch + presence_pct * weights.presence;
    let mut total_weight = weights.vouch + weights.presence;

    if let Some(behavioral) = inputs.behavioral_score {
        weighted_sum += behavioral.min(100) * weights.behavioral;
        total_weight += weights.behavioral;
    }

    if total_weight == 0 {
        return 0;
    }
    weighted_sum / total_weight
}

/// The seam a platform shell fills in with a real zero-knowledge proof of
/// genuine human movement (whitepaper SS5, signal (b)).
///
/// ## Honest limit — do not implement this without a human cryptographer
///
/// This is explicitly **not** a crate to write ZK circuits in casually.
/// D-0035 point 5 records the whitepaper's own requirement: this component
/// must be "written by humans, reviewed by humans, and audited externally,
/// never delegated to automated tooling." [`NoEntropySource`] is the only
/// implementation in this repo, and it is the permanent, correct choice
/// until that human-led work exists — not a placeholder to casually fill in.
pub trait BehavioralEntropySource {
    /// A 0..=100 confidence contribution from on-device behavioral/location
    /// entropy, or `None` if no real proof is available. Raw sensor data
    /// must never leave the device — only this scalar verdict may.
    fn score(&mut self) -> Option<u32>;
}

/// The reference [`BehavioralEntropySource`]: no real proof exists, so
/// signal (b) simply does not contribute. Every device is a
/// `NoEntropySource` device until the human-authored, externally-audited
/// implementation described above lands.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoEntropySource;

impl BehavioralEntropySource for NoEntropySource {
    fn score(&mut self) -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decay() -> DecayPolicy {
        DecayPolicy {
            full_credit_window_ms: 1_000,
            zero_after_ms: 2_000,
        }
    }

    #[test]
    fn no_entropy_source_always_returns_none() {
        let mut source = NoEntropySource;
        assert_eq!(source.score(), None);
    }

    #[test]
    fn decay_weight_is_full_within_the_window_and_zero_past_it() {
        let d = decay();
        assert_eq!(d.weight_percent(0), 100);
        assert_eq!(d.weight_percent(1_000), 100);
        assert_eq!(d.weight_percent(2_000), 0);
        assert_eq!(d.weight_percent(10_000), 0);
    }

    #[test]
    fn decay_weight_is_linear_between_the_bounds() {
        let d = decay();
        // Halfway between 1_000 and 2_000.
        assert_eq!(d.weight_percent(1_500), 50);
    }

    #[test]
    fn fresh_full_signals_fuse_to_full_confidence() {
        let inputs = ConfidenceInputs {
            vouch_trust: TRUST_SCALE,
            vouch_age_ms: 0,
            presence_score: 100,
            presence_age_ms: 0,
            behavioral_score: None,
        };
        let score = fuse_confidence(&inputs, &decay(), &ConfidenceWeights::whitepaper_default());
        assert_eq!(score, 100);
    }

    #[test]
    fn stale_signals_decay_toward_zero() {
        let inputs = ConfidenceInputs {
            vouch_trust: TRUST_SCALE,
            vouch_age_ms: 10_000,
            presence_score: 100,
            presence_age_ms: 10_000,
            behavioral_score: None,
        };
        let score = fuse_confidence(&inputs, &decay(), &ConfidenceWeights::whitepaper_default());
        assert_eq!(score, 0);
    }

    #[test]
    fn behavioral_signal_when_present_pulls_the_score_toward_it() {
        let base = ConfidenceInputs {
            vouch_trust: 0,
            vouch_age_ms: 0,
            presence_score: 0,
            presence_age_ms: 0,
            behavioral_score: None,
        };
        let without = fuse_confidence(&base, &decay(), &ConfidenceWeights::whitepaper_default());
        let with_full = ConfidenceInputs {
            behavioral_score: Some(100),
            ..base
        };
        let with = fuse_confidence(
            &with_full,
            &decay(),
            &ConfidenceWeights::whitepaper_default(),
        );
        assert_eq!(without, 0);
        assert!(with > without);
    }

    #[test]
    fn zero_weights_and_zero_inputs_do_not_panic() {
        let inputs = ConfidenceInputs {
            vouch_trust: 0,
            vouch_age_ms: 0,
            presence_score: 0,
            presence_age_ms: 0,
            behavioral_score: None,
        };
        let weights = ConfidenceWeights {
            vouch: 0,
            presence: 0,
            behavioral: 0,
        };
        assert_eq!(fuse_confidence(&inputs, &decay(), &weights), 0);
    }
}

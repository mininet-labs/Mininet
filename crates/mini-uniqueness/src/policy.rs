//! The published personhood policy and the audit gate (founder choice,
//! 2026-11-02: published-policy threshold, physical-presence signals combined
//! with reciprocal peer vouching; see
//! `docs/design/humanness-score-and-peer-vouching-proposal.md`).
//!
//! ## What this is
//!
//! - [`PersonhoodPolicy`] — one versioned, content-addressed document stating
//!   exactly how a humanness score (0..=100) is computed and where the audit
//!   line sits. Anyone can recompute any score from the public policy; there
//!   is no secret or adaptive formula (Directive 14). The signal set is
//!   **closed** (INV-18-03): physical presence and the seed-anchored vouching
//!   graph only. Provider/bank/app-derived signals are not representable.
//! - [`AuditGate`] — maps a score to [`GateOutcome::Pass`] or
//!   [`GateOutcome::NeedsAudit`]. There is deliberately **no** excluded
//!   outcome: nobody is removed from the network; `NeedsAudit` only routes a
//!   vote or monthly payment to an appeal/audit path owned by the caller.
//! - [`bounded_threshold`] — validates a *proposed* change of the minimum
//!   against frozen floor/ceiling bounds and a per-change step limit. The
//!   one-human-one-vote and timelock live in governance, not here.
//!
//! ## Honest limits
//!
//! This counts identity roots; it does not prove one human (Sybil resistance
//! is unsolved, `docs/INVARIANTS.md`). A score is evidence strength, not a
//! personhood certificate, and nothing here may feed a balance or a vote
//! weight (P1): it only gates eligibility, equally, per identity root.

use crate::confidence::DecayPolicy;
use crate::status::{HumanRecord, SignalSource, TrustWeights};

/// Policy wire/format version, bumped on any change to the encoding.
pub const POLICY_FORMAT_VERSION: u8 = 1;

/// A published personhood policy. Construct with [`PersonhoodPolicy::new`],
/// which validates it; the digest identifies exactly these parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonhoodPolicy {
    /// Monotonic policy version chosen by governance.
    pub version: u32,
    /// Trust weight (0..=100) of physical-presence evidence.
    pub presence_weight: u32,
    /// Trust weight (0..=100) of seed-anchored vouching-graph evidence.
    pub vouching_weight: u32,
    /// Full-credit window before evidence starts decaying (ms).
    pub full_credit_window_ms: u64,
    /// Age at which evidence counts as zero (ms).
    pub zero_after_ms: u64,
    /// Score (0..=100) at or above which an action passes without audit.
    pub audit_minimum: u8,
    /// Maximum distinct vouches any one identity root may hold.
    pub max_vouch_degree: u32,
    /// Width of the audit window just below the minimum: only scores in
    /// `[audit_minimum - audit_band, audit_minimum)` are audited. The score is
    /// meant to be confident enough that clearly-low accounts need no peer
    /// time spent on them; they are simply not yet eligible for gated actions.
    pub audit_band: u8,
}

/// Why a policy or a proposed threshold change was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    /// A weight, threshold or window is out of range or inconsistent.
    Invalid,
    /// The proposed minimum is outside the frozen floor/ceiling.
    OutOfBounds,
    /// The proposed minimum moves further than one change may.
    StepTooLarge,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PolicyError::Invalid => "personhood policy is invalid",
            PolicyError::OutOfBounds => "proposed minimum is outside the allowed bounds",
            PolicyError::StepTooLarge => "proposed minimum moves too far in one change",
        })
    }
}
impl std::error::Error for PolicyError {}

impl PersonhoodPolicy {
    /// Validate and build a policy. Both signal weights must be at least one
    /// and at most 100 (a policy that zeroes the vouching graph would let a
    /// self-attesting farm saturate the score, issue #18), the decay window
    /// must be non-empty, the minimum at most 100, and the vouch degree cap
    /// at least one.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version: u32,
        presence_weight: u32,
        vouching_weight: u32,
        full_credit_window_ms: u64,
        zero_after_ms: u64,
        audit_minimum: u8,
        max_vouch_degree: u32,
        audit_band: u8,
    ) -> Result<Self, PolicyError> {
        let ok = (1..=100).contains(&presence_weight)
            && (1..=100).contains(&vouching_weight)
            && zero_after_ms > full_credit_window_ms
            && audit_minimum <= 100
            && max_vouch_degree >= 1
            && audit_band <= audit_minimum;
        if !ok {
            return Err(PolicyError::Invalid);
        }
        Ok(PersonhoodPolicy {
            version,
            presence_weight,
            vouching_weight,
            full_credit_window_ms,
            zero_after_ms,
            audit_minimum,
            max_vouch_degree,
            audit_band,
        })
    }

    /// Canonical bytes: fixed field order, fixed-width big-endian integers.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(48);
        b.extend_from_slice(b"mininet/personhood-policy/v");
        b.push(POLICY_FORMAT_VERSION);
        b.extend_from_slice(&self.version.to_be_bytes());
        b.extend_from_slice(&self.presence_weight.to_be_bytes());
        b.extend_from_slice(&self.vouching_weight.to_be_bytes());
        b.extend_from_slice(&self.full_credit_window_ms.to_be_bytes());
        b.extend_from_slice(&self.zero_after_ms.to_be_bytes());
        b.push(self.audit_minimum);
        b.extend_from_slice(&self.max_vouch_degree.to_be_bytes());
        b.push(self.audit_band);
        b
    }

    /// The policy's content address: everyone computing a score names the
    /// digest of the policy they used, so scores are checkable.
    pub fn digest(&self) -> [u8; 32] {
        mini_crypto::hash::blake3_256(&self.canonical_bytes())
    }

    /// A conservative starting point; the numbers are governance parameters,
    /// not frozen law.
    pub fn starting_point() -> Self {
        PersonhoodPolicy {
            version: 1,
            presence_weight: 100,
            vouching_weight: 70,
            full_credit_window_ms: 30 * 86_400_000,
            zero_after_ms: 365 * 86_400_000,
            audit_minimum: 30,
            max_vouch_degree: 150,
            audit_band: 10,
        }
    }

    /// The record's humanness score (0..=100) under this policy. Only the
    /// closed signal set counts: `External`/behavioral evidence weighs zero.
    pub fn score(&self, record: &HumanRecord, now_ms: u64) -> u8 {
        let weights = TrustWeights::closed_set(self.presence_weight, self.vouching_weight);
        let decay = DecayPolicy {
            full_credit_window_ms: self.full_credit_window_ms,
            zero_after_ms: self.zero_after_ms,
        };
        record.score(&weights, &decay, now_ms).min(100) as u8
    }

    /// The audit gate this policy defines.
    pub fn gate(&self) -> AuditGate {
        AuditGate {
            minimum: self.audit_minimum,
            band: self.audit_band,
        }
    }

    /// Whether the policy's closed set contains `source`.
    pub fn counts(source: SignalSource) -> bool {
        matches!(
            source,
            SignalSource::PhysicalPresence | SignalSource::VouchingGraph
        )
    }
}

/// Result of the audit gate. There is no exclusion variant by design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome {
    /// Score meets the published minimum.
    Pass,
    /// Score is just below the minimum (inside the audit band): peer audit.
    NeedsAudit,
    /// Score is below the audit band: not yet eligible for gated actions, and
    /// no peer time is spent on it. The identity stays on the network, keeps
    /// earning evidence, and is re-evaluated every epoch.
    Insufficient,
}

/// Gate for personhood-sensitive actions (voting, monthly payments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditGate {
    /// Minimum score (0..=100) to pass without audit.
    pub minimum: u8,
    /// Width of the audit window just below `minimum`.
    pub band: u8,
}

impl AuditGate {
    /// Evaluate a score against the minimum.
    pub fn evaluate(&self, score: u8) -> GateOutcome {
        if score >= self.minimum {
            GateOutcome::Pass
        } else if score >= self.minimum.saturating_sub(self.band) {
            GateOutcome::NeedsAudit
        } else {
            GateOutcome::Insufficient
        }
    }
}

/// Frozen limits on where the minimum may sit and how far one change may move
/// it, so the threshold cannot be ratcheted to exclude or to rubber-stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThresholdBounds {
    /// Lowest minimum ever allowed.
    pub floor: u8,
    /// Highest minimum ever allowed.
    pub ceiling: u8,
    /// Largest absolute move per accepted change.
    pub max_step: u8,
}

/// Validate a proposed new minimum against `bounds` from the `current` one.
pub fn bounded_threshold(
    current: u8,
    proposed: u8,
    bounds: ThresholdBounds,
) -> Result<u8, PolicyError> {
    if bounds.floor > bounds.ceiling || bounds.ceiling > 100 {
        return Err(PolicyError::Invalid);
    }
    if proposed < bounds.floor || proposed > bounds.ceiling {
        return Err(PolicyError::OutOfBounds);
    }
    if current.abs_diff(proposed) > bounds.max_step {
        return Err(PolicyError::StepTooLarge);
    }
    Ok(proposed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::SignalEvidence;

    fn ev(source: SignalSource, strength: u32, at: u64) -> SignalEvidence {
        SignalEvidence {
            source,
            strength,
            recorded_at_ms: at,
        }
    }

    #[test]
    fn policy_digest_is_stable_and_changes_with_any_parameter() {
        let p = PersonhoodPolicy::starting_point();
        assert_eq!(p.digest(), PersonhoodPolicy::starting_point().digest());
        let mut q = p.clone();
        q.audit_minimum += 1;
        assert_ne!(p.digest(), q.digest());
    }

    #[test]
    fn invalid_policies_are_refused() {
        let ok = PersonhoodPolicy::starting_point();
        assert!(PersonhoodPolicy::new(1, 0, 70, 1, 2, 30, 10, 5).is_err());
        assert!(PersonhoodPolicy::new(1, 100, 0, 1, 2, 30, 10, 5).is_err());
        assert!(PersonhoodPolicy::new(1, 100, 70, 5, 5, 30, 10, 5).is_err());
        assert!(PersonhoodPolicy::new(1, 100, 70, 1, 2, 101, 10, 5).is_err());
        assert!(PersonhoodPolicy::new(1, 100, 70, 1, 2, 30, 0, 5).is_err());
        assert!(PersonhoodPolicy::new(1, 100, 70, 1, 2, 30, 10, 31).is_err());
        assert!(PersonhoodPolicy::new(
            ok.version,
            ok.presence_weight,
            ok.vouching_weight,
            ok.full_credit_window_ms,
            ok.zero_after_ms,
            ok.audit_minimum,
            ok.max_vouch_degree,
            ok.audit_band
        )
        .is_ok());
    }

    #[test]
    fn external_and_behavioral_evidence_cannot_raise_the_score() {
        let p = PersonhoodPolicy::starting_point();
        let mut base = HumanRecord::new();
        base.record(ev(SignalSource::VouchingGraph, 40, 1_000));
        let mut padded = base.clone();
        padded.record(ev(SignalSource::External(7), 100, 1_000));
        padded.record(ev(SignalSource::BehavioralEntropy, 100, 1_000));
        assert_eq!(p.score(&base, 2_000), p.score(&padded, 2_000));
        assert!(!PersonhoodPolicy::counts(SignalSource::External(7)));
    }

    #[test]
    fn combined_presence_and_vouching_beats_either_alone_at_equal_strength() {
        let p = PersonhoodPolicy::starting_point();
        let mut only_v = HumanRecord::new();
        only_v.record(ev(SignalSource::VouchingGraph, 50, 0));
        let mut both = only_v.clone();
        both.record(ev(SignalSource::PhysicalPresence, 90, 0));
        assert!(p.score(&both, 10) > p.score(&only_v, 10));
    }

    #[test]
    fn only_scores_just_below_the_minimum_are_audited() {
        let g = AuditGate {
            minimum: 30,
            band: 10,
        };
        assert_eq!(g.evaluate(100), GateOutcome::Pass);
        assert_eq!(g.evaluate(30), GateOutcome::Pass);
        assert_eq!(g.evaluate(29), GateOutcome::NeedsAudit);
        assert_eq!(g.evaluate(20), GateOutcome::NeedsAudit);
        assert_eq!(g.evaluate(19), GateOutcome::Insufficient);
        assert_eq!(g.evaluate(0), GateOutcome::Insufficient);
        // A band wider than the minimum cannot underflow.
        let wide = AuditGate {
            minimum: 5,
            band: 50,
        };
        assert_eq!(wide.evaluate(0), GateOutcome::NeedsAudit);
    }

    #[test]
    fn threshold_changes_respect_floor_ceiling_and_step() {
        let b = ThresholdBounds {
            floor: 10,
            ceiling: 60,
            max_step: 5,
        };
        assert_eq!(bounded_threshold(30, 35, b), Ok(35));
        assert_eq!(bounded_threshold(30, 36, b), Err(PolicyError::StepTooLarge));
        assert_eq!(bounded_threshold(12, 9, b), Err(PolicyError::OutOfBounds));
        assert_eq!(bounded_threshold(58, 61, b), Err(PolicyError::OutOfBounds));
        let bad = ThresholdBounds {
            floor: 50,
            ceiling: 10,
            max_step: 5,
        };
        assert_eq!(bounded_threshold(30, 30, bad), Err(PolicyError::Invalid));
    }
}

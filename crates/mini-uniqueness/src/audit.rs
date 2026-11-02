//! The audit path: peer review of anonymous activity summaries plus a panel
//! vote, stricter after the first [`MATURE_FROM_EPOCH`] epochs (founder
//! decision, 2026-11-02). Pure state machines: no I/O, no clocks, no keys.
//!
//! ## Shape
//!
//! 1. A personhood-gated action (voting, monthly payment) whose score is
//!    *just below* the published minimum (inside the policy's audit band)
//!    opens an [`AuditCase`]. Scores far below the band are simply not yet
//!    eligible and cost reviewers nothing; the score is meant to be confident
//!    enough that audits are only for the borderline.
//! 2. A panel of reviewers is drawn by [`select_panel`] — a public,
//!    reproducible sortition over qualified identity roots, excluding the
//!    subject and the subject's direct vouchers (conflict of interest).
//! 3. Reviewers see only an [`ActivitySummary`]: bucketed aggregates under an
//!    opaque case handle, never the subject's identity, signal sources or raw
//!    data (P5). Each casts one [`Verdict`].
//! 4. [`AuditCase::outcome`] clears the action for that epoch, denies it for
//!    that epoch, or expires. **No outcome removes anyone from the network**
//!    and a denial lapses with the epoch; the identity may try again.
//!
//! ## Honest limits
//!
//! Reviewer *authentication* (signed votes, transport) is the caller's job;
//! this module enforces panel membership, one vote each, thresholds and
//! expiry. Sortition is only as unpredictable as the caller's seed (use a
//! finalized value nobody could choose after seeing the candidates). Counts
//! identity roots, not proven humans (Sybil resistance is unsolved).

use crate::policy::{AuditGate, GateOutcome, PersonhoodPolicy, PolicyError};
use did_mini::Did;
use std::collections::{HashMap, HashSet};

/// A governance epoch counter.
pub type Epoch = u64;

/// Epochs after which the stricter schedule applies (founder: "stricter after
/// first 1000 epoch").
pub const MATURE_FROM_EPOCH: Epoch = 1000;

/// Actions the personhood gate protects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GatedAction {
    /// Casting a governance vote.
    Vote,
    /// Receiving a monthly payment.
    MonthlyPayment,
}

/// Panel parameters for one stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditParams {
    /// Reviewers drawn per case.
    pub panel_size: u32,
    /// Confirmations needed to clear; must be a strict majority.
    pub approve_at_least: u32,
    /// Epochs a case stays open before it expires.
    pub case_lifetime_epochs: u64,
}

impl AuditParams {
    fn valid(&self) -> bool {
        self.panel_size >= 1
            && self.approve_at_least <= self.panel_size
            && u64::from(self.approve_at_least) * 2 > u64::from(self.panel_size)
            && self.case_lifetime_epochs >= 1
    }
}

/// Everything that tightens over time, for one stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// The published scoring policy and audit minimum for this stage.
    pub policy: PersonhoodPolicy,
    /// Panel parameters.
    pub audit: AuditParams,
    /// New vouches one identity root may form per epoch.
    pub vouches_per_epoch: u32,
}

/// Early and mature stages with the epoch where the mature one begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySchedule {
    early: Stage,
    mature: Stage,
    mature_from_epoch: Epoch,
}

impl PolicySchedule {
    /// Build a schedule, refusing any mature stage that is *looser* than the
    /// early one: higher-or-equal audit minimum and approval ratio, and
    /// lower-or-equal vouch degree cap, per-epoch vouch quota.
    pub fn new(early: Stage, mature: Stage, mature_from_epoch: Epoch) -> Result<Self, PolicyError> {
        let (e, m) = (&early, &mature);
        let ratio_not_looser = u64::from(m.audit.approve_at_least) * u64::from(e.audit.panel_size)
            >= u64::from(e.audit.approve_at_least) * u64::from(m.audit.panel_size);
        let ok = e.audit.valid()
            && m.audit.valid()
            && mature_from_epoch >= 1
            && m.policy.audit_minimum >= e.policy.audit_minimum
            && m.policy.max_vouch_degree <= e.policy.max_vouch_degree
            && m.policy.audit_band <= e.policy.audit_band
            && m.vouches_per_epoch <= e.vouches_per_epoch
            && m.vouches_per_epoch >= 1
            && m.audit.panel_size >= e.audit.panel_size
            && ratio_not_looser;
        if !ok {
            return Err(PolicyError::Invalid);
        }
        Ok(PolicySchedule {
            early,
            mature,
            mature_from_epoch,
        })
    }

    /// The founder-chosen starting schedule: mature from epoch 1000.
    pub fn starting_point() -> Self {
        let early = Stage {
            policy: PersonhoodPolicy::starting_point(),
            audit: AuditParams {
                panel_size: 7,
                approve_at_least: 4,
                case_lifetime_epochs: 3,
            },
            vouches_per_epoch: 10,
        };
        let mut mature_policy = PersonhoodPolicy::starting_point();
        mature_policy.version = 2;
        mature_policy.audit_minimum = 45;
        mature_policy.audit_band = 5;
        mature_policy.max_vouch_degree = 100;
        let mature = Stage {
            policy: mature_policy,
            audit: AuditParams {
                panel_size: 11,
                approve_at_least: 8,
                case_lifetime_epochs: 3,
            },
            vouches_per_epoch: 5,
        };
        PolicySchedule::new(early, mature, MATURE_FROM_EPOCH)
            .expect("starting schedule is consistent")
    }

    /// The stage in force at `epoch`.
    pub fn stage_at(&self, epoch: Epoch) -> &Stage {
        if epoch >= self.mature_from_epoch {
            &self.mature
        } else {
            &self.early
        }
    }
}

/// Why panel selection or a vote was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditError {
    /// Fewer eligible reviewers than the panel needs.
    NotEnoughReviewers,
    /// The voter is not on this case's panel.
    NotOnPanel,
    /// The reviewer already voted.
    AlreadyVoted,
    /// The case already has a final outcome or has expired.
    Closed,
    /// The score is not inside the audit band, so no case may be opened.
    OutsideAuditBand,
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AuditError::NotEnoughReviewers => "not enough eligible reviewers",
            AuditError::NotOnPanel => "reviewer is not on this panel",
            AuditError::AlreadyVoted => "reviewer already voted",
            AuditError::Closed => "audit case is closed",
            AuditError::OutsideAuditBand => "score is outside the audit band",
        })
    }
}
impl std::error::Error for AuditError {}

/// What a reviewer sees: bucketed aggregates only. No identity, no sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivitySummary {
    /// Humanness score (0..=100) rounded down to a multiple of 10.
    pub score_bucket: u8,
    /// Evidence age in whole epochs, capped at 255.
    pub age_epochs: u8,
    /// Distinct live signal kinds, capped at 255.
    pub live_signals: u8,
    /// Vouch count bucketed: 0, 1-4, 5-19, 20+ as 0..=3.
    pub vouch_bucket: u8,
}

impl ActivitySummary {
    /// Build a bucketed summary from exact values (only the buckets leave).
    pub fn bucketed(score: u8, age_epochs: u64, live_signals: usize, vouches: usize) -> Self {
        ActivitySummary {
            score_bucket: score.min(100) / 10 * 10,
            age_epochs: age_epochs.min(255) as u8,
            live_signals: live_signals.min(255) as u8,
            vouch_bucket: match vouches {
                0 => 0,
                1..=4 => 1,
                5..=19 => 2,
                _ => 3,
            },
        }
    }
}

/// A reviewer's judgement of the anonymous summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The activity looks like one real person's.
    Confirm,
    /// The activity looks farmed, scripted or duplicated.
    Doubt,
}

/// Public, reproducible sortition. Candidates are ranked by
/// `BLAKE3(seed || did)`; the subject and `excluded` roots (the subject's
/// vouchers) are skipped; the first `k` are the panel.
pub fn select_panel(
    candidates: &[Did],
    subject: &Did,
    excluded: &HashSet<Did>,
    seed: [u8; 32],
    k: u32,
) -> Result<Vec<Did>, AuditError> {
    let mut ranked: Vec<([u8; 32], &Did)> = candidates
        .iter()
        .filter(|c| *c != subject && !excluded.contains(*c))
        .collect::<HashSet<_>>()
        .into_iter()
        .map(|c| {
            let mut input = Vec::with_capacity(32 + c.as_str().len());
            input.extend_from_slice(&seed);
            input.extend_from_slice(c.as_str().as_bytes());
            (mini_crypto::hash::blake3_256(&input), c)
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.as_str().cmp(b.1.as_str())));
    if ranked.len() < k as usize {
        return Err(AuditError::NotEnoughReviewers);
    }
    Ok(ranked
        .into_iter()
        .take(k as usize)
        .map(|(_, c)| c.clone())
        .collect())
}

/// Final or pending state of a case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditOutcome {
    /// Enough confirmations: the action is allowed for the case's epoch.
    Cleared,
    /// Confirmation can no longer be reached: denied for the case's epoch.
    Denied,
    /// Neither yet; votes may still arrive.
    Pending,
    /// The case lapsed without a result: treated as not cleared, no penalty.
    Expired,
}

/// Everything needed to open an [`AuditCase`].
#[derive(Debug)]
pub struct CaseSpec<'a> {
    /// The identity root under audit.
    pub subject: &'a Did,
    /// The gated action.
    pub action: GatedAction,
    /// The epoch the case is opened in.
    pub epoch: Epoch,
    /// Panel parameters of the stage in force.
    pub params: AuditParams,
    /// Anonymous summary reviewers will see.
    pub summary: ActivitySummary,
    /// The drawn panel.
    pub panel: Vec<Did>,
    /// Coordinator-private salt for the opaque handle.
    pub salt: [u8; 32],
    /// The subject's exact score (must be inside the audit band).
    pub score: u8,
    /// The gate in force.
    pub gate: AuditGate,
}

/// One audit of one action for one identity root in one epoch.
#[derive(Debug, Clone)]
pub struct AuditCase {
    handle: [u8; 32],
    action: GatedAction,
    opened_epoch: Epoch,
    params: AuditParams,
    summary: ActivitySummary,
    panel: Vec<Did>,
    votes: HashMap<Did, Verdict>,
}

impl AuditCase {
    /// Open a case. Refused unless `score` is inside the audit band of `gate`
    /// (just below the minimum): clearly-low accounts are never audited. The opaque `handle` is `BLAKE3(domain||subject||epoch||
    /// action||salt)` so reviewers can refer to the case without learning the
    /// subject; keep `salt` secret to the coordinator.
    pub fn open(spec: CaseSpec<'_>) -> Result<Self, AuditError> {
        let CaseSpec {
            subject,
            action,
            epoch,
            params,
            summary,
            panel,
            salt,
            score,
            gate,
        } = spec;
        if gate.evaluate(score) != GateOutcome::NeedsAudit {
            return Err(AuditError::OutsideAuditBand);
        }
        if !params.valid() || panel.len() != params.panel_size as usize {
            return Err(AuditError::NotEnoughReviewers);
        }
        let mut input = b"mininet/audit-case/v1".to_vec();
        input.extend_from_slice(subject.as_str().as_bytes());
        input.extend_from_slice(&epoch.to_be_bytes());
        input.push(action as u8);
        input.extend_from_slice(&salt);
        Ok(AuditCase {
            handle: mini_crypto::hash::blake3_256(&input),
            action,
            opened_epoch: epoch,
            params,
            summary,
            panel,
            votes: HashMap::new(),
        })
    }

    /// The opaque case handle.
    pub fn handle(&self) -> [u8; 32] {
        self.handle
    }
    /// The action under audit.
    pub fn action(&self) -> GatedAction {
        self.action
    }
    /// The epoch the case was opened (and the only one its result covers).
    pub fn epoch(&self) -> Epoch {
        self.opened_epoch
    }
    /// The anonymous summary reviewers judge.
    pub fn summary(&self) -> ActivitySummary {
        self.summary
    }
    /// The drawn panel.
    pub fn panel(&self) -> &[Did] {
        &self.panel
    }

    /// Record a reviewer's verdict at `now_epoch`.
    pub fn cast(
        &mut self,
        reviewer: &Did,
        verdict: Verdict,
        now_epoch: Epoch,
    ) -> Result<(), AuditError> {
        if self.outcome(now_epoch) != AuditOutcome::Pending {
            return Err(AuditError::Closed);
        }
        if !self.panel.contains(reviewer) {
            return Err(AuditError::NotOnPanel);
        }
        if self.votes.contains_key(reviewer) {
            return Err(AuditError::AlreadyVoted);
        }
        self.votes.insert(reviewer.clone(), verdict);
        Ok(())
    }

    /// The case's outcome at `now_epoch`.
    pub fn outcome(&self, now_epoch: Epoch) -> AuditOutcome {
        let confirms = self
            .votes
            .values()
            .filter(|v| **v == Verdict::Confirm)
            .count() as u32;
        let doubts = self.votes.len() as u32 - confirms;
        if confirms >= self.params.approve_at_least {
            AuditOutcome::Cleared
        } else if doubts > self.params.panel_size - self.params.approve_at_least {
            AuditOutcome::Denied
        } else if now_epoch >= self.opened_epoch + self.params.case_lifetime_epochs {
            AuditOutcome::Expired
        } else {
            AuditOutcome::Pending
        }
    }
}

/// What the gate decides for one root, action and epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    /// Allowed (score passes or an audit cleared it this epoch).
    Allowed,
    /// Score is just below the minimum and no audit has cleared it: open a case.
    NeedsAudit,
    /// Score is below the audit band: not eligible this epoch, no audit.
    Insufficient,
    /// An audit denied it this epoch. Lapses next epoch; never an exclusion.
    DeniedThisEpoch,
}

/// Records settled audits so repeated checks in one epoch agree.
#[derive(Debug, Default)]
pub struct EligibilityLedger {
    settled: HashMap<(Did, GatedAction), (Epoch, bool)>,
}

impl EligibilityLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a case's final result for `subject`; pending/expired are ignored.
    pub fn record(&mut self, subject: &Did, case: &AuditCase, now_epoch: Epoch) {
        match case.outcome(now_epoch) {
            AuditOutcome::Cleared => {
                self.settled
                    .insert((subject.clone(), case.action()), (case.epoch(), true));
            }
            AuditOutcome::Denied => {
                self.settled
                    .insert((subject.clone(), case.action()), (case.epoch(), false));
            }
            AuditOutcome::Pending | AuditOutcome::Expired => {}
        }
    }

    /// Decide eligibility. A high enough score is always `Allowed` and never
    /// consults the ledger; a settled audit only matters in its own epoch.
    pub fn check(
        &self,
        subject: &Did,
        action: GatedAction,
        epoch: Epoch,
        score: u8,
        gate: AuditGate,
    ) -> Eligibility {
        match gate.evaluate(score) {
            GateOutcome::Pass => return Eligibility::Allowed,
            GateOutcome::Insufficient => return Eligibility::Insufficient,
            GateOutcome::NeedsAudit => {}
        }
        match self.settled.get(&(subject.clone(), action)) {
            Some((e, true)) if *e == epoch => Eligibility::Allowed,
            Some((e, false)) if *e == epoch => Eligibility::DeniedThisEpoch,
            _ => Eligibility::NeedsAudit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did(n: u8) -> Did {
        Controller::incept_single_from_seeds(&[n; 32], &[n.wrapping_add(100); 32])
            .unwrap()
            .did()
    }
    fn roots(n: u8) -> Vec<Did> {
        (1..=n).map(did).collect()
    }
    fn sched() -> PolicySchedule {
        PolicySchedule::starting_point()
    }
    fn gate() -> AuditGate {
        AuditGate {
            minimum: 30,
            band: 10,
        }
    }
    fn summary() -> ActivitySummary {
        ActivitySummary::bucketed(27, 40, 2, 3)
    }

    #[test]
    fn schedule_is_stricter_after_epoch_1000_and_refuses_loosening() {
        let s = sched();
        let (e, m) = (s.stage_at(999), s.stage_at(1000));
        assert!(m.policy.audit_minimum > e.policy.audit_minimum);
        assert!(m.policy.audit_band < e.policy.audit_band);
        assert!(m.audit.panel_size > e.audit.panel_size);
        assert!(m.vouches_per_epoch < e.vouches_per_epoch);
        assert!(m.policy.max_vouch_degree < e.policy.max_vouch_degree);
        let looser = Stage {
            policy: PersonhoodPolicy::starting_point(),
            audit: AuditParams {
                panel_size: 3,
                approve_at_least: 2,
                case_lifetime_epochs: 3,
            },
            vouches_per_epoch: 99,
        };
        assert!(PolicySchedule::new(e.clone(), looser, 1000).is_err());
    }

    #[test]
    fn audit_params_require_a_strict_majority() {
        let bad = AuditParams {
            panel_size: 4,
            approve_at_least: 2,
            case_lifetime_epochs: 1,
        };
        assert!(!bad.valid());
    }

    #[test]
    fn panel_is_reproducible_excludes_subject_and_vouchers_and_needs_enough() {
        let all = roots(30);
        let subject = all[0].clone();
        let excluded: HashSet<Did> = all[1..4].iter().cloned().collect();
        let a = select_panel(&all, &subject, &excluded, [7; 32], 7).unwrap();
        let b = select_panel(&all, &subject, &excluded, [7; 32], 7).unwrap();
        let c = select_panel(&all, &subject, &excluded, [8; 32], 7).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(!a.contains(&subject) && a.iter().all(|r| !excluded.contains(r)));
        assert_eq!(
            select_panel(&all[..6], &subject, &excluded, [7; 32], 7),
            Err(AuditError::NotEnoughReviewers)
        );
    }

    #[test]
    fn summary_exposes_only_buckets() {
        let s = ActivitySummary::bucketed(27, 400, 300, 7);
        assert_eq!(s.score_bucket, 20);
        assert_eq!(s.age_epochs, 255);
        assert_eq!(s.live_signals, 255);
        assert_eq!(s.vouch_bucket, 2);
    }

    fn open_case(epoch: Epoch) -> (AuditCase, Vec<Did>, Did) {
        let all = roots(30);
        let subject = all[0].clone();
        let params = sched().stage_at(epoch).audit;
        let panel =
            select_panel(&all, &subject, &HashSet::new(), [1; 32], params.panel_size).unwrap();
        let case = AuditCase::open(CaseSpec {
            subject: &subject,
            action: GatedAction::Vote,
            epoch,
            params,
            summary: summary(),
            panel: panel.clone(),
            salt: [5; 32],
            score: 27,
            gate: gate(),
        })
        .unwrap();
        (case, panel, subject)
    }

    #[test]
    fn majority_confirmation_clears_and_only_for_that_epoch() {
        let (mut case, panel, subject) = open_case(10);
        for r in panel.iter().take(4) {
            case.cast(r, Verdict::Confirm, 10).unwrap();
        }
        assert_eq!(case.outcome(10), AuditOutcome::Cleared);
        let mut ledger = EligibilityLedger::new();
        ledger.record(&subject, &case, 10);
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 10, 27, gate()),
            Eligibility::Allowed
        );
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 11, 27, gate()),
            Eligibility::NeedsAudit
        );
        assert_eq!(
            ledger.check(&subject, GatedAction::MonthlyPayment, 10, 27, gate()),
            Eligibility::NeedsAudit
        );
    }

    #[test]
    fn denial_lapses_next_epoch_and_never_excludes() {
        let (mut case, panel, subject) = open_case(10);
        for r in panel.iter().take(4) {
            case.cast(r, Verdict::Doubt, 10).unwrap();
        }
        assert_eq!(case.outcome(10), AuditOutcome::Denied);
        let mut ledger = EligibilityLedger::new();
        ledger.record(&subject, &case, 10);
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 10, 27, gate()),
            Eligibility::DeniedThisEpoch
        );
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 11, 27, gate()),
            Eligibility::NeedsAudit
        );
        // A passing score is never blocked by an old denial.
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 10, 35, gate()),
            Eligibility::Allowed
        );
    }

    #[test]
    fn votes_are_one_per_panelist_and_panel_only_and_close_on_expiry() {
        let (mut case, panel, _) = open_case(10);
        let outsider = did(250);
        assert_eq!(
            case.cast(&outsider, Verdict::Confirm, 10),
            Err(AuditError::NotOnPanel)
        );
        case.cast(&panel[0], Verdict::Confirm, 10).unwrap();
        assert_eq!(
            case.cast(&panel[0], Verdict::Doubt, 10),
            Err(AuditError::AlreadyVoted)
        );
        assert_eq!(case.outcome(12), AuditOutcome::Pending);
        assert_eq!(case.outcome(13), AuditOutcome::Expired);
        assert_eq!(
            case.cast(&panel[1], Verdict::Confirm, 13),
            Err(AuditError::Closed)
        );
    }

    #[test]
    fn mature_stage_needs_more_confirmations() {
        let (mut case, panel, _) = open_case(1000);
        for r in panel.iter().take(7) {
            case.cast(r, Verdict::Confirm, 1000).unwrap();
        }
        assert_eq!(case.outcome(1000), AuditOutcome::Pending);
        case.cast(&panel[7], Verdict::Confirm, 1000).unwrap();
        assert_eq!(case.outcome(1000), AuditOutcome::Cleared);
    }

    #[test]
    fn clearly_low_and_clearly_high_scores_cannot_open_a_case() {
        let all = roots(30);
        let subject = all[0].clone();
        let params = sched().stage_at(10).audit;
        let panel =
            select_panel(&all, &subject, &HashSet::new(), [1; 32], params.panel_size).unwrap();
        for score in [0u8, 19, 30, 90] {
            let r = AuditCase::open(CaseSpec {
                subject: &subject,
                action: GatedAction::Vote,
                epoch: 10,
                params,
                summary: summary(),
                panel: panel.clone(),
                salt: [5; 32],
                score,
                gate: gate(),
            });
            assert_eq!(r.unwrap_err(), AuditError::OutsideAuditBand, "{score}");
        }
        let ledger = EligibilityLedger::new();
        assert_eq!(
            ledger.check(&subject, GatedAction::Vote, 10, 5, gate()),
            Eligibility::Insufficient
        );
    }

    #[test]
    fn case_handle_hides_the_subject_but_is_stable() {
        let (a, _, _) = open_case(10);
        let (b, _, _) = open_case(10);
        assert_eq!(a.handle(), b.handle());
        assert_ne!(open_case(11).0.handle(), a.handle());
    }
}

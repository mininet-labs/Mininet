//! Reference policy kernel for the proposed Founding Parliament.
//!
//! This module is intentionally not wired into canonical Forge authorization
//! yet. `tests/parliament_policy.rs` compiles and attacks these rules so the
//! proposal has executable invariants without silently activating a new
//! governance authority before an exact-state decision.

/// Initial active-seat capacity.
pub const FOUNDING_SEATS: u32 = 7;
/// Defensive allocation ceiling for this reference kernel. The legitimate
/// `2n + 1` sequence reaches this exact value (7 -> ... -> 65_535), which is
/// already far beyond plausible human parliamentary scale. Raising it later is
/// an explicit code/evidence change rather than an unbounded allocation path.
pub const MAX_ACTIVE_SEATS: u32 = 65_535;
/// H0 invitation allowance in a rolling 30-day window.
pub const H0_INVITATIONS_PER_WINDOW: u32 = 100;
/// Ordinary Steward invitation allowance in a rolling 60-day window.
pub const STEWARD_INVITATIONS_PER_WINDOW: u32 = 10;
/// Steward service required before invitation rights activate.
pub const STEWARD_INVITER_MIN_SERVICE_DAYS: u32 = 90;
/// Maximum duration of an immediate emergency action.
pub const MAX_EMERGENCY_ORDER_MS: u64 = 72 * 60 * 60 * 1_000;
/// Maximum H0 Guardian Stay duration.
pub const MAX_GUARDIAN_STAY_MS: u64 = 14 * 24 * 60 * 60 * 1_000;
/// 100 percent in basis points.
pub const FULL_PUBLIC_ELIGIBILITY_BPS: u16 = 10_000;

/// Fail-closed policy validation error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParliamentPolicyError {
    /// A seat id was outside the current chamber.
    UnknownSeat,
    /// A seat attempted to vote more than once.
    DuplicateSeatVote,
    /// A seat-capacity input exceeded the reference kernel's allocation bound.
    SeatCapacityOutOfRange,
    /// A threshold or quorum was not met.
    ThresholdNotMet,
    /// An invitation allowance or qualification rule was exceeded.
    InvitationNotAllowed,
    /// Time bounds were invalid or too long.
    InvalidDuration,
    /// A governance transition attempted to move authority backwards.
    AuthorityRegression,
    /// Expansion evidence was incomplete.
    ExpansionNotProven,
    /// Public-governance activation evidence was incomplete.
    PublicTransitionNotProven,
    /// The H0 stay was attempted against a protected transition/action.
    GuardianStayForbidden,
    /// The same exact target was already stayed once.
    GuardianStayAlreadyUsed,
    /// Duty evidence did not justify compensation.
    DutyNotProven,
}

/// Parliamentary motion classes with deliberately different thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionClass {
    Ordinary,
    Major,
    EmergencyFix,
    Constitutional,
    PublicTransition,
    GuardianStayOverride,
}

/// One seat's ballot. Abstention counts for ordinary quorum but never as YES.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
}

/// A vote is keyed by active seat, not balance, reward, contribution volume,
/// committee rank, invitation ancestry, or H0 status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatVote {
    pub seat: u32,
    pub choice: VoteChoice,
}

/// Deterministic tally after duplicate/out-of-range rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoteTally {
    pub yes: u32,
    pub no: u32,
    pub abstain: u32,
}

impl VoteTally {
    pub fn participation(self) -> u32 {
        self.yes + self.no + self.abstain
    }
}

fn ceil_two_thirds(n: u32) -> u32 {
    n.saturating_mul(2).saturating_add(2) / 3
}

fn ceil_three_fourths(n: u32) -> u32 {
    n.saturating_mul(3).saturating_add(3) / 4
}

fn majority(n: u32) -> u32 {
    n / 2 + 1
}

/// Reject duplicate seat ballots and derive a deterministic tally.
pub fn tally_votes(
    active_seats: u32,
    votes: &[SeatVote],
) -> Result<VoteTally, ParliamentPolicyError> {
    if active_seats == 0 {
        return Err(ParliamentPolicyError::ThresholdNotMet);
    }
    if active_seats > MAX_ACTIVE_SEATS {
        return Err(ParliamentPolicyError::SeatCapacityOutOfRange);
    }
    let mut seen = vec![false; active_seats as usize];
    let mut tally = VoteTally {
        yes: 0,
        no: 0,
        abstain: 0,
    };
    for vote in votes {
        if vote.seat >= active_seats {
            return Err(ParliamentPolicyError::UnknownSeat);
        }
        if seen[vote.seat as usize] {
            return Err(ParliamentPolicyError::DuplicateSeatVote);
        }
        seen[vote.seat as usize] = true;
        match vote.choice {
            VoteChoice::Yes => tally.yes += 1,
            VoteChoice::No => tally.no += 1,
            VoteChoice::Abstain => tally.abstain += 1,
        }
    }
    Ok(tally)
}

/// Decide one exact motion under the proposed Founding Parliament thresholds.
pub fn motion_passes(
    class: MotionClass,
    active_seats: u32,
    votes: &[SeatVote],
) -> Result<bool, ParliamentPolicyError> {
    let tally = tally_votes(active_seats, votes)?;
    let passes = match class {
        MotionClass::Ordinary => {
            tally.participation() >= ceil_two_thirds(active_seats) && tally.yes > tally.no
        }
        MotionClass::Major => {
            tally.participation() >= ceil_two_thirds(active_seats)
                && tally.yes >= majority(active_seats)
        }
        MotionClass::EmergencyFix => tally.yes >= majority(active_seats),
        MotionClass::Constitutional | MotionClass::PublicTransition => {
            tally.yes >= ceil_two_thirds(active_seats)
        }
        MotionClass::GuardianStayOverride => tally.yes >= ceil_three_fourths(active_seats),
    };
    Ok(passes)
}

/// Founding invitation source. Invitation is candidacy access only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviterClass {
    H0,
    Steward {
        active_service_days: u32,
        completed_duty_periods: u32,
    },
}

/// Rolling-window allowance returned after qualification checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvitationAllowance {
    pub max_invitations: u32,
    pub window_days: u32,
}

pub fn invitation_allowance(
    inviter: InviterClass,
) -> Result<InvitationAllowance, ParliamentPolicyError> {
    match inviter {
        InviterClass::H0 => Ok(InvitationAllowance {
            max_invitations: H0_INVITATIONS_PER_WINDOW,
            window_days: 30,
        }),
        InviterClass::Steward {
            active_service_days,
            completed_duty_periods,
        } if active_service_days >= STEWARD_INVITER_MIN_SERVICE_DAYS
            && completed_duty_periods >= 1 =>
        {
            Ok(InvitationAllowance {
                max_invitations: STEWARD_INVITATIONS_PER_WINDOW,
                window_days: 60,
            })
        }
        InviterClass::Steward { .. } => Err(ParliamentPolicyError::InvitationNotAllowed),
    }
}

/// Check whether one more invitation may be issued in the current window.
pub fn can_issue_invitation(
    inviter: InviterClass,
    already_issued_in_window: u32,
) -> Result<(), ParliamentPolicyError> {
    let allowance = invitation_allowance(inviter)?;
    if already_issued_in_window >= allowance.max_invitations {
        return Err(ParliamentPolicyError::InvitationNotAllowed);
    }
    Ok(())
}

/// Defensive actions that may take immediate effect. Destructive powers are
/// intentionally absent from the type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergencyAction {
    P0Warning,
    P1Warning,
    ReleaseQuarantine,
    CriticalWorkaround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergencyOrder {
    pub action: EmergencyAction,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub exact_target: [u8; 32],
}

pub fn validate_emergency_order(order: EmergencyOrder) -> Result<(), ParliamentPolicyError> {
    let duration = order
        .expires_at_ms
        .checked_sub(order.issued_at_ms)
        .ok_or(ParliamentPolicyError::InvalidDuration)?;
    if duration == 0 || duration > MAX_EMERGENCY_ORDER_MS {
        return Err(ParliamentPolicyError::InvalidDuration);
    }
    Ok(())
}

/// Evidence used to claim one duty-period allowance. No vote direction or
/// balance field exists, by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutyEvidence {
    pub committee_tasks_assigned: u32,
    pub committee_tasks_completed: u32,
    pub plenary_votes_eligible: u32,
    pub plenary_votes_participated: u32,
    pub emergency_calls_assigned: u32,
    pub emergency_calls_answered: u32,
    pub substantive_review_completed: bool,
}

fn at_least_70_percent(done: u32, assigned: u32) -> bool {
    // Saturating u32 multiplication was wrong here: near u32::MAX both sides
    // saturate to the same value regardless of the true ratio (done =
    // 429_496_730, assigned = u32::MAX is ~10% complete but passed). u64
    // comfortably holds u32::MAX * 10 with no overflow, so there is no
    // reason to saturate at all.
    assigned == 0 || u64::from(done) * 10 >= u64::from(assigned) * 7
}

/// Duty compensation requires work, not title possession or a particular vote.
pub fn duty_payment_eligible(e: DutyEvidence) -> Result<(), ParliamentPolicyError> {
    // `at_least_70_percent` treats an `assigned == 0` category as vacuously
    // satisfied, so a period with nothing assigned anywhere would otherwise
    // pass on `substantive_review_completed` alone -- a passive salary for
    // title possession, the exact pattern this kernel exists to refuse.
    let any_duty_assigned = e.committee_tasks_assigned > 0
        || e.plenary_votes_eligible > 0
        || e.emergency_calls_assigned > 0;
    let committee_ok = e.committee_tasks_completed <= e.committee_tasks_assigned
        && at_least_70_percent(e.committee_tasks_completed, e.committee_tasks_assigned);
    let plenary_ok = e.plenary_votes_participated <= e.plenary_votes_eligible
        && at_least_70_percent(e.plenary_votes_participated, e.plenary_votes_eligible);
    let emergency_ok = e.emergency_calls_answered == e.emergency_calls_assigned;
    if any_duty_assigned
        && committee_ok
        && plenary_ok
        && emergency_ok
        && e.substantive_review_completed
    {
        Ok(())
    } else {
        Err(ParliamentPolicyError::DutyNotProven)
    }
}

/// Proposed governance phases. Public is terminal with respect to insider-only
/// eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParliamentPhase {
    Founding,
    Expanding,
    Public,
}

/// Machine-readable authority state used by the transition validator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParliamentState {
    pub phase: ParliamentPhase,
    pub seat_capacity: u32,
    /// Share of candidate eligibility open through mature public personhood.
    /// It is monotonic and reaches 10_000 at Public.
    pub public_eligibility_bps: u16,
    pub h0_guardian_active: bool,
}

/// Evidence gates for chamber growth and final public transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionEvidence {
    pub qualified_candidates: u32,
    pub committees_staffed: bool,
    pub key_rotation_recovery_tested: bool,
    pub adversarial_governance_exercise_passed: bool,
    pub forge_operates_without_github: bool,
    pub invitation_capture_review_passed: bool,
    pub mature_personhood: bool,
    pub public_ballot_proven: bool,
    pub no_founder_recovery_dependency: bool,
}

pub fn next_seat_capacity(current: u32) -> Option<u32> {
    if current >= MAX_ACTIVE_SEATS {
        return None;
    }
    let next = current.checked_mul(2)?.checked_add(1)?;
    (next <= MAX_ACTIVE_SEATS).then_some(next)
}

/// Validate a proposed authority transition. This is intentionally stricter
/// than ordinary motion voting: a majority cannot vote evidence into existence.
pub fn validate_transition(
    current: ParliamentState,
    next: ParliamentState,
    evidence: TransitionEvidence,
) -> Result<(), ParliamentPolicyError> {
    if current.seat_capacity > MAX_ACTIVE_SEATS || next.seat_capacity > MAX_ACTIVE_SEATS {
        return Err(ParliamentPolicyError::SeatCapacityOutOfRange);
    }
    if current.seat_capacity == 0
        || next.seat_capacity < current.seat_capacity
        || next.public_eligibility_bps < current.public_eligibility_bps
        || next.public_eligibility_bps > FULL_PUBLIC_ELIGIBILITY_BPS
        // Full eligibility is what the Public-phase evidence gate below
        // exists to prove (mature personhood, a proven public ballot, no
        // Founder recovery dependency, H0 authority gone). Without this,
        // a transition that never sets `next.phase = Public` could still
        // set `public_eligibility_bps = FULL_PUBLIC_ELIGIBILITY_BPS`
        // while `next.phase` stays `Expanding`, skipping that gate
        // entirely and reaching full public access on nothing but the
        // Expanding-phase evidence.
        || (next.public_eligibility_bps == FULL_PUBLIC_ELIGIBILITY_BPS
            && next.phase != ParliamentPhase::Public)
        || next.phase < current.phase
        || (!current.h0_guardian_active && next.h0_guardian_active)
    {
        return Err(ParliamentPolicyError::AuthorityRegression);
    }

    if next.seat_capacity > current.seat_capacity {
        let expected = next_seat_capacity(current.seat_capacity)
            .ok_or(ParliamentPolicyError::ExpansionNotProven)?;
        let expansion_ok = next.seat_capacity == expected
            && evidence.qualified_candidates >= next.seat_capacity
            && evidence.committees_staffed
            && evidence.key_rotation_recovery_tested
            && evidence.adversarial_governance_exercise_passed
            && evidence.forge_operates_without_github
            && evidence.invitation_capture_review_passed;
        if !expansion_ok {
            return Err(ParliamentPolicyError::ExpansionNotProven);
        }
    }

    if next.phase == ParliamentPhase::Public {
        let public_ok = next.public_eligibility_bps == FULL_PUBLIC_ELIGIBILITY_BPS
            && !next.h0_guardian_active
            && evidence.mature_personhood
            && evidence.public_ballot_proven
            && evidence.no_founder_recovery_dependency
            && evidence.forge_operates_without_github
            && evidence.adversarial_governance_exercise_passed;
        if !public_ok {
            return Err(ParliamentPolicyError::PublicTransitionNotProven);
        }
    }

    if current.phase == ParliamentPhase::Public
        && (next.phase != ParliamentPhase::Public
            || next.public_eligibility_bps != FULL_PUBLIC_ELIGIBILITY_BPS
            || next.h0_guardian_active)
    {
        return Err(ParliamentPolicyError::AuthorityRegression);
    }

    Ok(())
}

/// The H0 stay can only target ordinary enduring parliamentary acts. Public
/// transition, override, H0 sunset/removal, and owner sovereignty are outside
/// its reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardianStayTarget {
    Motion(MotionClass),
    GuardianSunset,
    H0SeatRemoval,
    OwnerAdoptionChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardianReason {
    VoiceValueWall,
    PermanentAdminAuthority,
    ForcedUpdateOrKillSwitch,
    HiddenUnmasking,
    ConfiscationOrValueBackdoor,
    ForkExitOrOwnerSovereignty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardianStay {
    pub target: GuardianStayTarget,
    pub reason: GuardianReason,
    pub exact_target: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
}

pub fn validate_guardian_stay(
    h0_guardian_active: bool,
    stay: GuardianStay,
    previously_stayed_exact_targets: &[[u8; 32]],
) -> Result<(), ParliamentPolicyError> {
    if !h0_guardian_active {
        return Err(ParliamentPolicyError::GuardianStayForbidden);
    }
    match stay.target {
        GuardianStayTarget::Motion(MotionClass::PublicTransition)
        | GuardianStayTarget::Motion(MotionClass::GuardianStayOverride)
        | GuardianStayTarget::GuardianSunset
        | GuardianStayTarget::H0SeatRemoval
        | GuardianStayTarget::OwnerAdoptionChoice => {
            return Err(ParliamentPolicyError::GuardianStayForbidden)
        }
        GuardianStayTarget::Motion(_) => {}
    }
    if previously_stayed_exact_targets.contains(&stay.exact_target) {
        return Err(ParliamentPolicyError::GuardianStayAlreadyUsed);
    }
    let duration = stay
        .expires_at_ms
        .checked_sub(stay.issued_at_ms)
        .ok_or(ParliamentPolicyError::InvalidDuration)?;
    if duration == 0 || duration > MAX_GUARDIAN_STAY_MS {
        return Err(ParliamentPolicyError::InvalidDuration);
    }
    Ok(())
}

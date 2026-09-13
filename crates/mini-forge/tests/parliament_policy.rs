#![allow(dead_code)]

#[path = "../src/parliament_policy.rs"]
mod parliament_policy;

use parliament_policy::*;

fn yes_votes(n: u32) -> Vec<SeatVote> {
    (0..n)
        .map(|seat| SeatVote {
            seat,
            choice: VoteChoice::Yes,
        })
        .collect()
}

#[test]
fn founder_has_no_extra_ordinary_vote_weight() {
    let votes = vec![
        SeatVote {
            seat: 0,
            choice: VoteChoice::Yes,
        },
        SeatVote {
            seat: 1,
            choice: VoteChoice::No,
        },
        SeatVote {
            seat: 2,
            choice: VoteChoice::No,
        },
        SeatVote {
            seat: 3,
            choice: VoteChoice::No,
        },
        SeatVote {
            seat: 4,
            choice: VoteChoice::Yes,
        },
    ];
    assert!(!motion_passes(MotionClass::Ordinary, 7, &votes).unwrap());
}

#[test]
fn duplicate_seat_cannot_vote_twice() {
    let votes = vec![
        SeatVote {
            seat: 0,
            choice: VoteChoice::Yes,
        },
        SeatVote {
            seat: 0,
            choice: VoteChoice::Yes,
        },
    ];
    assert_eq!(
        motion_passes(MotionClass::Ordinary, 7, &votes),
        Err(ParliamentPolicyError::DuplicateSeatVote)
    );
}

#[test]
fn hostile_seat_capacity_is_rejected_before_allocation() {
    assert_eq!(
        tally_votes(u32::MAX, &[]),
        Err(ParliamentPolicyError::SeatCapacityOutOfRange)
    );
    assert_eq!(
        motion_passes(MotionClass::Ordinary, MAX_ACTIVE_SEATS + 1, &[]),
        Err(ParliamentPolicyError::SeatCapacityOutOfRange)
    );
    assert_eq!(next_seat_capacity(MAX_ACTIVE_SEATS), None);
}

#[test]
fn ordinary_majority_needs_two_thirds_participation() {
    let votes = yes_votes(4);
    assert!(!motion_passes(MotionClass::Ordinary, 7, &votes).unwrap());

    let votes = yes_votes(5);
    assert!(motion_passes(MotionClass::Ordinary, 7, &votes).unwrap());
}

#[test]
fn major_motion_needs_quorum_and_majority_of_all_seats() {
    let four_yes = yes_votes(4);
    assert!(!motion_passes(MotionClass::Major, 7, &four_yes).unwrap());

    let mut four_yes_one_abstain = four_yes;
    four_yes_one_abstain.push(SeatVote {
        seat: 4,
        choice: VoteChoice::Abstain,
    });
    assert!(motion_passes(MotionClass::Major, 7, &four_yes_one_abstain).unwrap());
}

#[test]
fn emergency_fix_needs_majority_of_all_seats() {
    let three = yes_votes(3);
    let four = yes_votes(4);
    assert!(!motion_passes(MotionClass::EmergencyFix, 7, &three).unwrap());
    assert!(motion_passes(MotionClass::EmergencyFix, 7, &four).unwrap());
}

#[test]
fn constitutional_and_public_transition_need_two_thirds_of_all_seats() {
    let four = yes_votes(4);
    let five = yes_votes(5);
    assert!(!motion_passes(MotionClass::Constitutional, 7, &four).unwrap());
    assert!(motion_passes(MotionClass::Constitutional, 7, &five).unwrap());
    assert!(!motion_passes(MotionClass::PublicTransition, 7, &four).unwrap());
    assert!(motion_passes(MotionClass::PublicTransition, 7, &five).unwrap());
}

#[test]
fn guardian_override_needs_three_quarters_of_all_seats() {
    let five = yes_votes(5);
    let six = yes_votes(6);
    assert!(!motion_passes(MotionClass::GuardianStayOverride, 7, &five).unwrap());
    assert!(motion_passes(MotionClass::GuardianStayOverride, 7, &six).unwrap());
}

#[test]
fn invitation_limits_are_candidate_access_not_votes() {
    assert_eq!(
        invitation_allowance(InviterClass::H0).unwrap(),
        InvitationAllowance {
            max_invitations: 100,
            window_days: 30
        }
    );
    assert_eq!(
        invitation_allowance(InviterClass::Steward {
            active_service_days: 90,
            completed_duty_periods: 1,
        })
        .unwrap(),
        InvitationAllowance {
            max_invitations: 10,
            window_days: 60
        }
    );
    assert_eq!(
        invitation_allowance(InviterClass::Steward {
            active_service_days: 89,
            completed_duty_periods: 5,
        }),
        Err(ParliamentPolicyError::InvitationNotAllowed)
    );
    assert_eq!(
        can_issue_invitation(InviterClass::H0, 100),
        Err(ParliamentPolicyError::InvitationNotAllowed)
    );
}

#[test]
fn immediate_effect_orders_are_short_lived() {
    let order = EmergencyOrder {
        action: EmergencyAction::ReleaseQuarantine,
        issued_at_ms: 1_000,
        expires_at_ms: 1_000 + MAX_EMERGENCY_ORDER_MS,
        exact_target: [7; 32],
    };
    assert_eq!(validate_emergency_order(order), Ok(()));

    let too_long = EmergencyOrder {
        expires_at_ms: order.expires_at_ms + 1,
        ..order
    };
    assert_eq!(
        validate_emergency_order(too_long),
        Err(ParliamentPolicyError::InvalidDuration)
    );
}

#[test]
fn duty_payment_requires_activity_not_vote_direction() {
    let active = DutyEvidence {
        committee_tasks_assigned: 10,
        committee_tasks_completed: 7,
        plenary_votes_eligible: 10,
        plenary_votes_participated: 7,
        emergency_calls_assigned: 2,
        emergency_calls_answered: 2,
        substantive_review_completed: true,
    };
    assert_eq!(duty_payment_eligible(active), Ok(()));

    let inactive = DutyEvidence {
        plenary_votes_participated: 6,
        ..active
    };
    assert_eq!(
        duty_payment_eligible(inactive),
        Err(ParliamentPolicyError::DutyNotProven)
    );
}

#[test]
fn duty_payment_refuses_a_period_with_nothing_assigned() {
    // Every activity ratio is vacuously satisfied when its denominator is
    // zero, so a duty period assigned literally nothing must still be
    // refused rather than pass on the unstructured review flag alone --
    // otherwise this is a passive salary for title possession.
    let nothing_assigned = DutyEvidence {
        committee_tasks_assigned: 0,
        committee_tasks_completed: 0,
        plenary_votes_eligible: 0,
        plenary_votes_participated: 0,
        emergency_calls_assigned: 0,
        emergency_calls_answered: 0,
        substantive_review_completed: true,
    };
    assert_eq!(
        duty_payment_eligible(nothing_assigned),
        Err(ParliamentPolicyError::DutyNotProven)
    );
}

#[test]
fn chamber_growth_is_exact_and_evidence_gated() {
    assert_eq!(next_seat_capacity(7), Some(15));
    assert_eq!(next_seat_capacity(15), Some(31));

    let current = ParliamentState {
        phase: ParliamentPhase::Founding,
        seat_capacity: 7,
        public_eligibility_bps: 0,
        h0_guardian_active: true,
    };
    let evidence = TransitionEvidence {
        qualified_candidates: 15,
        committees_staffed: true,
        key_rotation_recovery_tested: true,
        adversarial_governance_exercise_passed: true,
        forge_operates_without_github: true,
        invitation_capture_review_passed: true,
        mature_personhood: false,
        public_ballot_proven: false,
        no_founder_recovery_dependency: false,
    };
    let next = ParliamentState {
        phase: ParliamentPhase::Expanding,
        seat_capacity: 15,
        public_eligibility_bps: 1_000,
        h0_guardian_active: true,
    };
    assert_eq!(validate_transition(current, next, evidence), Ok(()));

    let skipped = ParliamentState {
        seat_capacity: 31,
        ..next
    };
    assert_eq!(
        validate_transition(current, skipped, evidence),
        Err(ParliamentPolicyError::ExpansionNotProven)
    );
}

#[test]
fn public_eligibility_never_moves_backwards() {
    let current = ParliamentState {
        phase: ParliamentPhase::Expanding,
        seat_capacity: 31,
        public_eligibility_bps: 4_000,
        h0_guardian_active: true,
    };
    let regressed = ParliamentState {
        public_eligibility_bps: 3_999,
        ..current
    };
    let evidence = TransitionEvidence {
        qualified_candidates: 0,
        committees_staffed: false,
        key_rotation_recovery_tested: false,
        adversarial_governance_exercise_passed: false,
        forge_operates_without_github: false,
        invitation_capture_review_passed: false,
        mature_personhood: false,
        public_ballot_proven: false,
        no_founder_recovery_dependency: false,
    };
    assert_eq!(
        validate_transition(current, regressed, evidence),
        Err(ParliamentPolicyError::AuthorityRegression)
    );
}

#[test]
fn public_transition_kills_h0_exception_one_way() {
    let current = ParliamentState {
        phase: ParliamentPhase::Expanding,
        seat_capacity: 31,
        public_eligibility_bps: 7_500,
        h0_guardian_active: true,
    };
    let public = ParliamentState {
        phase: ParliamentPhase::Public,
        seat_capacity: 31,
        public_eligibility_bps: FULL_PUBLIC_ELIGIBILITY_BPS,
        h0_guardian_active: false,
    };
    let evidence = TransitionEvidence {
        qualified_candidates: 31,
        committees_staffed: true,
        key_rotation_recovery_tested: true,
        adversarial_governance_exercise_passed: true,
        forge_operates_without_github: true,
        invitation_capture_review_passed: true,
        mature_personhood: true,
        public_ballot_proven: true,
        no_founder_recovery_dependency: true,
    };
    assert_eq!(validate_transition(current, public, evidence), Ok(()));

    let restored = ParliamentState {
        phase: ParliamentPhase::Expanding,
        public_eligibility_bps: 5_000,
        h0_guardian_active: true,
        ..public
    };
    assert_eq!(
        validate_transition(public, restored, evidence),
        Err(ParliamentPolicyError::AuthorityRegression)
    );
}

#[test]
fn h0_cannot_stay_public_transition_or_own_sunset() {
    let public_transition = GuardianStay {
        target: GuardianStayTarget::Motion(MotionClass::PublicTransition),
        reason: GuardianReason::VoiceValueWall,
        exact_target: [1; 32],
        issued_at_ms: 0,
        expires_at_ms: 1,
    };
    assert_eq!(
        validate_guardian_stay(true, public_transition, &[]),
        Err(ParliamentPolicyError::GuardianStayForbidden)
    );

    let sunset = GuardianStay {
        target: GuardianStayTarget::GuardianSunset,
        exact_target: [2; 32],
        ..public_transition
    };
    assert_eq!(
        validate_guardian_stay(true, sunset, &[]),
        Err(ParliamentPolicyError::GuardianStayForbidden)
    );
}

#[test]
fn h0_stay_is_bounded_and_one_shot_per_exact_target() {
    let stay = GuardianStay {
        target: GuardianStayTarget::Motion(MotionClass::Constitutional),
        reason: GuardianReason::PermanentAdminAuthority,
        exact_target: [9; 32],
        issued_at_ms: 5_000,
        expires_at_ms: 5_000 + MAX_GUARDIAN_STAY_MS,
    };
    assert_eq!(validate_guardian_stay(true, stay, &[]), Ok(()));
    assert_eq!(
        validate_guardian_stay(true, stay, &[[9; 32]]),
        Err(ParliamentPolicyError::GuardianStayAlreadyUsed)
    );

    let too_long = GuardianStay {
        expires_at_ms: stay.expires_at_ms + 1,
        ..stay
    };
    assert_eq!(
        validate_guardian_stay(true, too_long, &[]),
        Err(ParliamentPolicyError::InvalidDuration)
    );
}

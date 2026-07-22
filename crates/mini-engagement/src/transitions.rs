//! Pure state transitions over [`Engagement`].
//!
//! No transition here ever executes a payment, submits anything to
//! consensus, or contacts a counterparty -- these are in-memory state
//! moves only. Wiring a transition to real settlement submission
//! (`mini_settlement::reconcile`) is separate, later work.

use did_mini::Did;

use crate::error::{EngagementError, Result};
use crate::state::{Engagement, EngagementState, Party};

/// Move an `Offered` engagement to `Accepted`. Any other starting state is
/// rejected -- an engagement can only be accepted once.
pub fn accept(mut e: Engagement, by: Did, now_ms: u64) -> Result<Engagement> {
    match e.state {
        EngagementState::Offered => {
            e.state = EngagementState::Accepted { by, at_ms: now_ms };
            Ok(e)
        }
        _ => Err(EngagementError::InvalidTransition),
    }
}

/// Release part of the escrowed amount. Valid from `Accepted` or another
/// `Milestone` -- i.e. any live, already-accepted state. Rejects a release
/// that would push the running total past the escrowed claim's amount: an
/// engagement can never release more than it holds.
pub fn release_milestone(
    mut e: Engagement,
    index: u16,
    released_micromini: u64,
) -> Result<Engagement> {
    match e.state {
        EngagementState::Accepted { .. } | EngagementState::Milestone { .. } => {
            let new_total = e
                .released_micromini
                .checked_add(released_micromini)
                .ok_or(EngagementError::MilestoneExceedsEscrow)?;
            if new_total > e.escrow_claim.amount_micro {
                return Err(EngagementError::MilestoneExceedsEscrow);
            }
            e.released_micromini = new_total;
            e.state = EngagementState::Milestone {
                index,
                released_micromini,
            };
            Ok(e)
        }
        _ => Err(EngagementError::InvalidTransition),
    }
}

/// Mark the engagement fully performed. Valid from `Accepted` or
/// `Milestone`.
pub fn complete(mut e: Engagement, now_ms: u64) -> Result<Engagement> {
    match e.state {
        EngagementState::Accepted { .. } | EngagementState::Milestone { .. } => {
            e.state = EngagementState::Completed { at_ms: now_ms };
            Ok(e)
        }
        _ => Err(EngagementError::InvalidTransition),
    }
}

/// Raise a dispute. Valid from `Accepted` or `Milestone` -- an `Offered`
/// engagement nobody has accepted yet has nothing to dispute, and a
/// terminal engagement is already settled.
pub fn dispute(mut e: Engagement, raised_by: Party, arbiters: Vec<Did>) -> Result<Engagement> {
    match e.state {
        EngagementState::Accepted { .. } | EngagementState::Milestone { .. } => {
            e.state = EngagementState::Disputed {
                raised_by,
                arbiters,
            };
            Ok(e)
        }
        _ => Err(EngagementError::InvalidTransition),
    }
}

/// The timeout edge every non-terminal state has back to the payer (FD-18
/// Part II.2's first obligation: "a provider that disappears cannot
/// strand funds"). A total function, not a `Result`-returning one: a
/// no-op once the engagement has already reached a terminal state, or if
/// `now_ms` has not reached `deadline_ms` yet -- timing out a `Completed`
/// or already-`TimedOut` engagement, or one that simply isn't due yet,
/// never happens.
pub fn timeout(mut e: Engagement, now_ms: u64) -> Engagement {
    if e.state.is_terminal() || now_ms < e.deadline_ms {
        return e;
    }
    let payer = e.payer.clone();
    e.state = EngagementState::TimedOut { refunded_to: payer };
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_crypto::SigningKey;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};
    use mini_settlement::sign_claim;

    fn sample_object_id() -> mini_objects::ObjectId {
        let root = Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        let device =
            Controller::incept_device_single_from_seeds(&root.did(), &[3u8; 32], &[4u8; 32])
                .unwrap();
        let obj = ObjectBuilder::new(ObjectType::Custom("terms".to_string()))
            .payload(Payload::Public(vec![1, 2, 3]))
            .sign(&root.did(), &device)
            .unwrap();
        obj.id().clone()
    }

    fn sample_engagement(amount_micro: u64, deadline_ms: u64) -> Engagement {
        let payer = Controller::incept_single().unwrap().did();
        let performer = Controller::incept_single().unwrap().did();
        let payer_key = SigningKey::from_seed(&[9u8; 32]);
        let claim = sign_claim(
            &payer_key,
            b"performer-payee-bytes",
            amount_micro,
            1,
            u64::MAX,
            b"chain-state",
            0,
        )
        .unwrap();
        Engagement::offer(sample_object_id(), payer, performer, claim, deadline_ms)
    }

    #[test]
    fn accept_moves_offered_to_accepted() {
        let e = sample_engagement(1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by.clone(), 500).unwrap();
        assert_eq!(e.state, EngagementState::Accepted { by, at_ms: 500 });
    }

    #[test]
    fn accepting_an_already_accepted_engagement_is_rejected() {
        let e = sample_engagement(1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by.clone(), 500).unwrap();
        assert_eq!(accept(e, by, 600), Err(EngagementError::InvalidTransition));
    }

    #[test]
    fn release_milestone_updates_running_total_and_state() {
        let e = sample_engagement(1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 500).unwrap();
        let e = release_milestone(e, 0, 400).unwrap();
        assert_eq!(e.released_micromini, 400);
        assert_eq!(e.remaining_micromini(), 600);
        assert_eq!(
            e.state,
            EngagementState::Milestone {
                index: 0,
                released_micromini: 400
            }
        );

        let e = release_milestone(e, 1, 600).unwrap();
        assert_eq!(e.released_micromini, 1_000);
        assert_eq!(e.remaining_micromini(), 0);
    }

    #[test]
    fn release_milestone_rejects_exceeding_the_escrow() {
        let e = sample_engagement(1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 500).unwrap();
        let e = release_milestone(e, 0, 900).unwrap();
        assert_eq!(
            release_milestone(e, 1, 200),
            Err(EngagementError::MilestoneExceedsEscrow)
        );
    }

    #[test]
    fn release_milestone_from_offered_is_rejected() {
        let e = sample_engagement(1_000, 10_000);
        assert_eq!(
            release_milestone(e, 0, 1),
            Err(EngagementError::InvalidTransition)
        );
    }

    #[test]
    fn complete_is_valid_from_accepted_and_milestone_only() {
        let e = sample_engagement(1_000, 10_000);
        assert_eq!(
            complete(e.clone(), 100),
            Err(EngagementError::InvalidTransition)
        );

        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 500).unwrap();
        let e = complete(e, 700).unwrap();
        assert_eq!(e.state, EngagementState::Completed { at_ms: 700 });
    }

    #[test]
    fn completing_a_completed_engagement_is_rejected() {
        let e = sample_engagement(1_000, 10_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 500).unwrap();
        let e = complete(e, 700).unwrap();
        assert_eq!(complete(e, 800), Err(EngagementError::InvalidTransition));
    }

    #[test]
    fn dispute_is_valid_from_accepted_and_milestone_only() {
        let e = sample_engagement(1_000, 10_000);
        let arbiters = vec![Controller::incept_single().unwrap().did()];
        assert_eq!(
            dispute(e.clone(), Party::Payer, arbiters.clone()),
            Err(EngagementError::InvalidTransition)
        );

        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 500).unwrap();
        let e = dispute(e, Party::Performer, arbiters.clone()).unwrap();
        assert_eq!(
            e.state,
            EngagementState::Disputed {
                raised_by: Party::Performer,
                arbiters
            }
        );
    }

    #[test]
    fn timeout_refunds_the_payer_from_any_non_terminal_state_past_the_deadline() {
        let e = sample_engagement(1_000, 1_000);
        let payer = e.payer.clone();

        // Offered, past deadline.
        let e = timeout(e, 2_000);
        assert_eq!(
            e.state,
            EngagementState::TimedOut {
                refunded_to: payer.clone()
            }
        );
    }

    #[test]
    fn timeout_is_a_noop_before_the_deadline() {
        let e = sample_engagement(1_000, 1_000);
        let e = timeout(e, 500);
        assert_eq!(e.state, EngagementState::Offered);
    }

    #[test]
    fn timeout_never_overwrites_a_completed_engagement() {
        let e = sample_engagement(1_000, 1_000);
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 100).unwrap();
        let e = complete(e, 200).unwrap();
        let e = timeout(e, 5_000);
        assert_eq!(e.state, EngagementState::Completed { at_ms: 200 });
    }

    #[test]
    fn timeout_from_accepted_or_milestone_still_refunds_the_payer() {
        let e = sample_engagement(1_000, 1_000);
        let payer = e.payer.clone();
        let by = Controller::incept_single().unwrap().did();
        let e = accept(e, by, 100).unwrap();
        let e = release_milestone(e, 0, 200).unwrap();
        let e = timeout(e, 2_000);
        assert_eq!(e.state, EngagementState::TimedOut { refunded_to: payer });
    }
}

//! [`ReviewState`]: the review lifecycle of one intake object. Unlike
//! [`crate::AuthorityClass`], this is not a linear order — `Rejected`
//! and `Superseded` are terminal outcomes, not "more advanced" than
//! `Accepted`. [`ReviewState::allows_transition_to`] is the one place
//! that decides which transitions are legal, so
//! [`crate::IntakeEnvelope::advance_review_state`] never has to embed
//! that logic itself.

use crate::error::{IntakeError, Result};

/// Where an intake object sits in the human/governed review lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReviewState {
    /// The default for every newly constructed envelope.
    Unreviewed,
    /// Held back from further processing — e.g. a malformed or
    /// suspicious source, per the research report's intake-flow
    /// diagram.
    Quarantined,
    UnderReview,
    Accepted,
    Rejected,
    /// Replaced by a newer intake object; kept for history rather than
    /// deleted (append-only provenance, matching this repository's
    /// `DECISION_LOG.md`/`FAILURE_BOOK.md` convention of marking
    /// superseded rather than erasing).
    Superseded,
}

impl ReviewState {
    pub(crate) fn tag(self) -> u8 {
        match self {
            ReviewState::Unreviewed => 1,
            ReviewState::Quarantined => 2,
            ReviewState::UnderReview => 3,
            ReviewState::Accepted => 4,
            ReviewState::Rejected => 5,
            ReviewState::Superseded => 6,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(ReviewState::Unreviewed),
            2 => Ok(ReviewState::Quarantined),
            3 => Ok(ReviewState::UnderReview),
            4 => Ok(ReviewState::Accepted),
            5 => Ok(ReviewState::Rejected),
            6 => Ok(ReviewState::Superseded),
            _ => Err(IntakeError::BadReviewState),
        }
    }

    /// Whether moving from `self` to `next` is a legal transition.
    /// `Rejected` and `Superseded` are terminal — nothing leaves them
    /// (a corrected re-intake is a *new* envelope, not a resurrection of
    /// a rejected one).
    pub fn allows_transition_to(self, next: ReviewState) -> bool {
        use ReviewState::*;
        matches!(
            (self, next),
            (Unreviewed, Quarantined | UnderReview | Rejected)
                | (Quarantined, UnderReview | Rejected)
                | (UnderReview, Accepted | Rejected | Quarantined)
                | (Accepted, Superseded)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReviewState::*;

    #[test]
    fn the_ordinary_happy_path_is_allowed() {
        assert!(Unreviewed.allows_transition_to(UnderReview));
        assert!(UnderReview.allows_transition_to(Accepted));
        assert!(Accepted.allows_transition_to(Superseded));
    }

    #[test]
    fn terminal_states_allow_no_further_transition() {
        for terminal in [Rejected, Superseded] {
            for next in [
                Unreviewed,
                Quarantined,
                UnderReview,
                Accepted,
                Rejected,
                Superseded,
            ] {
                assert!(!terminal.allows_transition_to(next));
            }
        }
    }

    #[test]
    fn accepted_cannot_skip_backward_to_unreviewed() {
        assert!(!Accepted.allows_transition_to(Unreviewed));
    }

    #[test]
    fn every_state_round_trips_through_its_tag() {
        for state in [
            Unreviewed,
            Quarantined,
            UnderReview,
            Accepted,
            Rejected,
            Superseded,
        ] {
            assert_eq!(ReviewState::from_tag(state.tag()).unwrap(), state);
        }
    }

    #[test]
    fn an_unrecognized_tag_is_rejected() {
        assert_eq!(ReviewState::from_tag(200), Err(IntakeError::BadReviewState));
    }
}

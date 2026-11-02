//! Governance of the audit minimum: one identity root, one equal vote, a
//! timelock, a participation floor, and the frozen [`ThresholdBounds`].
//! A pure state machine: callers supply eligibility (a root that passes the
//! personhood gate) and authenticate voters; this module enforces the rules.
//!
//! Counts identity roots, not proven humans (Sybil resistance is unsolved).

use crate::audit::Epoch;
use crate::policy::{bounded_threshold, PolicyError, ThresholdBounds};
use did_mini::Did;
use std::collections::HashSet;

/// Why a governor action was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernorError {
    /// The proposal violates the bounds or step limit.
    Policy(PolicyError),
    /// A proposal is already open.
    AlreadyOpen,
    /// No proposal is open.
    NothingOpen,
    /// The voter is not eligible (did not pass the personhood gate).
    NotEligible,
    /// The voter already voted on this proposal.
    AlreadyVoted,
    /// The timelock has not elapsed yet.
    Timelocked,
}

impl std::fmt::Display for GovernorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GovernorError::Policy(e) => e.fmt(f),
            GovernorError::AlreadyOpen => f.write_str("a threshold proposal is already open"),
            GovernorError::NothingOpen => f.write_str("no threshold proposal is open"),
            GovernorError::NotEligible => f.write_str("voter is not eligible"),
            GovernorError::AlreadyVoted => f.write_str("voter already voted"),
            GovernorError::Timelocked => f.write_str("proposal is still timelocked"),
        }
    }
}
impl std::error::Error for GovernorError {}

/// Result of finalizing a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finalized {
    /// The minimum changed to this value.
    Applied(u8),
    /// Not enough participation or no majority: the minimum is unchanged.
    Rejected,
}

#[derive(Debug, Clone)]
struct Pending {
    proposed: u8,
    opened: Epoch,
    yes: HashSet<Did>,
    no: HashSet<Did>,
}

/// Governs the published audit minimum.
#[derive(Debug, Clone)]
pub struct ThresholdGovernor {
    current: u8,
    bounds: ThresholdBounds,
    timelock_epochs: u64,
    min_votes: u32,
    pending: Option<Pending>,
}

impl ThresholdGovernor {
    /// New governor. `current` must already satisfy the bounds.
    pub fn new(
        current: u8,
        bounds: ThresholdBounds,
        timelock_epochs: u64,
        min_votes: u32,
    ) -> Result<Self, PolicyError> {
        bounded_threshold(current, current, bounds)?;
        if timelock_epochs == 0 || min_votes == 0 {
            return Err(PolicyError::Invalid);
        }
        Ok(ThresholdGovernor {
            current,
            bounds,
            timelock_epochs,
            min_votes,
            pending: None,
        })
    }

    /// The minimum currently in force.
    pub fn current(&self) -> u8 {
        self.current
    }

    /// Open a proposal to move the minimum.
    pub fn propose(&mut self, proposed: u8, epoch: Epoch) -> Result<(), GovernorError> {
        if self.pending.is_some() {
            return Err(GovernorError::AlreadyOpen);
        }
        bounded_threshold(self.current, proposed, self.bounds).map_err(GovernorError::Policy)?;
        self.pending = Some(Pending {
            proposed,
            opened: epoch,
            yes: HashSet::new(),
            no: HashSet::new(),
        });
        Ok(())
    }

    /// One equal vote per identity root. `eligible` is the caller's verdict
    /// that this root passes the personhood gate.
    pub fn vote(&mut self, voter: &Did, yes: bool, eligible: bool) -> Result<(), GovernorError> {
        let p = self.pending.as_mut().ok_or(GovernorError::NothingOpen)?;
        if !eligible {
            return Err(GovernorError::NotEligible);
        }
        if p.yes.contains(voter) || p.no.contains(voter) {
            return Err(GovernorError::AlreadyVoted);
        }
        if yes { &mut p.yes } else { &mut p.no }.insert(voter.clone());
        Ok(())
    }

    /// Settle after the timelock: applied only with enough participation and a
    /// strict majority of votes cast.
    pub fn finalize(&mut self, epoch: Epoch) -> Result<Finalized, GovernorError> {
        let p = self.pending.as_ref().ok_or(GovernorError::NothingOpen)?;
        if epoch < p.opened + self.timelock_epochs {
            return Err(GovernorError::Timelocked);
        }
        let total = (p.yes.len() + p.no.len()) as u32;
        let applied = total >= self.min_votes && p.yes.len() > p.no.len();
        let proposed = p.proposed;
        self.pending = None;
        if applied {
            self.current = proposed;
            Ok(Finalized::Applied(proposed))
        } else {
            Ok(Finalized::Rejected)
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
    fn gov() -> ThresholdGovernor {
        ThresholdGovernor::new(
            30,
            ThresholdBounds {
                floor: 10,
                ceiling: 60,
                max_step: 5,
            },
            3,
            3,
        )
        .unwrap()
    }

    #[test]
    fn majority_with_quorum_applies_only_after_the_timelock() {
        let mut g = gov();
        g.propose(35, 10).unwrap();
        for n in 1..=3 {
            g.vote(&did(n), true, true).unwrap();
        }
        g.vote(&did(4), false, true).unwrap();
        assert_eq!(g.finalize(12), Err(GovernorError::Timelocked));
        assert_eq!(g.finalize(13), Ok(Finalized::Applied(35)));
        assert_eq!(g.current(), 35);
    }

    #[test]
    fn no_quorum_or_no_majority_leaves_the_minimum_unchanged() {
        let mut g = gov();
        g.propose(35, 0).unwrap();
        g.vote(&did(1), true, true).unwrap();
        assert_eq!(g.finalize(3), Ok(Finalized::Rejected));
        assert_eq!(g.current(), 30);
        g.propose(35, 4).unwrap();
        for n in 1..=2 {
            g.vote(&did(n), true, true).unwrap();
        }
        for n in 3..=5 {
            g.vote(&did(n), false, true).unwrap();
        }
        assert_eq!(g.finalize(7), Ok(Finalized::Rejected));
        assert_eq!(g.current(), 30);
    }

    #[test]
    fn votes_are_equal_one_per_root_and_gate_eligible_only() {
        let mut g = gov();
        g.propose(32, 0).unwrap();
        g.vote(&did(1), true, true).unwrap();
        assert_eq!(
            g.vote(&did(1), false, true),
            Err(GovernorError::AlreadyVoted)
        );
        assert_eq!(
            g.vote(&did(2), true, false),
            Err(GovernorError::NotEligible)
        );
    }

    #[test]
    fn proposals_respect_bounds_and_one_at_a_time() {
        let mut g = gov();
        assert_eq!(
            g.propose(40, 0),
            Err(GovernorError::Policy(PolicyError::StepTooLarge))
        );
        assert_eq!(
            g.propose(5, 0),
            Err(GovernorError::Policy(PolicyError::OutOfBounds))
        );
        g.propose(31, 0).unwrap();
        assert_eq!(g.propose(32, 0), Err(GovernorError::AlreadyOpen));
        assert_eq!(
            gov().vote(&did(1), true, true),
            Err(GovernorError::NothingOpen)
        );
    }
}

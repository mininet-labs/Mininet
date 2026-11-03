//! Opt-in graded payout: applies a published `PersonhoodPolicy`'s standing
//! rules to snapshot building. A Full root gets its whole entitlement in the
//! snapshot; a Provisional root gets the policy's share now and the rest is
//! reported as *held*; an Unvouched root gets nothing now and everything is
//! held. Held value is delayed, never forfeited: a later campaign releases it
//! with [`add_release`] once the root reaches Full standing.
//!
//! This only decides *how much goes into a snapshot*. It holds no keys, moves
//! no value, and does not change [`crate::SnapshotBuilder::insert`], which
//! stays available for campaigns that choose not to grade. It counts identity
//! roots, not verified humans: the Sybil limits in `mini-uniqueness` apply
//! unchanged.

use did_mini::Did;
use mini_uniqueness::{HumanRecord, PersonhoodPolicy, Standing};

use crate::error::Result;
use crate::snapshot::{AllocationEntry, SnapshotBuilder};

/// What grading one root's entitlement produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradedOutcome {
    pub standing: Standing,
    /// Micro-MINI placed in the snapshot now.
    pub pay_now: u64,
    /// Micro-MINI held for later release. `pay_now + held` always equals the
    /// entitlement.
    pub held: u64,
}

/// Grade `entitlement` for `identity_root` and insert the pay-now part (if
/// any) into `builder`. The caller must record `held` somewhere durable; this
/// crate does not store it.
#[allow(clippy::too_many_arguments)]
pub fn add_graded(
    builder: &mut SnapshotBuilder,
    policy: &PersonhoodPolicy,
    record: &HumanRecord,
    identity_root: Did,
    now_ms: u64,
    audit_cleared: bool,
    entitlement: u64,
    reason: &str,
) -> Result<GradedOutcome> {
    let standing = policy.standing(record, now_ms, audit_cleared);
    let (pay_now, held) = policy.payout_split(standing, entitlement);
    if pay_now > 0 {
        builder.insert(AllocationEntry {
            identity_root,
            amount_micro: pay_now,
            human_status: None,
            reason: reason.to_string(),
        })?;
    }
    Ok(GradedOutcome {
        standing,
        pay_now,
        held,
    })
}

/// What releasing a root's held value produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseOutcome {
    pub standing: Standing,
    pub released: u64,
    pub still_held: u64,
}

/// Release `held` for `identity_root` into `builder` if the root is now Full;
/// otherwise leave it held. Nothing is ever dropped.
#[allow(clippy::too_many_arguments)]
pub fn add_release(
    builder: &mut SnapshotBuilder,
    policy: &PersonhoodPolicy,
    record: &HumanRecord,
    identity_root: Did,
    now_ms: u64,
    audit_cleared: bool,
    held: u64,
    reason: &str,
) -> Result<ReleaseOutcome> {
    let standing = policy.standing(record, now_ms, audit_cleared);
    let released = policy.release_held(standing, held);
    if released > 0 {
        builder.insert(AllocationEntry {
            identity_root,
            amount_micro: released,
            human_status: None,
            reason: reason.to_string(),
        })?;
    }
    Ok(ReleaseOutcome {
        standing,
        released,
        still_held: held - released,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_uniqueness::{SignalEvidence, SignalSource};

    fn root() -> Did {
        Controller::incept_single().unwrap().did()
    }

    #[test]
    fn unvouched_root_gets_nothing_now_and_everything_held() {
        let policy = PersonhoodPolicy::starting_point();
        let record = HumanRecord::default();
        let mut b = SnapshotBuilder::new(b"c".to_vec()).unwrap();
        let out = add_graded(&mut b, &policy, &record, root(), 1, false, 1_000, "t").unwrap();
        assert_eq!(out.standing, Standing::Unvouched);
        assert_eq!(out.pay_now, 0);
        assert_eq!(out.held, 1_000);
        assert!(b.is_empty());
    }

    #[test]
    fn release_of_an_unvouched_root_stays_held() {
        let policy = PersonhoodPolicy::starting_point();
        let record = HumanRecord::default();
        let mut b = SnapshotBuilder::new(b"c".to_vec()).unwrap();
        let out = add_release(&mut b, &policy, &record, root(), 1, false, 700, "t").unwrap();
        assert_eq!(out.released, 0);
        assert_eq!(out.still_held, 700);
        assert!(b.is_empty());
    }

    fn vouched(strength: u32) -> HumanRecord {
        let mut r = HumanRecord::new();
        r.record(SignalEvidence {
            source: SignalSource::VouchingGraph,
            strength,
            recorded_at_ms: 1_000,
        });
        r.record(SignalEvidence {
            source: SignalSource::PhysicalPresence,
            strength: 80,
            recorded_at_ms: 1_000,
        });
        r
    }

    #[test]
    fn provisional_gets_a_share_now_and_the_rest_is_held_then_released_at_full() {
        let policy = PersonhoodPolicy::starting_point();
        let prov = vouched(u32::from(policy.provisional_vouch_strength));
        let mut b = SnapshotBuilder::new(b"c".to_vec()).unwrap();
        let who = root();
        let out = add_graded(
            &mut b,
            &policy,
            &prov,
            who.clone(),
            2_000,
            false,
            1_000,
            "t",
        )
        .unwrap();
        assert_eq!(out.standing, Standing::Provisional);
        assert_eq!(out.pay_now + out.held, 1_000);
        assert!(out.pay_now > 0 && out.held > 0);
        assert_eq!(b.len(), 1);

        let full = vouched(100);
        let mut later = SnapshotBuilder::new(b"c2".to_vec()).unwrap();
        let rel =
            add_release(&mut later, &policy, &full, who, 2_000, false, out.held, "r").unwrap();
        assert_eq!(rel.standing, Standing::Full);
        assert_eq!(rel.released, out.held);
        assert_eq!(rel.still_held, 0);
    }

    #[test]
    fn full_standing_is_paid_in_full() {
        let policy = PersonhoodPolicy::starting_point();
        let mut b = SnapshotBuilder::new(b"c".to_vec()).unwrap();
        let out = add_graded(
            &mut b,
            &policy,
            &vouched(100),
            root(),
            2_000,
            false,
            1_000,
            "t",
        )
        .unwrap();
        assert_eq!(out.standing, Standing::Full);
        assert_eq!((out.pay_now, out.held), (1_000, 0));
    }
}

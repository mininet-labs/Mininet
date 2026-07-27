//! Treasury balance reconciliation: proof the treasury actually holds
//! enough MINI to honor every allocation in a snapshot, before any claim
//! against it is treated as redeemable. Named as an explicit "not built"
//! gap in D-0356/`docs/STATUS.md` until this module closed it.
//!
//! This is bookkeeping over two integers, nothing more. It does not read
//! a real treasury balance from anywhere -- `treasury_balance_micro` is
//! whatever the caller already knows the treasury holds, from a source
//! this crate has no opinion on. It also does not gate
//! `mini_airdrop::verify_and_resolve_claim` or
//! [`crate::verify_payout_approvals`]: neither calls into this module.
//! A campaign operator (or whatever real custody mechanism eventually
//! exists) is expected to run this before treating a snapshot as
//! claimable, the same "protocol never judges, caller decides when to
//! act on advisory information" discipline `mini-airdrop`'s own
//! `AllocationEntry::human_status` already follows.

use mini_airdrop::AirdropSnapshot;

use crate::error::{ReconciliationError, ReconciliationResult};

/// Sum every entry's `amount_micro` in `snapshot`, using checked
/// addition so an adversarially constructed snapshot cannot silently
/// wrap around `u64` and understate its own total.
pub fn total_allocated_micro(snapshot: &AirdropSnapshot) -> ReconciliationResult<u64> {
    let mut total: u64 = 0;
    for entry in snapshot.entries() {
        total = total
            .checked_add(entry.amount_micro)
            .ok_or(ReconciliationError::TotalAllocationOverflow)?;
    }
    Ok(total)
}

/// Check that `treasury_balance_micro` covers every allocation in
/// `snapshot`.
pub fn check_snapshot_within_treasury_balance(
    snapshot: &AirdropSnapshot,
    treasury_balance_micro: u64,
) -> ReconciliationResult<()> {
    let required_micro = total_allocated_micro(snapshot)?;
    if required_micro > treasury_balance_micro {
        return Err(ReconciliationError::InsufficientTreasuryBalance {
            required_micro,
            available_micro: treasury_balance_micro,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;
    use mini_airdrop::{AllocationEntry, SnapshotBuilder};

    fn snapshot_with_amounts(amounts: &[u64]) -> AirdropSnapshot {
        let mut b = SnapshotBuilder::new(b"campaign-1".to_vec()).unwrap();
        for &amount_micro in amounts {
            let identity_root = Controller::incept_single().unwrap().did();
            b.insert(AllocationEntry {
                identity_root,
                amount_micro,
                human_status: None,
                reason: "test".to_string(),
            })
            .unwrap();
        }
        b.build()
    }

    #[test]
    fn an_empty_snapshot_requires_zero_balance() {
        let snapshot = snapshot_with_amounts(&[]);
        assert_eq!(total_allocated_micro(&snapshot).unwrap(), 0);
        assert!(check_snapshot_within_treasury_balance(&snapshot, 0).is_ok());
    }

    #[test]
    fn total_allocated_micro_sums_every_entry() {
        let snapshot = snapshot_with_amounts(&[1_000, 2_500, 500]);
        assert_eq!(total_allocated_micro(&snapshot).unwrap(), 4_000);
    }

    #[test]
    fn a_balance_exactly_matching_the_total_is_accepted() {
        let snapshot = snapshot_with_amounts(&[1_000, 2_000]);
        assert!(check_snapshot_within_treasury_balance(&snapshot, 3_000).is_ok());
    }

    #[test]
    fn a_balance_with_room_to_spare_is_accepted() {
        let snapshot = snapshot_with_amounts(&[1_000, 2_000]);
        assert!(check_snapshot_within_treasury_balance(&snapshot, 10_000).is_ok());
    }

    #[test]
    fn a_balance_short_by_one_is_rejected() {
        let snapshot = snapshot_with_amounts(&[1_000, 2_000]);
        assert_eq!(
            check_snapshot_within_treasury_balance(&snapshot, 2_999).unwrap_err(),
            ReconciliationError::InsufficientTreasuryBalance {
                required_micro: 3_000,
                available_micro: 2_999,
            }
        );
    }

    #[test]
    fn an_overflowing_total_is_reported_not_panicked() {
        let snapshot = snapshot_with_amounts(&[u64::MAX, 1]);
        assert_eq!(
            total_allocated_micro(&snapshot).unwrap_err(),
            ReconciliationError::TotalAllocationOverflow
        );
        assert_eq!(
            check_snapshot_within_treasury_balance(&snapshot, u64::MAX).unwrap_err(),
            ReconciliationError::TotalAllocationOverflow
        );
    }
}

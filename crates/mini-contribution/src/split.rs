//! Deterministic multi-party reward splitting.

use crate::error::{ContributionError, Result};
use crate::role::DeliveryRole;

/// A creator/seeder MINI split, in basis points (1/100 of a percent). Must
/// sum to exactly [`RewardSplit::TOTAL_BPS`] -- constructed only through
/// [`RewardSplit::new`], which enforces that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RewardSplit {
    creator_bps: u16,
    seeder_bps: u16,
}

impl RewardSplit {
    /// Basis points in a whole (100.00%).
    pub const TOTAL_BPS: u16 = 10_000;

    /// Construct a split, rejecting one that does not sum to
    /// [`Self::TOTAL_BPS`].
    pub fn new(creator_bps: u16, seeder_bps: u16) -> Result<Self> {
        if creator_bps.checked_add(seeder_bps) != Some(Self::TOTAL_BPS) {
            return Err(ContributionError::InvalidSplit);
        }
        Ok(Self {
            creator_bps,
            seeder_bps,
        })
    }

    pub fn creator_bps(&self) -> u16 {
        self.creator_bps
    }

    pub fn seeder_bps(&self) -> u16 {
        self.seeder_bps
    }
}

/// One payee's share of a split payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayeeShare {
    pub role: DeliveryRole,
    pub account: Vec<u8>,
    pub amount_micro: u64,
}

/// Deterministically divide `total_micro` between a creator and a seeder
/// account per `split`. Integer division; any division remainder is left
/// undistributed -- never paid to anyone, never silently lost -- the same
/// "leave the remainder unissued" discipline `mini_economy::plan_human_share`
/// already uses for a different split. A zero-bps (or zero-amount-after-
/// division) share is simply omitted from the result rather than producing
/// a zero-amount claim, since `mini-settlement` rejects those outright.
pub fn split_amount(
    total_micro: u64,
    split: RewardSplit,
    creator_account: Vec<u8>,
    seeder_account: Vec<u8>,
) -> Vec<PayeeShare> {
    let creator_amount = (u128::from(total_micro) * u128::from(split.creator_bps)
        / u128::from(RewardSplit::TOTAL_BPS)) as u64;
    let seeder_amount = (u128::from(total_micro) * u128::from(split.seeder_bps)
        / u128::from(RewardSplit::TOTAL_BPS)) as u64;

    let mut shares = Vec::with_capacity(2);
    if creator_amount > 0 {
        shares.push(PayeeShare {
            role: DeliveryRole::Creator,
            account: creator_account,
            amount_micro: creator_amount,
        });
    }
    if seeder_amount > 0 {
        shares.push(PayeeShare {
            role: DeliveryRole::Seeder,
            account: seeder_account,
            amount_micro: seeder_amount,
        });
    }
    shares
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_split_that_does_not_sum_to_10_000_bps_is_rejected() {
        assert_eq!(
            RewardSplit::new(5_000, 4_999),
            Err(ContributionError::InvalidSplit)
        );
        assert_eq!(
            RewardSplit::new(5_000, 5_001),
            Err(ContributionError::InvalidSplit)
        );
    }

    #[test]
    fn a_valid_split_divides_the_total_and_leaves_the_remainder_undistributed() {
        let split = RewardSplit::new(7_000, 3_000).unwrap();
        let shares = split_amount(1_001, split, vec![1], vec![2]);
        // 1_001 * 7_000 / 10_000 = 700; 1_001 * 3_000 / 10_000 = 300 -- both
        // floor, so 1 micro-MINI of the total is deliberately undistributed.
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].amount_micro, 700);
        assert_eq!(shares[1].amount_micro, 300);
        assert_eq!(shares.iter().map(|s| s.amount_micro).sum::<u64>(), 1_000);
    }

    #[test]
    fn a_zero_bps_share_is_omitted_rather_than_a_zero_amount_claim() {
        let split = RewardSplit::new(10_000, 0).unwrap();
        let shares = split_amount(500, split, vec![1], vec![2]);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].role, DeliveryRole::Creator);
        assert_eq!(shares[0].amount_micro, 500);
    }

    #[test]
    fn a_total_too_small_to_divide_at_all_produces_no_shares() {
        let split = RewardSplit::new(1, 9_999).unwrap();
        let shares = split_amount(1, split, vec![1], vec![2]);
        assert!(shares.is_empty());
    }
}

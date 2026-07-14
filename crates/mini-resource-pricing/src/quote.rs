//! Resource price vector and quote engine (D-0302; founder research phase
//! P6, `MN-601`): declares a MINI-denominated price for a
//! [`mini_privacy_policy::PrivacyTier`] and payload, from
//! [`mini_privacy_policy::expected_cost`]'s declared resource multipliers.
//!
//! **This is a quote, not a market-clearing price.** No payment executes
//! here — no e-cash, no blinded token, no ledger write. That is
//! deliberately later, separate work (`MN-602`/`MN-603`) with its own
//! external-review posture, matching this repo's D-0047 gate for anything
//! crypto-adjacent. `quote` is pure: same inputs always produce the same
//! output, no I/O, no side effect.

use mini_privacy_policy::{expected_cost, PrivacyTier, ResourceCost};

use crate::error::{PricingError, Result};

/// A governed price for one unit of a resource, in micro-MINI (10⁻⁶ MINI —
/// the same convention `mini-settlement`/`mini-bounty`/`mini-reward`
/// already use for a plain, non-confidential amount).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceVector {
    /// Micro-MINI per megabyte of bandwidth at Tier 0 (1x) cost.
    pub bandwidth_micro_mini_per_mb: u64,
    /// Micro-MINI per megabyte-day of storage at Tier 0 (1x) cost.
    pub storage_micro_mini_per_mb_day: u64,
}

/// A declared price range for one tier/payload combination. A range, not
/// a single number, because [`ResourceCost`]'s own multipliers are a
/// range — collapsing it to one figure here would be exactly the kind of
/// overclaim the cost doctrine forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    pub tier: PrivacyTier,
    pub min_micro_mini: u64,
    pub max_micro_mini: u64,
    /// Mirrors [`ResourceCost::requires_payment`] — `Tier::Direct` alone
    /// is free of a resource-market assumption.
    pub requires_payment: bool,
}

fn price_component(
    price_per_unit_micro_mini: u64,
    multiplier_millix: u32,
    units: u64,
) -> Result<u64> {
    let scaled = (price_per_unit_micro_mini as u128)
        .checked_mul(units as u128)
        .and_then(|v| v.checked_mul(multiplier_millix as u128))
        .ok_or(PricingError::Overflow)?
        / 1000;
    u64::try_from(scaled).map_err(|_| PricingError::Overflow)
}

/// Quote `tier` for `payload_mb` of bandwidth held in storage for
/// `storage_days`. Combines [`expected_cost`]'s declared multiplier range
/// with `prices`' per-unit rates; does not itself validate that `tier`
/// can actually satisfy any particular [`mini_privacy_policy::
/// ProtectionProperty`] — that check belongs to `mini-transport-policy`'s
/// router, a separate, independent lane this crate deliberately does not
/// depend on.
pub fn quote(
    prices: &PriceVector,
    tier: PrivacyTier,
    payload_mb: u64,
    storage_days: u64,
) -> Result<Quote> {
    let cost: ResourceCost = expected_cost(tier);
    let storage_unit = payload_mb
        .checked_mul(storage_days)
        .ok_or(PricingError::Overflow)?;

    let bandwidth_min = price_component(
        prices.bandwidth_micro_mini_per_mb,
        cost.bandwidth_multiplier_millix_min,
        payload_mb,
    )?;
    let bandwidth_max = price_component(
        prices.bandwidth_micro_mini_per_mb,
        cost.bandwidth_multiplier_millix_max,
        payload_mb,
    )?;
    let storage_min = price_component(
        prices.storage_micro_mini_per_mb_day,
        cost.storage_multiplier_millix_min,
        storage_unit,
    )?;
    let storage_max = price_component(
        prices.storage_micro_mini_per_mb_day,
        cost.storage_multiplier_millix_max,
        storage_unit,
    )?;

    let min_micro_mini = bandwidth_min
        .checked_add(storage_min)
        .ok_or(PricingError::Overflow)?;
    let max_micro_mini = bandwidth_max
        .checked_add(storage_max)
        .ok_or(PricingError::Overflow)?;

    Ok(Quote {
        tier,
        min_micro_mini,
        max_micro_mini,
        requires_payment: cost.requires_payment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prices() -> PriceVector {
        PriceVector {
            bandwidth_micro_mini_per_mb: 1_000,
            storage_micro_mini_per_mb_day: 10,
        }
    }

    #[test]
    fn direct_tier_has_no_range_min_equals_max() {
        let q = quote(&prices(), PrivacyTier::Direct, 10, 1).unwrap();
        assert_eq!(q.min_micro_mini, q.max_micro_mini);
        assert!(!q.requires_payment);
    }

    #[test]
    fn relayed_tier_has_a_real_range() {
        let q = quote(&prices(), PrivacyTier::Relayed, 10, 1).unwrap();
        assert!(q.min_micro_mini < q.max_micro_mini);
        assert!(q.requires_payment);
    }

    #[test]
    fn max_is_never_less_than_min_for_any_tier() {
        for tier in [
            PrivacyTier::Direct,
            PrivacyTier::Relayed,
            PrivacyTier::Mixed,
            PrivacyTier::Burst,
        ] {
            let q = quote(&prices(), tier, 5, 3).unwrap();
            assert!(
                q.max_micro_mini >= q.min_micro_mini,
                "{tier:?} violated max >= min"
            );
        }
    }

    #[test]
    fn burst_costs_at_least_as_much_as_mixed_at_the_same_payload() {
        let mixed = quote(&prices(), PrivacyTier::Mixed, 10, 5).unwrap();
        let burst = quote(&prices(), PrivacyTier::Burst, 10, 5).unwrap();
        assert!(burst.min_micro_mini >= mixed.min_micro_mini);
        assert!(burst.max_micro_mini >= mixed.max_micro_mini);
    }

    #[test]
    fn zero_payload_and_zero_storage_days_quotes_zero() {
        let q = quote(&prices(), PrivacyTier::Mixed, 0, 0).unwrap();
        assert_eq!(q.min_micro_mini, 0);
        assert_eq!(q.max_micro_mini, 0);
    }

    #[test]
    fn an_overflowing_payload_is_rejected_not_silently_truncated() {
        let err = quote(&prices(), PrivacyTier::Burst, u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, PricingError::Overflow);
    }

    #[test]
    fn quote_is_deterministic_for_the_same_inputs() {
        let a = quote(&prices(), PrivacyTier::Mixed, 42, 7).unwrap();
        let b = quote(&prices(), PrivacyTier::Mixed, 42, 7).unwrap();
        assert_eq!(a, b);
    }
}

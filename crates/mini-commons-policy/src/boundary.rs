//! The paid-service boundary (research doctrine §7/§26, Track C4: "Ensure
//! only additional external service is quoted and settled").
//!
//! [`service_quote_for`] is the single typed crossing point between this
//! crate's free commons entitlements and `mini-resource-pricing`'s paid
//! tiers: requesting [`PrivacyTier::Direct`] -- the base service every
//! [`crate::PublicCommonsPolicy`] entitlement already grants for free --
//! never produces a quote, no matter what `entitlement` is. Requesting any
//! higher tier always produces the same quote `mini-resource-pricing`
//! would compute on its own, again regardless of `entitlement`. The two
//! axes never leak into each other: an entitlement cannot buy a cheaper
//! tier, and a tier can never grant or replace an entitlement.

use mini_privacy_policy::PrivacyTier;
use mini_resource_pricing::{quote, PriceVector, Quote, Result};

use crate::policy::Entitlement;

/// The quote for requesting `tier` at `payload_mb`/`storage_days`, given a
/// caller's `entitlement` for the underlying commons action. Returns
/// `Ok(None)` for [`PrivacyTier::Direct`] unconditionally -- the base
/// service is never settled, whatever the entitlement is. Returns
/// `Ok(Some(quote))` for every other tier, identical to calling
/// [`mini_resource_pricing::quote`] directly: `entitlement` never changes
/// the price. `entitlement` is taken only so this function's own tests can
/// prove that fact -- the parameter is otherwise unused.
pub fn service_quote_for(
    _entitlement: Entitlement,
    tier: PrivacyTier,
    prices: &PriceVector,
    payload_mb: u64,
    storage_days: u64,
) -> Result<Option<Quote>> {
    if tier == PrivacyTier::Direct {
        return Ok(None);
    }
    quote(prices, tier, payload_mb, storage_days).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_resource_pricing::PricingError;

    fn prices() -> PriceVector {
        PriceVector {
            bandwidth_micro_mini_per_mb: 1_000,
            storage_micro_mini_per_mb_day: 10,
        }
    }

    #[test]
    fn direct_tier_is_never_quoted_for_a_free_protocol_right() {
        let result = service_quote_for(
            Entitlement::FreeProtocolRight,
            PrivacyTier::Direct,
            &prices(),
            10,
            1,
        )
        .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn direct_tier_is_never_quoted_even_for_an_unsupported_entitlement() {
        let result = service_quote_for(
            Entitlement::Unsupported,
            PrivacyTier::Direct,
            &prices(),
            10,
            1,
        )
        .unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn direct_tier_stays_unquoted_across_a_payload_and_duration_sweep() {
        for payload_mb in [0, 1, 1_000, u64::MAX / 2] {
            for storage_days in [0, 1, 1_000, u64::MAX / 2] {
                let result = service_quote_for(
                    Entitlement::FreeProtocolRight,
                    PrivacyTier::Direct,
                    &prices(),
                    payload_mb,
                    storage_days,
                )
                .unwrap();
                assert_eq!(result, None, "Direct tier must never be quoted");
            }
        }
    }

    #[test]
    fn every_non_direct_tier_is_quoted_and_requires_payment() {
        for tier in [PrivacyTier::Relayed, PrivacyTier::Mixed, PrivacyTier::Burst] {
            let result =
                service_quote_for(Entitlement::FreeProtocolRight, tier, &prices(), 10, 1).unwrap();
            let q = result.expect("a non-Direct tier must always be quoted");
            assert!(q.requires_payment);
            assert_eq!(q.tier, tier);
        }
    }

    #[test]
    fn the_quote_for_a_paid_tier_is_identical_regardless_of_entitlement() {
        let free = service_quote_for(
            Entitlement::FreeProtocolRight,
            PrivacyTier::Mixed,
            &prices(),
            42,
            7,
        )
        .unwrap();
        let unsupported = service_quote_for(
            Entitlement::Unsupported,
            PrivacyTier::Mixed,
            &prices(),
            42,
            7,
        )
        .unwrap();
        assert_eq!(
            free, unsupported,
            "entitlement status must never change the price of a paid tier"
        );
    }

    #[test]
    fn the_quote_for_a_paid_tier_matches_calling_mini_resource_pricing_directly() {
        let via_boundary = service_quote_for(
            Entitlement::FreeProtocolRight,
            PrivacyTier::Relayed,
            &prices(),
            10,
            1,
        )
        .unwrap()
        .unwrap();
        let direct = quote(&prices(), PrivacyTier::Relayed, 10, 1).unwrap();
        assert_eq!(via_boundary, direct);
    }

    #[test]
    fn an_overflowing_paid_tier_quote_still_propagates_the_pricing_error() {
        let err = service_quote_for(
            Entitlement::FreeProtocolRight,
            PrivacyTier::Burst,
            &prices(),
            u64::MAX,
            u64::MAX,
        )
        .unwrap_err();
        assert_eq!(err, PricingError::Overflow);
    }
}

//! Protection quote and achieved-result receipt (D-0364, Track D2, founder
//! research `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §27: "Connect to existing privacy and resource-pricing vocabulary.")
//!
//! [`achieved_result_receipt_for`] is the one function in this crate that
//! actually *does* anything beyond holding data: given a
//! [`crate::PublicationProfile`] and the [`mini_privacy_policy::
//! ProtectionProperty`]s a caller wants that publication to achieve, it
//! reuses `mini-transport-policy`'s existing fail-closed router to decide
//! whether the profile's chosen [`mini_privacy_policy::PrivacyTier`]
//! actually satisfies them, then reuses `mini-resource-pricing`'s existing
//! quote engine for the price. It invents no new routing or pricing logic
//! of its own -- exactly the "connect to existing vocabulary" scope Track
//! D2 asks for, mirroring how Track C4's `service_quote_for` connected
//! `mini-commons-policy` to the same pricing engine.
//!
//! **This is a quote and a routing decision, not proof that a publication
//! happened.** No object is stored, no bytes move, no payment executes --
//! see [`mini_transport_policy::route`] and [`mini_resource_pricing::quote`]'s
//! own module docs for the same honesty boundary this crate inherits.

use mini_privacy_policy::{AchievedPrivacy, PrivacyRequest, PrivacyTier, ProtectionProperty};
use mini_resource_pricing::{quote, PriceVector, Quote};
use mini_transport_policy::{route, PayloadSizeClass, TransportRequest};

use crate::error::Result;
use crate::profile::PublicationProfile;

/// What was actually routable and payable for a given
/// [`PublicationProfile`] and set of requested protection properties.
/// `quote` is `None` exactly when [`PublicationProfile::transport`] is
/// [`PrivacyTier::Direct`] -- the same "free base tier is never quoted"
/// convention `mini-commons-policy`'s `service_quote_for` already
/// established for Track C4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievedResultReceipt {
    pub profile: PublicationProfile,
    pub achieved: AchievedPrivacy,
    pub quote: Option<Quote>,
}

/// Build an [`AchievedResultReceipt`] for `profile`, given the protection
/// properties the caller wants satisfied and the payload/storage this
/// publication needs.
///
/// **Fails closed**: if `profile.transport` cannot satisfy every property
/// in `properties`, this returns [`crate::PublicationPolicyError::Routing`]
/// (from `mini-transport-policy`'s own router) rather than silently
/// returning a receipt that claims a protection level the chosen tier
/// does not actually reach.
pub fn achieved_result_receipt_for(
    profile: PublicationProfile,
    properties: Vec<ProtectionProperty>,
    payload_size_class: PayloadSizeClass,
    prices: &PriceVector,
    payload_mb: u64,
    storage_days: u64,
) -> Result<AchievedResultReceipt> {
    let decision = route(&TransportRequest {
        privacy: PrivacyRequest {
            tier: profile.transport,
            properties,
        },
        payload_size_class,
    })?;

    let price_quote = if profile.transport == PrivacyTier::Direct {
        None
    } else {
        Some(quote(prices, profile.transport, payload_mb, storage_days)?)
    };

    Ok(AchievedResultReceipt {
        profile,
        achieved: decision.achieved,
        quote: price_quote,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Attribution, Persistence, Visibility};

    fn prices() -> PriceVector {
        PriceVector {
            bandwidth_micro_mini_per_mb: 1_000,
            storage_micro_mini_per_mb_day: 10,
        }
    }

    fn profile(transport: PrivacyTier) -> PublicationProfile {
        PublicationProfile {
            visibility: Visibility::Public,
            attribution: Attribution::Anonymous,
            transport,
            persistence: Persistence::Durable,
        }
    }

    #[test]
    fn direct_tier_receipt_has_no_quote() {
        let receipt = achieved_result_receipt_for(
            profile(PrivacyTier::Direct),
            vec![],
            PayloadSizeClass::Small,
            &prices(),
            10,
            1,
        )
        .unwrap();
        assert!(receipt.quote.is_none());
        assert_eq!(receipt.achieved.tier, PrivacyTier::Direct);
    }

    #[test]
    fn relayed_tier_receipt_has_a_quote_that_requires_payment() {
        let receipt = achieved_result_receipt_for(
            profile(PrivacyTier::Relayed),
            vec![ProtectionProperty::CounterpartyIpHiding],
            PayloadSizeClass::Small,
            &prices(),
            10,
            1,
        )
        .unwrap();
        let quote = receipt.quote.unwrap();
        assert!(quote.requires_payment);
        assert_eq!(quote.tier, PrivacyTier::Relayed);
    }

    #[test]
    fn an_unsatisfiable_property_fails_closed_rather_than_under_delivering() {
        let err = achieved_result_receipt_for(
            profile(PrivacyTier::Direct),
            vec![ProtectionProperty::WhoTalksToWhomHiding],
            PayloadSizeClass::Small,
            &prices(),
            10,
            1,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::PublicationPolicyError::Routing(
                mini_transport_policy::TransportPolicyError::UnsatisfiableProperty { .. }
            )
        ));
    }

    #[test]
    fn the_receipt_carries_the_exact_profile_it_was_built_for() {
        let built_profile = profile(PrivacyTier::Mixed);
        let receipt = achieved_result_receipt_for(
            built_profile,
            vec![],
            PayloadSizeClass::Medium,
            &prices(),
            5,
            2,
        )
        .unwrap();
        assert_eq!(receipt.profile, built_profile);
    }

    #[test]
    fn quote_matches_calling_mini_resource_pricing_quote_directly() {
        let receipt = achieved_result_receipt_for(
            profile(PrivacyTier::Burst),
            vec![],
            PayloadSizeClass::Large,
            &prices(),
            20,
            3,
        )
        .unwrap();
        let direct_quote = quote(&prices(), PrivacyTier::Burst, 20, 3).unwrap();
        assert_eq!(receipt.quote.unwrap(), direct_quote);
    }

    #[test]
    fn an_overflowing_payload_propagates_as_a_pricing_error() {
        let err = achieved_result_receipt_for(
            profile(PrivacyTier::Burst),
            vec![],
            PayloadSizeClass::Large,
            &prices(),
            u64::MAX,
            u64::MAX,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::PublicationPolicyError::Pricing(mini_resource_pricing::PricingError::Overflow)
        ));
    }
}

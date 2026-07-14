//! `TransportRequest` policy router (D-0301; founder research phase P2,
//! `MN-201`): given a [`mini_privacy_policy::PrivacyRequest`], decide which
//! [`mini_privacy_policy::Mechanism`]s *would* satisfy it and at what
//! declared cost — routing *decisions* only.
//!
//! **No transport exists yet to execute a routing decision.** This crate
//! has zero dependency on `mini-net`/`mini-bearer` on purpose: a relay,
//! mixnet, or bridge (later lanes — see `docs/design/
//! privacy-cost-doctrine-parallel-execution-plan.md`) consumes a
//! [`RouteDecision`] to know *what* to build; this crate never dials a
//! socket. `route`'s cost is [`mini_privacy_policy::expected_cost`]'s own
//! declared estimate, not a measurement.

use mini_privacy_policy::{
    expected_cost, AchievedPrivacy, Mechanism, PrivacyRequest, PrivacyTier, ProtectionProperty,
};

use crate::error::{Result, TransportPolicyError};

/// A coarse payload size class, since mixing/padding cost depends on
/// payload size the way `mini_privacy_policy::ResourceCost` alone cannot
/// express (that type is tier-level, not per-payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadSizeClass {
    /// Fits in a single mix/relay frame with no chunking.
    Small,
    /// Needs chunking at Tier 2+ (mix networks pad to fixed frame
    /// sizes — a large payload is many frames, not one padded frame).
    Medium,
    /// Large enough that Tier 3's erasure-replication storage cost
    /// dominates over its transport cost.
    Large,
}

/// What a caller wants moved, and under what privacy policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequest {
    pub privacy: PrivacyRequest,
    pub payload_size_class: PayloadSizeClass,
}

/// The router's answer: the mechanisms this tier requires, and the
/// resulting [`AchievedPrivacy`] record (declared, not measured — see
/// this module's own doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteDecision {
    pub achieved: AchievedPrivacy,
    pub payload_size_class: PayloadSizeClass,
}

/// The minimum tier at which a mechanism becomes available. Fixed
/// mapping, not user-configurable: a caller cannot request "Relayed but
/// with mix-network cover traffic," because that combination doesn't
/// correspond to any tier this workspace has actually designed for
/// (research doc §1.3's "why one maximum tier for everything fails the
/// doctrine" cuts the other way too — an undefined *partial* tier is
/// exactly the kind of unpriced combination the cost doctrine forbids).
fn mechanisms_for_tier(tier: PrivacyTier) -> Vec<Mechanism> {
    match tier {
        PrivacyTier::Direct => vec![Mechanism::AeadEncryption],
        PrivacyTier::Relayed => vec![Mechanism::AeadEncryption, Mechanism::OnionRelay],
        PrivacyTier::Mixed => vec![
            Mechanism::AeadEncryption,
            Mechanism::OnionRelay,
            Mechanism::MixNetwork,
            Mechanism::TrafficPadding,
            Mechanism::CoverTraffic,
            Mechanism::BoundedRandomDelay,
        ],
        PrivacyTier::Burst => {
            let mut mechanisms = mechanisms_for_tier(PrivacyTier::Mixed);
            mechanisms.push(Mechanism::ErasureCodedReplication);
            mechanisms
        }
    }
}

/// The minimum tier at which a property is achievable. A judgment call
/// recorded in D-0301 — not itself a proof, a routing policy this
/// workspace can revise as real mechanisms land. Conservative by design:
/// when unsure, this maps a property to a *higher* tier than it might
/// eventually need, since [`route`] fails closed on an under-provisioned
/// request rather than silently under-delivering (see [`route`]'s docs).
fn property_min_tier(property: ProtectionProperty) -> PrivacyTier {
    match property {
        ProtectionProperty::ContentSecrecy
        | ProtectionProperty::StorageIntegrity
        | ProtectionProperty::StorageAvailability
        | ProtectionProperty::HumanLivenessSignal
        | ProtectionProperty::HumanUniquenessSignal => PrivacyTier::Direct,
        ProtectionProperty::CounterpartyIpHiding | ProtectionProperty::MetadataMinimization => {
            PrivacyTier::Relayed
        }
        ProtectionProperty::WhoTalksToWhomHiding
        | ProtectionProperty::PaymentUnlinkability
        | ProtectionProperty::RequestUnlinkability
        | ProtectionProperty::TimingCorrelationResistance
        | ProtectionProperty::CensorshipResistance => PrivacyTier::Mixed,
        ProtectionProperty::SuppressionResistance => PrivacyTier::Burst,
        // `ProtectionProperty` is `#[non_exhaustive]` so this crate can be
        // extended without a breaking change here. Any future variant this
        // match doesn't yet know defaults to the highest tier: claiming a
        // lower tier suffices for a property this crate has no mapping for
        // yet would be exactly the kind of unearned confidence the cost
        // doctrine forbids.
        _ => PrivacyTier::Burst,
    }
}

/// Route `request`: verify every requested property is achievable at the
/// requested tier, then return the declared [`RouteDecision`].
///
/// **Fails closed, never downgrades silently**: if any requested
/// property needs a higher tier than [`PrivacyRequest::tier`], this
/// returns [`TransportPolicyError::UnsatisfiableProperty`] rather than
/// quietly routing at the (cheaper, weaker) requested tier anyway — a
/// caller that asked for `WhoTalksToWhomHiding` at `Tier::Relayed` gets a
/// clear error, not an `AchievedPrivacy` that silently doesn't actually
/// achieve what was asked.
pub fn route(request: &TransportRequest) -> Result<RouteDecision> {
    for &property in &request.privacy.properties {
        let minimum_tier = property_min_tier(property);
        if minimum_tier > request.privacy.tier {
            return Err(TransportPolicyError::UnsatisfiableProperty {
                property,
                requested_tier: request.privacy.tier,
                minimum_tier,
            });
        }
    }
    let tier = request.privacy.tier;
    let achieved = AchievedPrivacy::new(tier, mechanisms_for_tier(tier), expected_cost(tier));
    Ok(RouteDecision {
        achieved,
        payload_size_class: request.payload_size_class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(tier: PrivacyTier, properties: Vec<ProtectionProperty>) -> TransportRequest {
        TransportRequest {
            privacy: PrivacyRequest { tier, properties },
            payload_size_class: PayloadSizeClass::Small,
        }
    }

    #[test]
    fn direct_tier_with_no_properties_routes_with_only_aead() {
        let decision = route(&request(PrivacyTier::Direct, vec![])).unwrap();
        assert_eq!(decision.achieved.tier, PrivacyTier::Direct);
        assert_eq!(
            decision.achieved.mechanisms,
            vec![Mechanism::AeadEncryption]
        );
    }

    #[test]
    fn relayed_tier_satisfies_counterparty_ip_hiding() {
        let decision = route(&request(
            PrivacyTier::Relayed,
            vec![ProtectionProperty::CounterpartyIpHiding],
        ))
        .unwrap();
        assert!(decision
            .achieved
            .mechanisms
            .contains(&Mechanism::OnionRelay));
    }

    #[test]
    fn mixed_tier_satisfies_who_talks_to_whom_hiding() {
        let decision = route(&request(
            PrivacyTier::Mixed,
            vec![ProtectionProperty::WhoTalksToWhomHiding],
        ))
        .unwrap();
        assert!(decision
            .achieved
            .mechanisms
            .contains(&Mechanism::MixNetwork));
    }

    #[test]
    fn burst_tier_adds_erasure_coded_replication() {
        let decision = route(&request(PrivacyTier::Burst, vec![])).unwrap();
        assert!(decision
            .achieved
            .mechanisms
            .contains(&Mechanism::ErasureCodedReplication));
    }

    #[test]
    fn asking_for_who_talks_to_whom_hiding_at_direct_tier_fails_closed() {
        let err = route(&request(
            PrivacyTier::Direct,
            vec![ProtectionProperty::WhoTalksToWhomHiding],
        ))
        .unwrap_err();
        assert_eq!(
            err,
            TransportPolicyError::UnsatisfiableProperty {
                property: ProtectionProperty::WhoTalksToWhomHiding,
                requested_tier: PrivacyTier::Direct,
                minimum_tier: PrivacyTier::Mixed,
            }
        );
    }

    #[test]
    fn asking_for_suppression_resistance_below_burst_fails_closed() {
        let err = route(&request(
            PrivacyTier::Mixed,
            vec![ProtectionProperty::SuppressionResistance],
        ))
        .unwrap_err();
        assert_eq!(
            err,
            TransportPolicyError::UnsatisfiableProperty {
                property: ProtectionProperty::SuppressionResistance,
                requested_tier: PrivacyTier::Mixed,
                minimum_tier: PrivacyTier::Burst,
            }
        );
    }

    #[test]
    fn a_request_that_is_satisfiable_at_a_higher_tier_than_needed_still_routes() {
        // Direct-tier-achievable property, requested at Burst: over-provisioning
        // is never an error, only under-provisioning is.
        let decision = route(&request(
            PrivacyTier::Burst,
            vec![ProtectionProperty::ContentSecrecy],
        ))
        .unwrap();
        assert_eq!(decision.achieved.tier, PrivacyTier::Burst);
    }

    #[test]
    fn every_tiers_mechanism_list_is_a_superset_of_the_tier_belows() {
        let tiers = [
            PrivacyTier::Direct,
            PrivacyTier::Relayed,
            PrivacyTier::Mixed,
            PrivacyTier::Burst,
        ];
        for pair in tiers.windows(2) {
            let lower = mechanisms_for_tier(pair[0]);
            let higher = mechanisms_for_tier(pair[1]);
            for m in &lower {
                assert!(
                    higher.contains(m),
                    "{:?}'s mechanisms must be a subset of {:?}'s",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn payload_size_class_is_carried_through_unchanged() {
        let req = TransportRequest {
            privacy: PrivacyRequest {
                tier: PrivacyTier::Direct,
                properties: vec![],
            },
            payload_size_class: PayloadSizeClass::Large,
        };
        let decision = route(&req).unwrap();
        assert_eq!(decision.payload_size_class, PayloadSizeClass::Large);
    }
}

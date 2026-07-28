//! Source-hiding publication path (D-0365, Track D3, founder research
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §27: "Use role separation and relay infrastructure.")
//!
//! [`source_hiding_publication_path_for`] plans which `mini-relay` roles a
//! publication needs to hide the *publisher's network counterparty* (the
//! entry point that would otherwise learn the publisher's address) --
//! this is deliberately **not** gated on [`crate::Attribution`]. Hiding
//! the network path a publish request travels and disclosing the
//! publisher's identity root inside the published object are orthogonal:
//! a caller can want `Attribution::Attributed` content delivered over a
//! source-hidden path just as easily as `Attribution::Anonymous` content
//! delivered directly. Coupling this function to `Attribution` would
//! reintroduce exactly the cross-dimension assumption Track D1's
//! independence requirement forbids (see [`crate::profile`]'s module
//! doc) -- so this function always requests
//! [`mini_privacy_policy::ProtectionProperty::CounterpartyIpHiding`]
//! regardless of `profile.attribution`, and lets `profile.transport`
//! alone decide whether that's achievable.
//!
//! This composes two already-existing, already-tested layers and adds no
//! new relay logic of its own: `mini-transport-policy::route` (fail-closed
//! property check) and `mini-relay::roles_for_route_decision` (route
//! decision -> role list). Both were built by earlier lanes (D-0301,
//! D-0306) specifically so later work would not need to re-derive this.

use mini_privacy_policy::{AchievedPrivacy, PrivacyRequest, ProtectionProperty};
use mini_relay::{roles_for_route_decision, RelayRole};
use mini_transport_policy::{route, PayloadSizeClass, TransportRequest};

use crate::error::Result;
use crate::profile::PublicationProfile;

/// A planned set of `mini-relay` roles that would carry `profile`'s
/// publication with the publisher's network counterparty hidden from any
/// single relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceHidingPublicationPath {
    pub profile: PublicationProfile,
    pub achieved: AchievedPrivacy,
    pub roles: Vec<RelayRole>,
}

/// Plan a source-hiding path for `profile`.
///
/// **Fails closed at two different layers, both inherited rather than
/// re-derived here.** `mini-transport-policy::route` itself already
/// knows `CounterpartyIpHiding` needs at least `PrivacyTier::Relayed`,
/// so `profile.transport == PrivacyTier::Direct` is rejected there, as
/// [`crate::PublicationPolicyError::Routing`]
/// (`TransportPolicyError::UnsatisfiableProperty`), before this function
/// ever calls into `mini-relay`. `Mixed`/`Burst` both satisfy that
/// minimum-tier check (they are strictly higher than `Relayed`) but
/// still need the mix network `mini-relay` does not implement yet, so
/// they fail one layer later, in
/// [`mini_relay::roles_for_route_decision`], as
/// [`crate::PublicationPolicyError::Relay`]
/// (`RelayError::TierNotHandledByThisCrate`, gated behind D-0047/D-0305
/// external review, Track D4).
///
/// **This is a role *plan*, not a live path.** No relay identity is
/// contacted, no socket is dialed, and no [`mini_relay::
/// DeliveryAssignment`] is produced -- assigning real relay identities to
/// these roles (and then calling [`mini_relay::enforce_role_separation`]
/// on the result) is a separate discovery/selection concern this crate
/// does not have the information to perform on a caller's behalf.
pub fn source_hiding_publication_path_for(
    profile: PublicationProfile,
    payload_size_class: PayloadSizeClass,
) -> Result<SourceHidingPublicationPath> {
    let decision = route(&TransportRequest {
        privacy: PrivacyRequest {
            tier: profile.transport,
            properties: vec![ProtectionProperty::CounterpartyIpHiding],
        },
        payload_size_class,
    })?;
    let roles = roles_for_route_decision(&decision)?;
    Ok(SourceHidingPublicationPath {
        profile,
        achieved: decision.achieved,
        roles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Attribution, Persistence, Visibility};
    use mini_privacy_policy::PrivacyTier;
    use mini_relay::RelayError;

    fn profile(attribution: Attribution, transport: PrivacyTier) -> PublicationProfile {
        PublicationProfile {
            visibility: Visibility::Unlisted,
            attribution,
            transport,
            persistence: Persistence::Durable,
        }
    }

    #[test]
    fn a_relayed_tier_profile_plans_entry_and_rendezvous() {
        let path = source_hiding_publication_path_for(
            profile(Attribution::Anonymous, PrivacyTier::Relayed),
            PayloadSizeClass::Small,
        )
        .unwrap();
        assert_eq!(path.roles, vec![RelayRole::Entry, RelayRole::Rendezvous]);
    }

    #[test]
    fn an_attributed_profile_still_gets_a_source_hiding_path() {
        // Attribution and source-hiding are independent dimensions --
        // see this module's own doc comment for why this must not be
        // gated on Attribution::Anonymous.
        let path = source_hiding_publication_path_for(
            profile(Attribution::Attributed, PrivacyTier::Relayed),
            PayloadSizeClass::Small,
        )
        .unwrap();
        assert_eq!(path.roles, vec![RelayRole::Entry, RelayRole::Rendezvous]);
    }

    #[test]
    fn a_direct_tier_profile_fails_closed_at_the_routing_layer() {
        // CounterpartyIpHiding needs at least Relayed tier -- caught by
        // mini-transport-policy::route itself, before mini-relay is ever
        // consulted.
        let err = source_hiding_publication_path_for(
            profile(Attribution::Anonymous, PrivacyTier::Direct),
            PayloadSizeClass::Small,
        )
        .unwrap_err();
        assert_eq!(
            err,
            crate::PublicationPolicyError::Routing(
                mini_transport_policy::TransportPolicyError::UnsatisfiableProperty {
                    property: ProtectionProperty::CounterpartyIpHiding,
                    requested_tier: PrivacyTier::Direct,
                    minimum_tier: PrivacyTier::Relayed,
                }
            )
        );
    }

    #[test]
    fn a_mixed_tier_profile_fails_closed_not_yet_handled() {
        let err = source_hiding_publication_path_for(
            profile(Attribution::Anonymous, PrivacyTier::Mixed),
            PayloadSizeClass::Small,
        )
        .unwrap_err();
        assert_eq!(
            err,
            crate::PublicationPolicyError::Relay(RelayError::TierNotHandledByThisCrate)
        );
    }

    #[test]
    fn a_burst_tier_profile_fails_closed_not_yet_handled() {
        let err = source_hiding_publication_path_for(
            profile(Attribution::Anonymous, PrivacyTier::Burst),
            PayloadSizeClass::Small,
        )
        .unwrap_err();
        assert_eq!(
            err,
            crate::PublicationPolicyError::Relay(RelayError::TierNotHandledByThisCrate)
        );
    }

    #[test]
    fn the_path_carries_the_exact_profile_it_was_built_for() {
        let built_profile = profile(Attribution::Anonymous, PrivacyTier::Relayed);
        let path =
            source_hiding_publication_path_for(built_profile, PayloadSizeClass::Medium).unwrap();
        assert_eq!(path.profile, built_profile);
    }

    #[test]
    fn the_achieved_privacy_names_the_onion_relay_mechanism() {
        let path = source_hiding_publication_path_for(
            profile(Attribution::Anonymous, PrivacyTier::Relayed),
            PayloadSizeClass::Small,
        )
        .unwrap();
        assert!(path
            .achieved
            .mechanisms
            .contains(&mini_privacy_policy::Mechanism::OnionRelay));
    }
}

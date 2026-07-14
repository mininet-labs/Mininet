//! Bridges `mini_transport_policy::RouteDecision` to this crate's relay
//! role planning — the missing link D-0306's own "Required follow-up"
//! flagged: `route()` and `mini-relay` were two disconnected layers,
//! decision and mechanism, with nothing translating one into the other.
//!
//! [`roles_for_route_decision`] is deliberately narrow: it only accepts a
//! decision whose tier is [`PrivacyTier::Relayed`] and whose mechanism
//! list actually names [`Mechanism::OnionRelay`]. `Direct` needs no
//! relay at all; `Mixed`/`Burst` need the mix network (`MN-205`), which
//! does not exist yet and is gated behind external review (D-0047,
//! D-0305) — this function must not claim to satisfy a tier it cannot
//! actually provide.

use mini_privacy_policy::{Mechanism, PrivacyTier};
use mini_transport_policy::RouteDecision;

use crate::error::{RelayError, Result};
use crate::role::RelayRole;

/// The relay roles this crate's protocol must supply to carry out
/// `decision`. Returns exactly `[Entry, Rendezvous]` for a `Relayed`-tier
/// decision that names `Mechanism::OnionRelay` — the mandatory pair
/// [`crate::role_separation::enforce_role_separation`] itself requires.
/// `Delivery` is never planned here: whether an extra hop is warranted is
/// a caller/policy decision this function does not make on its own.
pub fn roles_for_route_decision(decision: &RouteDecision) -> Result<Vec<RelayRole>> {
    if decision.achieved.tier == PrivacyTier::Direct {
        return Err(RelayError::TierNeedsNoRelay);
    }
    if decision.achieved.tier != PrivacyTier::Relayed {
        return Err(RelayError::TierNotHandledByThisCrate);
    }
    if !decision
        .achieved
        .mechanisms
        .contains(&Mechanism::OnionRelay)
    {
        return Err(RelayError::MechanismNotRequested);
    }
    Ok(vec![RelayRole::Entry, RelayRole::Rendezvous])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_privacy_policy::PrivacyRequest;
    use mini_transport_policy::{route, PayloadSizeClass, TransportRequest};

    fn decision_for(tier: PrivacyTier) -> RouteDecision {
        let request = TransportRequest {
            privacy: PrivacyRequest {
                tier,
                properties: vec![],
            },
            payload_size_class: PayloadSizeClass::Small,
        };
        route(&request).unwrap()
    }

    #[test]
    fn a_relayed_tier_decision_plans_entry_and_rendezvous() {
        let decision = decision_for(PrivacyTier::Relayed);
        let roles = roles_for_route_decision(&decision).unwrap();
        assert_eq!(roles, vec![RelayRole::Entry, RelayRole::Rendezvous]);
    }

    #[test]
    fn a_direct_tier_decision_needs_no_relay() {
        let decision = decision_for(PrivacyTier::Direct);
        assert_eq!(
            roles_for_route_decision(&decision).unwrap_err(),
            RelayError::TierNeedsNoRelay
        );
    }

    #[test]
    fn a_mixed_tier_decision_is_not_handled_by_this_crate() {
        let decision = decision_for(PrivacyTier::Mixed);
        assert_eq!(
            roles_for_route_decision(&decision).unwrap_err(),
            RelayError::TierNotHandledByThisCrate
        );
    }

    #[test]
    fn a_burst_tier_decision_is_not_handled_by_this_crate() {
        let decision = decision_for(PrivacyTier::Burst);
        assert_eq!(
            roles_for_route_decision(&decision).unwrap_err(),
            RelayError::TierNotHandledByThisCrate
        );
    }

    #[test]
    fn a_decision_missing_the_onion_relay_mechanism_is_rejected() {
        let mut decision = decision_for(PrivacyTier::Relayed);
        decision
            .achieved
            .mechanisms
            .retain(|m| *m != Mechanism::OnionRelay);
        assert_eq!(
            roles_for_route_decision(&decision).unwrap_err(),
            RelayError::MechanismNotRequested
        );
    }

    #[test]
    fn the_planned_roles_satisfy_role_separation_with_distinct_relays() {
        use crate::role_separation::{enforce_role_separation, DeliveryAssignment};
        use did_mini::Controller;

        let decision = decision_for(PrivacyTier::Relayed);
        let roles = roles_for_route_decision(&decision).unwrap();
        let assignments: Vec<DeliveryAssignment> = roles
            .into_iter()
            .map(|role| DeliveryAssignment {
                role,
                relay: Controller::incept_single().unwrap().did(),
            })
            .collect();
        enforce_role_separation(&assignments).unwrap();
    }
}

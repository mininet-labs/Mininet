//! Enforces research §5.2's rule: "prevent one provider owning all roles
//! for one delivery." Given the relay identities assigned to a single
//! delivery, [`enforce_role_separation`] rejects any assignment where the
//! same identity appears in more than one role, and any assignment
//! missing a mandatory role.

use did_mini::Did;

use crate::error::{RelayError, Result};
use crate::role::RelayRole;

/// One relay identity's assigned role within a single delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryAssignment {
    pub role: RelayRole,
    pub relay: Did,
}

/// Reject an assignment where:
/// - the mandatory `Entry` or `Rendezvous` role is missing (research
///   §5.2: "no direct user-to-user connection" — every delivery needs at
///   least these two hops);
/// - any role (`Entry`/`Rendezvous`/`Delivery`) is assigned twice;
/// - the same relay identity ([`Did`]) is assigned to more than one role.
///
/// `Delivery` is optional — a delivery with only `Entry` + `Rendezvous`
/// is valid.
pub fn enforce_role_separation(assignments: &[DeliveryAssignment]) -> Result<()> {
    let mut seen_entry = false;
    let mut seen_rendezvous = false;
    let mut seen_delivery = false;

    for (i, a) in assignments.iter().enumerate() {
        match a.role {
            RelayRole::Entry => {
                if seen_entry {
                    return Err(RelayError::DuplicateRole(RelayRole::Entry));
                }
                seen_entry = true;
            }
            RelayRole::Rendezvous => {
                if seen_rendezvous {
                    return Err(RelayError::DuplicateRole(RelayRole::Rendezvous));
                }
                seen_rendezvous = true;
            }
            RelayRole::Delivery => {
                if seen_delivery {
                    return Err(RelayError::DuplicateRole(RelayRole::Delivery));
                }
                seen_delivery = true;
            }
        }
        for other in &assignments[i + 1..] {
            if other.relay == a.relay {
                return Err(RelayError::SingleRelayMultipleRoles);
            }
        }
    }

    if !seen_entry {
        return Err(RelayError::MissingRole(RelayRole::Entry));
    }
    if !seen_rendezvous {
        return Err(RelayError::MissingRole(RelayRole::Rendezvous));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::Controller;

    fn did() -> Did {
        Controller::incept_single().unwrap().did()
    }

    #[test]
    fn three_distinct_relays_pass() {
        let assignments = vec![
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: did(),
            },
            DeliveryAssignment {
                role: RelayRole::Rendezvous,
                relay: did(),
            },
            DeliveryAssignment {
                role: RelayRole::Delivery,
                relay: did(),
            },
        ];
        enforce_role_separation(&assignments).unwrap();
    }

    #[test]
    fn entry_and_rendezvous_alone_pass_delivery_is_optional() {
        let assignments = vec![
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: did(),
            },
            DeliveryAssignment {
                role: RelayRole::Rendezvous,
                relay: did(),
            },
        ];
        enforce_role_separation(&assignments).unwrap();
    }

    #[test]
    fn the_same_relay_holding_two_roles_is_rejected() {
        let shared = did();
        let assignments = vec![
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: shared.clone(),
            },
            DeliveryAssignment {
                role: RelayRole::Rendezvous,
                relay: shared,
            },
        ];
        assert_eq!(
            enforce_role_separation(&assignments).unwrap_err(),
            RelayError::SingleRelayMultipleRoles
        );
    }

    #[test]
    fn the_same_relay_holding_all_three_roles_is_rejected() {
        let shared = did();
        let assignments = vec![
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: shared.clone(),
            },
            DeliveryAssignment {
                role: RelayRole::Rendezvous,
                relay: shared.clone(),
            },
            DeliveryAssignment {
                role: RelayRole::Delivery,
                relay: shared,
            },
        ];
        assert_eq!(
            enforce_role_separation(&assignments).unwrap_err(),
            RelayError::SingleRelayMultipleRoles
        );
    }

    #[test]
    fn a_missing_entry_role_is_rejected() {
        let assignments = vec![DeliveryAssignment {
            role: RelayRole::Rendezvous,
            relay: did(),
        }];
        assert_eq!(
            enforce_role_separation(&assignments).unwrap_err(),
            RelayError::MissingRole(RelayRole::Entry)
        );
    }

    #[test]
    fn a_missing_rendezvous_role_is_rejected() {
        let assignments = vec![DeliveryAssignment {
            role: RelayRole::Entry,
            relay: did(),
        }];
        assert_eq!(
            enforce_role_separation(&assignments).unwrap_err(),
            RelayError::MissingRole(RelayRole::Rendezvous)
        );
    }

    #[test]
    fn a_duplicate_entry_role_by_different_relays_is_rejected() {
        let assignments = vec![
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: did(),
            },
            DeliveryAssignment {
                role: RelayRole::Entry,
                relay: did(),
            },
            DeliveryAssignment {
                role: RelayRole::Rendezvous,
                relay: did(),
            },
        ];
        assert_eq!(
            enforce_role_separation(&assignments).unwrap_err(),
            RelayError::DuplicateRole(RelayRole::Entry)
        );
    }

    #[test]
    fn an_empty_assignment_is_rejected_for_missing_entry() {
        assert_eq!(
            enforce_role_separation(&[]).unwrap_err(),
            RelayError::MissingRole(RelayRole::Entry)
        );
    }
}

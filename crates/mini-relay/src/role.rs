//! The three separable relay roles (`MN-202`, research §5.2): entry relay
//! (knows the client's address, not the destination), rendezvous/mailbox
//! relay (knows the destination's mailbox capability, not the client's
//! address), and an optional delivery relay. **No direct user-to-user
//! connection** — every delivery crosses at least an entry and a
//! rendezvous hop. No single relay identity may hold two roles for one
//! delivery — see [`crate::role_separation`].

use crate::error::{RelayError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayRole {
    /// Knows the client's address; does not know the final destination.
    Entry,
    /// Knows the destination's mailbox capability; does not know the
    /// client's address.
    Rendezvous,
    /// An optional additional hop between rendezvous and destination.
    Delivery,
}

impl RelayRole {
    pub(crate) fn tag(self) -> u8 {
        match self {
            RelayRole::Entry => 1,
            RelayRole::Rendezvous => 2,
            RelayRole::Delivery => 3,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Result<Self> {
        match tag {
            1 => Ok(RelayRole::Entry),
            2 => Ok(RelayRole::Rendezvous),
            3 => Ok(RelayRole::Delivery),
            _ => Err(RelayError::BadRelayRole),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_round_trips_through_its_tag() {
        for role in [RelayRole::Entry, RelayRole::Rendezvous, RelayRole::Delivery] {
            assert_eq!(RelayRole::from_tag(role.tag()).unwrap(), role);
        }
    }

    #[test]
    fn an_unknown_role_tag_is_rejected() {
        assert_eq!(RelayRole::from_tag(0xee), Err(RelayError::BadRelayRole));
    }
}

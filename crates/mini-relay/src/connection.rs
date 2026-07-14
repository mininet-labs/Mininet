//! Connection-scoped ephemeral identifiers and per-role relay identities.
//!
//! Research §5.2's rule: "connection-scoped ephemeral IDs; rotate relays
//! and queues; never a global DID in transport headers." A
//! [`ConnectionId`] is fresh random per circuit/delivery — never derived
//! from, or reversible to, any `did:mini` root. A relay operator's *role
//! identity* for one delivery is a pairwise pseudonym derived fresh per
//! `(root, role, connection id)`, reusing `did-mini`'s existing SPEC-01
//! §10 `Controller::incept_pairwise_pseudonym` mechanism directly (no
//! second HKDF call site) with this module's own domain-separated
//! context — deliberately independent of `mini_objects::pseudonym`'s
//! domain tag, so a relay operator's role identity in one delivery can
//! never be linked to its identity in another delivery, or to any
//! object-authorship/capability-holder pseudonym the same root might use
//! elsewhere in this workspace.

use did_mini::Controller;

use crate::error::{RelayError, Result};
use crate::role::RelayRole;

const RELAY_IDENTITY_DOMAIN: &[u8] = b"mininet/mini-relay/relay-identity/v1";

/// A fresh, random, connection-scoped identifier. Never a `did:mini` root
/// and never derived from one — an observer who sees a `ConnectionId`
/// learns nothing about which relay operators or endpoints are involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId([u8; 16]);

impl ConnectionId {
    /// A fresh random connection id from OS entropy.
    pub fn generate() -> Result<Self> {
        let full = mini_crypto::random_32().map_err(RelayError::Crypto)?;
        let mut id = [0u8; 16];
        id.copy_from_slice(&full[..16]);
        Ok(ConnectionId(id))
    }

    pub fn to_bytes(self) -> [u8; 16] {
        self.0
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        ConnectionId(bytes)
    }
}

/// Derive a per-role, per-connection relay identity from `root`.
/// Deterministic: the same `(root, role, connection_id)` always derives
/// the same pseudonym, letting a relay operator re-derive its own
/// identity for a retry with no extra storage — while remaining
/// unlinkable across different roles, different connections, and any
/// other purpose the same root derives pseudonyms for elsewhere.
///
/// `root` must be a single-key (non-multisig) controller — the same
/// restriction `Controller::incept_pairwise_pseudonym` itself enforces.
pub fn derive_relay_identity(
    root: &Controller,
    role: RelayRole,
    connection_id: ConnectionId,
) -> Result<Controller> {
    let bytes = connection_id.to_bytes();
    let mut context = Vec::with_capacity(RELAY_IDENTITY_DOMAIN.len() + 1 + bytes.len());
    context.extend_from_slice(RELAY_IDENTITY_DOMAIN);
    context.push(role.tag());
    context.extend_from_slice(&bytes);
    Ok(root.incept_pairwise_pseudonym(&context)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_generated_connection_ids_differ() {
        let a = ConnectionId::generate().unwrap();
        let b = ConnectionId::generate().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_connection_id_round_trips_through_bytes() {
        let a = ConnectionId::generate().unwrap();
        let decoded = ConnectionId::from_bytes(a.to_bytes());
        assert_eq!(a, decoded);
    }

    fn root() -> Controller {
        Controller::incept_single().unwrap()
    }

    #[test]
    fn the_same_root_role_and_connection_derive_the_same_identity() {
        let r = root();
        let c = ConnectionId::generate().unwrap();
        let a = derive_relay_identity(&r, RelayRole::Entry, c).unwrap();
        let b = derive_relay_identity(&r, RelayRole::Entry, c).unwrap();
        assert_eq!(a.did(), b.did());
    }

    #[test]
    fn different_roles_derive_different_identities() {
        let r = root();
        let c = ConnectionId::generate().unwrap();
        let a = derive_relay_identity(&r, RelayRole::Entry, c).unwrap();
        let b = derive_relay_identity(&r, RelayRole::Rendezvous, c).unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn different_connections_derive_different_identities() {
        let r = root();
        let c1 = ConnectionId::generate().unwrap();
        let c2 = ConnectionId::generate().unwrap();
        let a = derive_relay_identity(&r, RelayRole::Entry, c1).unwrap();
        let b = derive_relay_identity(&r, RelayRole::Entry, c2).unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn different_roots_derive_different_identities_for_the_same_role_and_connection() {
        let c = ConnectionId::generate().unwrap();
        let a = derive_relay_identity(&root(), RelayRole::Entry, c).unwrap();
        let b = derive_relay_identity(&root(), RelayRole::Entry, c).unwrap();
        assert_ne!(a.did(), b.did());
    }

    #[test]
    fn a_derived_relay_identity_can_sign_and_be_independently_verified() {
        let r = root();
        let c = ConnectionId::generate().unwrap();
        let identity = derive_relay_identity(&r, RelayRole::Delivery, c).unwrap();
        let sigs = identity.sign_message(b"hop payload");
        identity
            .kel()
            .verify_message(b"hop payload", &sigs)
            .unwrap();
    }
}

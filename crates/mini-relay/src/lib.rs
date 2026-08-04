//! Tier 1 relay + rendezvous protocol (D-0306, lane L6 of `docs/design/
//! privacy-cost-doctrine-parallel-execution-plan.md`, closes tracking
//! issue #144 / `MN-202`).
//!
//! Research (`docs/research/MININET_RESEARCH_V2_20260713.md` §5.2): three
//! separable roles — entry relay (knows the client's address, not the
//! destination), rendezvous/mailbox relay (knows the destination's
//! mailbox capability, not the client's address), and an optional
//! delivery relay. **No direct user-to-user connection.** Rules this
//! crate implements: connection-scoped ephemeral IDs ([`ConnectionId`]);
//! rotating mailbox capabilities ([`MailboxGrant`], rotated by issuing a
//! fresh grant, not a dedicated API); never a global `did:mini` root in
//! transport headers (relay identities are pairwise pseudonyms, see
//! [`derive_relay_identity`]); role separation so no single relay
//! provider holds two roles for one delivery ([`enforce_role_separation`]).
//!
//! ## What's real here, and what isn't
//!
//! The capability/pseudonym/envelope machinery in this crate is real,
//! tested Rust, composing only already-reviewed primitives from
//! `mini-crypto` (via `did-mini` and `mini-bearer` — no new cryptography).
//! [`build_onion`] adds the concrete `PrivacyTier::Relayed` execution path:
//! exactly three independently encrypted layers (`Entry -> Rendezvous ->
//! Delivery`) around a destination-encrypted fixed-size payload. Each relay
//! learns only its own role, one opaque next-hop token, and the next ciphertext;
//! no relay receives the application plaintext or both endpoints. This compact
//! Mininet onion is deliberately **not** called Sphinx and does not claim global
//! traffic-analysis resistance. Mixed/Burst remain a separate Sphinx/Loopix
//! implementation gated behind independent review.
//!
//! What remains outside this crate: discovery and authenticated endpoint
//! advertisements (`mini-transport-security`), NAT traversal, reconnect,
//! background service supervision, and the externally reviewed mix executor.
//! `MN-208` (restricting `mini-net` DHT lookups) also remains separate:
//! `mini-net` has no DHT value-storage layer yet to restrict.
//!
//! [`plan::roles_for_route_decision`] bridges `mini_transport_policy::
//! route`'s decision output to this crate's role planning — closing the
//! "two disconnected layers" gap D-0306 flagged as required follow-up.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod connection;
mod envelope;
mod error;
mod mailbox;
mod onion;
mod plan;
mod role;
mod role_separation;

pub use connection::{derive_relay_identity, ConnectionId};
pub use envelope::{RelayEnvelope, ENVELOPE_VERSION};
pub use error::{RelayError, Result};
pub use mailbox::{
    MailboxGrant, MailboxId, MailboxToken, MailboxTokenCommitment, MAILBOX_GRANT_VERSION,
};
pub use onion::{
    build_onion, open_onion_destination, OnionForward, OnionHop, OnionPacket, OnionReplayCache,
    PeeledOnion, LARGE_ONION_PAYLOAD_BYTES, MAX_ONION_NEXT_HOP_BYTES, MAX_ONION_REPLAY_ENTRIES,
    MEDIUM_ONION_PAYLOAD_BYTES, ONION_HOP_COUNT, ONION_VERSION, SMALL_ONION_PAYLOAD_BYTES,
};
pub use plan::roles_for_route_decision;
pub use role::RelayRole;
pub use role_separation::{enforce_role_separation, DeliveryAssignment};

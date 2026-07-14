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
//! **What this crate does not do**: dial a socket, run a live multi-hop
//! relay over real network connections, or provide NAT traversal/address
//! discovery — `RelayEnvelope::seal`/`open` are proven in-process against
//! paired `mini_bearer::Channel`s in this crate's own tests, the same way
//! `mini-bearer`'s own channel tests work, but a live multi-process relay
//! demo (like `mini-net`'s gossip demo) is explicitly future work, not
//! this lane. `MN-208` (restricting `mini-net` DHT lookups) is out of
//! scope entirely: `mini-net` has no DHT value-storage layer yet to
//! restrict — see tracking issue #144 for why that's a separate future
//! lane, not silently dropped here.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod connection;
mod envelope;
mod error;
mod mailbox;
mod role;
mod role_separation;

pub use connection::{derive_relay_identity, ConnectionId};
pub use envelope::{RelayEnvelope, ENVELOPE_VERSION};
pub use error::{RelayError, Result};
pub use mailbox::{
    MailboxGrant, MailboxId, MailboxToken, MailboxTokenCommitment, MAILBOX_GRANT_VERSION,
};
pub use role::RelayRole;
pub use role_separation::{enforce_role_separation, DeliveryAssignment};

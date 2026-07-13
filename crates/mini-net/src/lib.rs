//! Wide-area peer discovery and gossip broadcast for Mininet.
//!
//! `mini-bearer` handles local, identity-free bearers (BLE / local Wi-Fi /
//! relay — D-0009/D-0015); this crate is the layer D-0009 anticipated on
//! top of it for the wider network — finding peers beyond direct proximity
//! and propagating messages across them. It borrows two proven designs
//! rather than depending on the projects that popularized them (D-0034
//! point 3, "adapt the design, not the dependency" — the same stance
//! D-0008/D-0009 take toward every other adopted piece in this tree):
//!
//! - [`routing::RoutingTable`] — a Kademlia-style bucketed routing table:
//!   peers are stored by how many leading bits they share with the local
//!   id, giving O(log n) lookups without any node holding a full peer list.
//! - [`gossip::GossipRouter`] — dedup-flooding broadcast: forward a message
//!   the first time it's seen, drop it on every repeat, the same shape
//!   gossipsub's message cache uses.
//!
//! [`peer::PeerId`] is a **transport-routing** identifier only, generated
//! fresh per session — see that type's docs for why it must never be
//! treated as a stable identity.
//!
//! [`pex`] adds the piece neither of the above carries: a dialable address
//! for a known peer, plus the minimal request/response wire message two
//! peers use to hand each other `(PeerId, SocketAddr)` pairs with no
//! central directory server (founder review P1 backlog item "invitation
//! and peer-exchange discovery").
//!
//! ## Honest limits
//!
//! This crate is the routing/broadcast *logic*, not yet a running network
//! stack: real transport (which bearer carries these messages to the wider
//! internet), bucket-refresh-by-liveness-ping, and randomized gossip fanout
//! are `pending` and documented at each type below. The algorithms here are
//! deterministic and fully unit-tested without any socket, matching this
//! workspace's pattern of landing pure, testable logic before the adapter
//! that needs a real device/network to exercise (`mini-presence`'s RTT hook,
//! `mini-bearer`'s BLE adapter — D-0015/D-0016 — took the same shape).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod gossip;
mod peer;
mod pex;
mod routing;

pub use error::{NetError, Result};
pub use gossip::{fanout_peers, GossipRouter};
pub use peer::PeerId;
pub use pex::{
    absorb_response, build_response, AddressBook, PeerRecord, PexMessage, MAX_PEX_RECORDS,
};
pub use routing::{RoutingTable, BUCKET_SIZE};

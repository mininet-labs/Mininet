//! Bounded, authenticated real-transport delivery of `mini-search-federation`'s
//! F1 (signed crawl observations) and F2 (signed index segments) between two
//! Mininet peers over any [`mini_bearer::Bearer`]/[`mini_bearer::Channel`].
//!
//! This is the Track F "required follow-up" named in
//! `docs/design/federated-search-exchange-f1-f2.md`: "Wire F1/F2 objects
//! through a bounded real transport with authenticated peer behavior and
//! source-count limits." It does not reinvent a transport or a trust
//! boundary — the actual object delivery and verification is
//! `mini_sync::request_retrieval`/`serve_retrieval`, unmodified, over the
//! exact same `Channel` abstraction `mini-sync`/`mini-relay` already use.
//! What this crate adds on top:
//!
//! 1. A tiny bounded advertisement exchange ([`message`], private wire
//!    format) so a client learns *which* ids to ask `mini-sync` to retrieve.
//!    No query terms, ranking profile, or free text of any kind crosses the
//!    wire here — a peer states which F1/F2 object ids it holds, nothing
//!    more. Sending a caller's search query to a remote peer for
//!    server-side evaluation is Track F6 (private query transport),
//!    explicitly undesigned and out of scope; this crate never does that.
//! 2. A federation-specific post-check ([`pull_source`]) on top of
//!    `mini-sync`'s already-verified ingest: every returned object must
//!    decode as F1/F2, and, when the caller names an expected provider,
//!    must actually be authored by that identity. `mini-sync` proves an
//!    object is validly signed by *some* real identity; this crate is what
//!    makes "this session's objects, from this provider" a checked claim
//!    rather than an assumption a relaying/confused peer could violate.
//! 3. A source-count bound across a multi-peer session ([`pull_from_sources`]).
//!
//! ## What this deliberately does NOT do
//!
//! - No automatic feed into [`mini_search_federation::federate_query`].
//!   `federate_query` needs each source's `Corpus`/`DocumentContextTable` in
//!   addition to its `IndexSegment`, and nothing in this workspace yet
//!   defines a signed, transmittable form for those — wiring pulled F2
//!   objects all the way into a live federated query is separate, later
//!   work, not silently assumed here.
//! - No peer discovery, connection setup, or bootstrap. Callers dial and
//!   handshake exactly as any other `mini_bearer`/`mini_sync` caller does;
//!   this crate only ever takes an already-established `Bearer`/`Channel`.
//! - No scheduling, refresh policy, or persistence of which peers to pull
//!   from next.
//! - No fault isolation across peers in one session — see [`multi`]'s doc
//!   comment.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod message;
mod multi;
mod session;

pub use error::{NetError, Result};
pub use multi::{pull_from_sources, FederationPullReport, PeerSource};
pub use session::{pull_source, serve_source, SourcePullReport};

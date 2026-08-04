//! Bounded, authenticated real-transport delivery of `mini-search-federation`'s
//! F1 (signed crawl observations), F2 (signed index segments), and F2b
//! (signed corpus/context bundles) between two Mininet peers over any
//! [`mini_bearer::Bearer`]/[`mini_bearer::Channel`].
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
//!    wire in this exchange — a peer states which object ids it holds,
//!    nothing more.
//! 2. [`remote_query`]/[`serve_query`] (Track F6 Phase 1, [`query`]) are
//!    the deliberate, separately-scoped exception: a bounded query string
//!    *does* cross the wire here, confidential-in-transit only — the
//!    queried peer sees it in full. This is not a private-information-
//!    retrieval scheme; see `docs/design/f6-private-query-transport.md`
//!    for the full doctrine on what this is and is not.
//! 3. A federation-specific post-check ([`pull_source`]) on top of
//!    `mini-sync`'s already-verified ingest: every returned object must
//!    decode as F1/F2/F2b, and, when the caller names an expected provider,
//!    must actually be authored by that identity. `mini-sync` proves an
//!    object is validly signed by *some* real identity; this crate is what
//!    makes "this session's objects, from this provider" a checked claim
//!    rather than an assumption a relaying/confused peer could violate.
//! 4. A source-count bound across a multi-peer session ([`pull_from_sources`]).
//! 5. [`assemble_federation_source`], which turns a
//!    [`SourcePullReport::trusted`] id set into a real, owned
//!    [`OwnedFederationSource`] ready for
//!    [`mini_search_federation::federate_query`] — pairing a pulled F2
//!    [`mini_lexical_index::IndexSegment`] with the F2b corpus bundle that
//!    declares the same [`mini_web_types::IndexSegmentId`], and rebuilding
//!    a fresh `Corpus`/`DocumentContextTable` from the bundle's declared
//!    entries. Before F2b (`mini_search_federation`'s
//!    `publish_corpus_bundle`/`read_corpus_bundle`) existed, a real
//!    federated query could only ever run over in-process data; this
//!    closes that gap.
//!
//! ## What this deliberately does NOT do
//!
//! - No peer discovery, connection setup, or bootstrap. Callers dial and
//!   handshake exactly as any other `mini_bearer`/`mini_sync` caller does;
//!   this crate only ever takes an already-established `Bearer`/`Channel`.
//! - No scheduling, refresh policy, or persistence of which peers to pull
//!   from next.
//! - No fault isolation across peers in one session — see [`multi`]'s doc
//!   comment.
//! - [`assemble_federation_source`] handles one segment's worth of data at
//!   a time and requires the caller to already know which pulled ids belong
//!   to which provider/segment (typically one [`pull_source`] call's own
//!   `trusted` set); it does not scan an entire multi-provider store to
//!   auto-discover segment/bundle pairs.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod assemble;
mod error;
mod message;
mod multi;
mod query;
mod remote_merge;
mod session;

pub use assemble::{assemble_federation_source, OwnedFederationSource};
pub use error::{NetError, Result};
pub use multi::{pull_from_sources, FederationPullReport, PeerSource};
pub use query::{
    authenticated_provider_pseudonym, remote_query, remote_query_authenticated, serve_query,
    serve_query_authenticated, AuthenticatedQueryResults, WireResult, MAX_QUERY_RESULTS,
    MAX_QUERY_TEXT_BYTES,
};
pub use remote_merge::{
    federated_result_from_wire, merge_authenticated_remote_results, merge_remote_results,
};
pub use session::{pull_source, serve_source, SourcePullReport};

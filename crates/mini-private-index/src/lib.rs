//! MN-208 private-lookup boundary (D-0310, `docs/research/
//! MN208_PRIVATE_LOOKUP_DHT_RESEARCH_20260714.md`).
//!
//! The research report's executive conclusion is explicit: MN-208 should
//! not begin by building a general-purpose value DHT and adding privacy
//! around it afterward — `mini-net` has no provider-record or value-
//! storage DHT to restrict yet (confirmed independently by `mini-relay`'s
//! own D-0306 scoping). The correct MN-208 outcome is a design doctrine
//! and a narrowly scoped private-index protocol: public network routing
//! may use public peer-discovery data, but private object discovery must
//! never require broadcasting a sensitive object identifier, capability,
//! mailbox address, subscription, or content interest to a public DHT.
//!
//! ## What's implemented here (Phase 0 doctrine + Phase 1 primitive)
//!
//! - [`LookupPrivacyClass`]: the report's frozen five-tier taxonomy
//!   (`Public` -> `CapabilityScoped` -> `PrivateProxied` ->
//!   `PrivateBundled` -> `PrivatePIR`), so policy code has a typed
//!   vocabulary instead of caller judgment. Only `CapabilityScoped`'s
//!   primitive is implemented.
//! - [`derive_lookup_label`]/[`LookupPurpose`]/[`IndexEpoch`]/
//!   [`CapabilitySecret`]: capability-derived rotating lookup labels via
//!   HKDF-SHA256 (`mini-crypto`'s existing, already-reviewed KDF suite —
//!   no new cryptography) — the opaque handle a client sends to an index
//!   service instead of a plaintext object ID.
//! - [`PrivateIndexRecord`]/[`RecordSizeClass`]: a signed, fixed-size-
//!   class record a writer publishes at one [`LookupLabel`]. The
//!   encrypted payload is opaque to this crate — content encryption is
//!   the caller's job.
//! - [`LocalIndex`]: a local, in-memory store enforcing signature
//!   validity, writer-cannot-be-hijacked, and monotonic sequence
//!   (rollback rejection) — the per-replica discipline a networked
//!   private index would need, exercised here without a network.
//!
//! ## What's deliberately NOT implemented — "No network yet"
//!
//! There is no wire protocol, no replicated index service, no relay-
//! based role separation (OHTTP-style query proxying), no query
//! batching/decoys, no caching/prefetch layer, and no Private
//! Information Retrieval. PIR in particular is explicitly gated behind
//! external cryptographic review (CLAUDE.md's no-new-cryptography rule,
//! D-0047) — nothing in this crate may be described as providing PIR.
//! See `docs/design/private-lookup-and-dht-boundary.md` for the full
//! phase sequence and what each later phase still needs.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod label;
mod local_index;
mod lookup_class;
mod record;

pub use error::{IndexError, Result};
pub use label::{
    derive_lookup_label, CapabilitySecret, IndexEpoch, LookupLabel, LookupPurpose,
};
pub use local_index::LocalIndex;
pub use lookup_class::LookupPrivacyClass;
pub use record::{PrivateIndexRecord, RecordSizeClass, RECORD_VERSION};

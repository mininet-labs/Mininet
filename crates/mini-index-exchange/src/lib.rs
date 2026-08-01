//! MiniSearch index-segment exchange (Track F2 of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! §29 — "Content-addressed index segments: publish and verify immutable
//! index segments").
//!
//! ## What's implemented here
//!
//! [`SegmentPublication`]: a provider's signature over an
//! [`mini_lexical_index::IndexManifest`], plus the verification that lets
//! any node accept a published segment from untrusted bytes.
//! [`SegmentPublication::publish`] signs; [`SegmentPublication::verify`]
//! checks the signature and names the provider;
//! [`SegmentPublication::verify_segment`] additionally checks the segment's
//! bytes against the published content address; and
//! [`accept_published_segment`] is the full receive path — decode untrusted
//! segment and publication bytes, verify both, return the validated segment
//! and its provider or an error.
//!
//! ## The trust model
//!
//! Acceptance rests on two independent checks, both required:
//!
//! - **content address** — the segment's re-derived BLAKE3 `segment_id`
//!   must equal the published id, so a provider cannot attach an id to bytes
//!   it did not produce; and
//! - **signature** — the manifest is signed, so a third party cannot forge
//!   a publication in a provider's name, and the provider's
//!   [`mini_web_types::ProviderPseudonym`] is derived from the verifying key.
//!
//! "Provider P published exactly this segment" is therefore verifiable from
//! bytes alone, with no trusted registry. That is the mechanism behind
//! D-0312's plurality: many providers publish index segments built from the
//! same crawl observations, and anyone caches, replicates, and compares them
//! by id without trusting whoever sent them.
//!
//! ## What's deliberately NOT here
//!
//! No network or transport (that is Track F6's private query transport and
//! the existing `mini-bearer`/`mini-sync` layers). No storage. No federated
//! query merging (F3) or local re-ranking (F4). No provider payments (F5) —
//! and, per Directive 16, a publication carries no balance, stake, weight,
//! or ranking entitlement of any kind: it attests *provenance*, never worth.
//! No new cryptography (Directive 14, D-0421): signing and verification are
//! `mini-crypto`'s existing Ed25519/ML-DSA-65 primitives, unchanged.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod publication;

pub use error::{ExchangeError, Result};
pub use publication::{
    accept_published_segment, provider_pseudonym, SegmentPublication, VerifiedPublication,
};

// Re-exported so callers name providers and segments with the same
// vocabulary the rest of MiniSearch uses.
pub use mini_web_types::ProviderPseudonym;

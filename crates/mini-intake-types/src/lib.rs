//! Mininet Intake shared vocabulary (Track B1 of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! founder direction 2026-07-18; D-0311/D-0312 record the adjacent
//! public-commons and open-web-search decisions this track feeds).
//!
//! This is the native, clean-room boundary through which external
//! documents will eventually enter Mininet — designed independently
//! from scratch, with no code or dependency taken from any external
//! licensed intake tool (the research document's own non-negotiable
//! §2.1 rule).
//!
//! ## What's implemented here
//!
//! Pure types and a deterministic, length-prefixed wire codec (the same
//! discipline as `mini-relay`/`mini-bridge`/`mini-private-index`):
//! [`IntakeId`] (wrapping `mini_crypto`'s existing [`mini_crypto::Multihash`]
//! — no new cryptography), [`SourceRecord`], [`DerivedRepresentation`]/
//! [`DerivationRecord`]/[`GeneratorIdentity`]/[`RepresentationKind`],
//! [`AuthorityClass`] (frozen, ordered, six-tier taxonomy), [`ReviewState`]
//! (a state machine with [`ReviewState::allows_transition_to`] naming every
//! legal transition), [`IntakeLink`], [`IntakeWarning`], and
//! [`IntakeEnvelope`] tying them together.
//!
//! The crate's core rule — "derived text is not the source, automated
//! classification is not judgment, and imported material receives no
//! project authority merely because Mininet can parse it" (research
//! report §3.2) — is enforced structurally, not just documented:
//! [`IntakeEnvelope::new`] always starts at [`ReviewState::Unreviewed`]
//! and [`AuthorityClass::UntrustedExternal`], and the only way forward is
//! [`IntakeEnvelope::advance_review_state`] (rejects illegal transitions,
//! including any transition out of a terminal `Rejected`/`Superseded`
//! state) and [`IntakeEnvelope::promote_authority`] (rejects reaching
//! [`AuthorityClass::ReviewedEvidence`] or higher unless the review state
//! is already [`ReviewState::Accepted`]).
//!
//! ## What's deliberately NOT implemented
//!
//! No parser, filesystem watcher, network client, or AI model — this
//! crate is vocabulary only, per the research report's own explicit
//! scope for `mini-intake-types`. No hashing: [`IntakeId`]/[`SourceRecord`]
//! carry a [`mini_crypto::Multihash`] a caller already computed; deciding
//! *what* gets hashed is an orchestration concern for the trusted intake
//! coordinator (`mini-intake`, Track B2), not a vocabulary concern. No
//! storage of the represented bytes themselves. No extractor protocol or
//! sandboxing (Track B3). See `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! Part V for the full Track B PR sequence this crate is the first slice
//! of.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod authority;
mod codec;
mod envelope;
mod error;
mod ids;
mod link;
mod media;
mod representation;
mod review;
mod source;
mod warning;

pub use authority::AuthorityClass;
pub use envelope::{IntakeEnvelope, ENVELOPE_VERSION};
pub use error::{IntakeError, Result};
pub use ids::IntakeId;
pub use link::IntakeLink;
pub use media::MediaType;
pub use representation::{
    DerivationRecord, DerivedRepresentation, GeneratorIdentity, RepresentationKind,
};
pub use review::ReviewState;
pub use source::SourceRecord;
pub use warning::IntakeWarning;

//! Bridges an `Accepted` `mini-intake` [`IntakeEnvelope`] to a real, signed
//! `mini-social` [`Post`], closing the piece Track B5's own docs already
//! named as still missing: `IntakeEnvelope::add_link` (D-0360) gates
//! attaching an [`IntakeLink`] behind `ReviewState::Accepted`, but nothing
//! in this workspace previously called it against a real target crate —
//! `mini-intake`'s own module docs say so explicitly ("no publication
//! linking (Track B5)"). This is that caller, composing already-shipped
//! pieces rather than inventing a new authority path.
//!
//! ## What this crate does
//!
//! [`publish_accepted_intake_as_post`] takes an already-`Accepted`
//! [`IntakeEnvelope`] whose declared media type is `TextPlain`/`Markdown`
//! (the only two kinds `mini-intake`'s Track B2 coordinator ever stores),
//! reads its immutable source bytes back via
//! [`mini_intake::read_verified_source_bytes`] — re-verified against the
//! envelope's declared digest/length/intake id, not merely fetched by
//! digest key, since a content-addressed backend can "repair" a blob to
//! different bytes under the same key (see that function's docs) — and
//! publishes them as a bounded [`mini_social::Post`] via
//! [`mini_social::build_post`]/[`mini_social::publish_post`] — the same
//! length-bound-before-sign discipline `mini-social`'s other object types
//! already have. It returns both the produced [`mini_objects::Object`] and
//! the matching [`IntakeLink::Post`] target, derived from the post's own
//! content id, not invented separately. [`build_accepted_intake_post`]
//! exposes the signing step alone (no store insert) for a caller that
//! needs to durably persist the exact signed bytes before committing them
//! — `mini-cli`'s crash-recoverable publish journal is exactly such a
//! caller; see that function's docs.
//!
//! ## What this crate deliberately does not do
//!
//! - **Does not call `envelope.add_link` itself.** That mutation (and
//!   persisting it back via `mini_intake::save_envelope`) stays the
//!   caller's own explicit step, exactly as `mini-intake`'s own coordinator
//!   never advances review state or authority on a caller's behalf. This
//!   crate only produces the [`IntakeLink`] value; attaching it is a
//!   separate, visible decision.
//! - **Does not promote [`mini_intake_types::AuthorityClass`].** A linked
//!   post is still whatever authority class the envelope already carries —
//!   this crate grants no new authority and reads no vote/governance state.
//! - **No new cryptography.** The post's signature is `mini-social`'s own
//!   existing `did-mini` KEL signing; the `IntakeLink::Post` target is a
//!   plain multihash decode of the post's own content id, the same
//!   operation `mini_objects::ObjectId::parse` already performs internally.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;

pub use error::{IntakeSocialError, Result};

use did_mini::{Controller, Did};
use mini_crypto::Multihash;
use mini_intake_types::{IntakeEnvelope, IntakeLink, MediaType, ReviewState};
use mini_objects::Object;
use mini_store::{Backend, Store};

/// Publish `envelope`'s text/Markdown source as a real, signed
/// [`mini_social::Post`]. Fails unless `envelope.review_state()` is already
/// [`ReviewState::Accepted`] — the same review-before-recognition gate
/// [`IntakeEnvelope::add_link`] itself enforces, checked here too so a
/// caller cannot skip it by never calling `add_link`.
///
/// `intake_backend` is whatever [`mini_store::Backend`] `mini-intake`
/// stored the source bytes in; `social_store` is the (possibly different)
/// [`mini_store::Store`] the resulting post is inserted into — the same
/// two-storage-layer split `mini-intake`'s own docs already justify
/// ("intake material has no signature at ingest time; `Store` assumes
/// self-certifying signed objects").
pub fn publish_accepted_intake_as_post<IB, SB>(
    intake_backend: &IB,
    social_store: &mut Store<SB>,
    human: &Did,
    device: &Controller,
    envelope: &IntakeEnvelope,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<(Object, IntakeLink)>
where
    IB: Backend,
    SB: Backend,
{
    let post = build_accepted_intake_post(
        intake_backend,
        human,
        device,
        envelope,
        timestamp_ms,
        sequence,
    )?;
    social_store
        .insert(&post)
        .map_err(IntakeSocialError::Store)?;
    let link = intake_link_for_post(&post)?;
    Ok((post, link))
}

/// The signing half of [`publish_accepted_intake_as_post`], split out so a
/// caller that needs to durably persist the exact signed bytes *before*
/// inserting them anywhere (e.g. `mini-cli`'s crash-recoverable publish
/// journal — a retry after a crash must reuse this exact signature rather
/// than allocate a new sequence/timestamp and sign a second, distinct,
/// still-feed-eligible post) has one real path to do that. Performs the
/// exact same `Accepted`/media-type/source-integrity checks
/// [`publish_accepted_intake_as_post`] does; the caller is responsible for
/// inserting the returned object into a store and attaching the matching
/// [`intake_link_for_post`] itself.
pub fn build_accepted_intake_post<IB: Backend>(
    intake_backend: &IB,
    human: &Did,
    device: &Controller,
    envelope: &IntakeEnvelope,
    timestamp_ms: u64,
    sequence: u64,
) -> Result<Object> {
    if envelope.review_state() != ReviewState::Accepted {
        return Err(IntakeSocialError::NotAccepted);
    }
    match envelope.source.media_type {
        MediaType::TextPlain | MediaType::Markdown => {}
        _ => return Err(IntakeSocialError::UnsupportedMediaType),
    }

    let bytes = mini_intake::read_verified_source_bytes(intake_backend, envelope)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| IntakeSocialError::NotUtf8)?;

    Ok(mini_social::build_post(
        human,
        device,
        text,
        timestamp_ms,
        sequence,
    )?)
}

/// Decode an already-published post's content id back into the
/// [`Multihash`] an [`IntakeLink::Post`] carries — the same decode
/// [`mini_objects::ObjectId::parse`] performs internally, not a second
/// derivation of the id. Public so a caller resuming a journaled,
/// already-signed post (see [`build_accepted_intake_post`]) can derive the
/// same link this crate would have produced, without a second, divergent
/// derivation.
pub fn intake_link_for_post(post: &Object) -> Result<IntakeLink> {
    let bytes = mini_crypto::encoding::decode(post.id().as_str())?;
    let digest = Multihash::from_bytes(&bytes)?;
    Ok(IntakeLink::Post(digest))
}

// Re-exported only so downstream callers do not need a direct `mini-social`
// dependency merely to name the type this crate's own bound composes with.
pub use mini_social::MAX_POST_BYTES;

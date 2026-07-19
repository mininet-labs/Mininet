//! Trusted Mininet Intake coordinator (Track B2 of `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! founder direction 2026-07-18). Builds on `mini-intake-types`'s (Track
//! B1, D-0313) pure vocabulary by actually driving it: hashing real
//! bytes, storing them immutably, deduplicating on content, and creating
//! `IntakeEnvelope`s — all without ever promoting or demoting an
//! envelope's review state or authority class on its own.
//!
//! ## What's implemented here
//!
//! [`intake_local_file`]: reads one local text/Markdown file, computes its
//! `BLAKE3` digest (`mini_crypto`, no new cryptography), stores the raw
//! immutable bytes plus a fresh `Unreviewed`/`UntrustedExternal`
//! `IntakeEnvelope` — or, on a dedup hit (byte-identical content already
//! intaken), returns the *existing* envelope untouched, so a caller that
//! already advanced review state never gets silently reset. [`load_envelope`]/
//! [`read_source_bytes`] read back what's stored. [`save_envelope`] persists
//! a caller-mutated envelope (e.g. after `advance_review_state`/
//! `promote_authority`) — this crate never calls it on a caller's behalf.
//!
//! Storage composes `mini_store::Backend` (`MemoryBackend`/`FsBackend`) —
//! the plain content-addressed blob/meta abstraction — rather than
//! `mini_store::Store`/`mini_objects::Object`, since intake material has
//! no `did:mini` signature at ingest time; `Store` assumes self-certifying
//! signed objects, which raw external bytes are not.
//!
//! ## What's deliberately NOT implemented
//!
//! No extractor, no PDF/HTML/other binary format support (Track B3/B4:
//! unsupported extensions are a hard `UnsupportedMediaType` error, never a
//! guess), no network client, no AI model, no publication linking (Track
//! B5), and no automatic review/authority advancement — that is always a
//! separate, explicit, later call this crate does not make itself.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod coordinator;
mod error;
mod media;

pub use coordinator::{intake_local_file, load_envelope, read_source_bytes, save_envelope};
pub use error::{IntakeCoordError, Result};
pub use media::detect_media_type;

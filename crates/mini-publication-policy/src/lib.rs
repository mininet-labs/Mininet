//! Protected-publishing profile and achieved-result receipt (D-0364;
//! founder research `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! Track D1/D2, §27).
//!
//! [`PublicationProfile`] turns "visibility, attribution, transport, and
//! persistence as independent choices" (Track D1) into a typed value with
//! no cross-field validation of its own -- see [`profile`]'s module doc
//! for why. [`achieved_result_receipt_for`] (Track D2) is the one
//! connecting function: it routes a profile's chosen transport tier
//! through `mini-transport-policy`'s existing fail-closed property check
//! and prices it through `mini-resource-pricing`'s existing quote engine,
//! producing an [`AchievedResultReceipt`].
//!
//! ## What this crate is not
//!
//! It is pure policy vocabulary plus one typed crossing point into
//! already-existing privacy/transport/pricing vocabulary -- still no
//! object store, relay, mixnet, or payment mechanism of its own. Tracks
//! D3-D6 (source-hiding publication path, mixed transport, suppression-
//! resistant replication, unlinkable settlement) describe systems this
//! crate does not build; each needs its own later, separately-scoped
//! work against `mini-relay`/`mini-bridge`, `mini-erasure`/`mini-porep`,
//! and a real settlement layer respectively.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod profile;
mod receipt;

pub use error::{PublicationPolicyError, Result};
pub use profile::{Attribution, Persistence, PublicationProfile, Visibility};
pub use receipt::{achieved_result_receipt_for, AchievedResultReceipt};

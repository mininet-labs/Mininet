//! Protected-publishing profile, achieved-result receipt, and
//! source-hiding path planning (D-0364/D-0365; founder research
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! Track D1/D2/D3, §27).
//!
//! [`PublicationProfile`] turns "visibility, attribution, transport, and
//! persistence as independent choices" (Track D1) into a typed value with
//! no cross-field validation of its own -- see [`profile`]'s module doc
//! for why. [`achieved_result_receipt_for`] (Track D2) routes a profile's
//! chosen transport tier through `mini-transport-policy`'s existing
//! fail-closed property check and prices it through `mini-resource-
//! pricing`'s existing quote engine, producing an [`AchievedResultReceipt`].
//! [`source_hiding_publication_path_for`] (Track D3) plans the
//! `mini-relay` roles a source-hidden publication needs, over the same
//! `route` call -- see [`source_hiding`]'s module doc for why it is
//! deliberately not gated on [`Attribution`].
//!
//! ## What this crate is not
//!
//! It is pure policy vocabulary plus typed crossing points into
//! already-existing privacy/transport/pricing/relay vocabulary -- still
//! no object store, live relay connection, mixnet, or payment mechanism
//! of its own. Tracks D4-D6 (mixed transport, suppression-resistant
//! replication, unlinkable settlement) describe systems this crate does
//! not build; each needs its own later, separately-scoped work against
//! `mini-erasure`/`mini-porep` and a real settlement layer.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod profile;
mod receipt;
mod source_hiding;

pub use error::{PublicationPolicyError, Result};
pub use profile::{Attribution, Persistence, PublicationProfile, Visibility};
pub use receipt::{achieved_result_receipt_for, AchievedResultReceipt};
pub use source_hiding::{source_hiding_publication_path_for, SourceHidingPublicationPath};

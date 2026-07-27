//! Public-commons entitlement policy (D-0361; founder research
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! Track C1, §7-8).
//!
//! The research doctrine's central claim is that speech, reading, public
//! discovery, and ordinary social participation must never be paywalled:
//! money may purchase measurable service capacity (privacy tiers, relay,
//! suppression-resistant replication -- see `mini-privacy-policy`) but never
//! governance weight, legitimacy, speech rights, personhood, or control over
//! another person. [`PublicCommonsPolicy`] turns that doctrine into a typed,
//! testable value: every entitlement it grants is a [`Entitlement::FreeProtocolRight`],
//! never a zero-price commercial purchase, and [`commons_policy_for`] proves
//! by construction that no wallet balance or governance weight can change
//! the result.
//!
//! ## What this crate is not
//!
//! It is pure policy vocabulary: no wallet, pricing, provider settlement,
//! search index, or ranking mechanism is implemented here. Two of the
//! research doctrine's five required test properties -- that paid providers
//! cannot suppress unpaid public objects at the protocol level, and that
//! paid protection status does not automatically improve organic ranking --
//! describe systems this crate does not build (a provider/settlement layer
//! and a search-ranking layer, both still backlog items: Track C4, Track E).
//! Proving those two properties needs an integration test written once those
//! crates exist; this crate can only guarantee, today, that its own
//! entitlement policy never consults a balance, a governance weight, or a
//! paid-provider/ranking signal, because no function here accepts one as
//! anything but an ignored, explicitly-typed input.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod policy;

pub use error::{CommonsPolicyError, Result};
pub use policy::{commons_policy_for, Entitlement, PublicCommonsPolicy, WalletStanding};

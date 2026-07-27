//! Public-commons entitlement policy (D-0361/D-0362/D-0363; founder research
//! `docs/research/MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`,
//! Track C1/C2/C4, §7-8/§26).
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
//! It is pure policy vocabulary plus one typed crossing point
//! ([`service_quote_for`]) into `mini-resource-pricing`'s existing tier
//! quotes -- still no wallet, provider settlement, search index, or
//! ranking mechanism of its own. Two of the research doctrine's five
//! required test properties -- that paid providers cannot suppress unpaid
//! public objects at the protocol level, and that paid protection status
//! does not automatically improve organic ranking -- describe systems this
//! crate does not build (a provider/settlement layer and a search-ranking
//! layer, both still backlog items). [`service_quote_for`] proves the
//! narrower, adjacent claim Track C4 actually asks for ("only additional
//! external service is quoted and settled" -- `Entitlement::FreeProtocolRight`
//! is never chargeable, and a paid tier's price never depends on
//! entitlement status); it is a necessary structural precondition for the
//! two deferred properties, not a proof of either. Proving those still
//! needs an integration test written once a real provider/settlement or
//! search-ranking crate exists.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod boundary;
mod budget;
mod error;
mod policy;

pub use boundary::service_quote_for;
pub use budget::{BatteryPolicy, ContributionBudget, CpuPercent, NetworkPolicy};
pub use error::{CommonsPolicyError, Result};
pub use policy::{commons_policy_for, Entitlement, PublicCommonsPolicy, WalletStanding};

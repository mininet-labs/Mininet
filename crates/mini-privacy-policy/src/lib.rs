//! Typed cost-doctrine vocabulary and the tier 0-3 privacy policy object
//! (D-0094; founder research `docs/research/MININET_RESEARCH_V2_20260713.md`,
//! phase P1 of the parallel-contributor program's own decomposition,
//! `MN-101`/`MN-102`).
//!
//! The founder research's central claim is that every privacy/availability/
//! integrity property is *purchasable* with a measurable resource cost, and
//! that five residual floors ([`ResidualFloor`]) are never removed by any
//! amount of spending. This crate turns that prose into typed, testable
//! data every future transport/storage surface can share, so "how private"
//! is a serializable, auditable value — never a marketing claim.
//!
//! ## What this crate is not
//!
//! It is pure policy vocabulary: no relay, mix network, erasure
//! replication, or payment mechanism is implemented here. [`tier::expected_cost`]
//! reproduces the founder research's own estimates, not a benchmark of
//! running code. Building the mechanisms Tier 1-3 assume is separate,
//! already-tracked work (`mini-net`, `mini-erasure`, `mini-value`, and new
//! crates the phase P2/P3 work will need) — see `docs/STATUS.md` §6.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod tier;
mod vocabulary;

pub use error::{PrivacyPolicyError, Result};
pub use tier::{
    expected_cost, AchievedPrivacy, PrivacyRequest, PrivacyTier, ResourceCost, MAX_FLOORS,
    MAX_MECHANISMS, MAX_PROPERTIES,
};
pub use vocabulary::{Mechanism, ProtectionProperty, ResidualFloor, RESIDUAL_FLOORS};

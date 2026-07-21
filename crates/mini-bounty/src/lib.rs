//! Anonymous developer-bounty claims (whitepaper's "how the rich
//! contribute" spirit, applied to "how developers get paid" — founder
//! direction, 2026-07-08): a contributor whose GitHub PR was approved can
//! claim a MINI payout without Mininet, the ledger, or any other observer
//! learning *which* approved contributor claimed it.
//!
//! ## The construction, in one paragraph
//!
//! When a contribution is approved (by a human maintainer reading
//! GitHub — never anonymous at that step, and this crate doesn't pretend
//! otherwise), the approver publishes only the contributor's one-time
//! claim public key as a [`pool::BountyGrant`] in a [`pool::BountyPool`].
//! To claim, the contributor produces a linkable ring signature (reusing
//! `mini_value::MininetRingSignature`, D-0036 — no new cryptographic
//! primitive is introduced here) proving they hold one of the pool's
//! grant keys, directing payout to a fresh stealth address
//! (`mini_value::MininetStealthAddress`). The ring signature's key image
//! prevents the same grant from paying out twice, tracked via
//! [`ledger::KeyImageLedger`]. Nobody but the claimant and whoever
//! approved their specific contribution ever learns which grant paid out
//! to which claim.
//!
//! ## Honest limits
//!
//! - **GitHub itself is never anonymous.** GitHub/Microsoft knows who
//!   pushed what, from what IP. The anonymity this crate provides is
//!   anonymity *from Mininet and the public ledger* — the same scoping
//!   the rest of this workspace already uses (the encrypted channel in
//!   `mini-bearer` is anonymous from network observers, not from an ISP).
//! - **No GitHub integration exists here.** This crate is the claim
//!   cryptography only — reading GitHub PR-approval events and minting
//!   [`pool::BountyGrant`]s from them is a separate, unbuilt integration
//!   layer.
//! - **Production payouts are gated by D-0047** (`docs/DECISION_LOG.md`):
//!   external cryptography audit is a hard gate for real value, same as
//!   every other `mini-value`/`mini-treasury` prototype. This crate adds
//!   no new primitive but does compose existing ones in a new way, which
//!   itself needs review before real MINI moves through it.
//! - **Anonymity-set size is the caller's responsibility.** A pool with
//!   one grant provides no anonymity at all — see
//!   [`pool::BountyPool::ring_size`].
//! - **The key-image ledger needs durable, consensus-backed storage in
//!   production** — [`ledger::InMemoryKeyImageLedger`] is for tests only,
//!   the same limitation `mini_presence::ReplayGuard` states for its own
//!   in-memory reference implementation.
//!
//! ## Funding one project at different rates (D-0336)
//!
//! [`pool::BountyPool::amount_per_grant_micro`] is deliberately flat across
//! every grant in a pool, permanently — varying it within one ring would
//! make the payout amount itself narrow down which grant claimed, eroding
//! the anonymity the ring exists to provide. To pay contributors to the
//! same project different amounts (e.g. a maintainer worth more than a
//! one-line fix), create *separate* flat-amount pools and optionally group
//! them with the same [`pool::BountyPool::project`] label — pure
//! organizational metadata, never part of what a claim signs over.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod claim;
mod error;
mod ledger;
mod pool;

pub use claim::{claim, verify_claim, BountyClaim};
pub use error::{BountyError, Result};
pub use ledger::{InMemoryKeyImageLedger, KeyImageLedger};
pub use pool::{BountyGrant, BountyPool};

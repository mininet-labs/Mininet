//! `mini-pq-anchor` -- PQ anchor pre-provisioning + wallet-facing inventory
//! (roadmap issue #231, "Frontier Trust 10"; PR #220's research proposal
//! §4.2).
//!
//! ## Scope
//!
//! Generating and tracking a dormant `mini_crypto::SignatureSuite::MlDsa65`
//! keypair a wallet holds in reserve, plus the UI-facing vocabulary a
//! client renders from. **Nothing here commits an anchor into any
//! `did-mini` KEL, attests to it, or grants it any authority.** That is
//! Phase 3 of `docs/design/post-quantum-identity-migration.md`, `did-mini`'s
//! work, not started, and gated on external cryptographic review before any
//! production identity use (CLAUDE.md's D-0047 external-audit gate).
//!
//! ## What this crate does not claim
//!
//! - It does not implement the emergency PQ migration procedure itself
//!   (roadmap issue #230) -- only the pre-provisioning half.
//! - It does not make an unanchored identity recoverable. PQ recovery
//!   Class C (PR #220 §4: an identity/fund with no pre-break unbroken
//!   anchor cannot be cryptographically distinguished from an attacker)
//!   remains exactly as unsolved as before this crate existed. This crate
//!   only helps identities that provisioned an anchor *before* a break.
//! - It does not persist an `MlDsa65` secret key across process restarts.
//!   `mini-crypto`'s Phase 2 boundary has no storage export/import path
//!   for that key type yet; a real wallet must solve on-device secret
//!   persistence separately, and this crate does not paper over that gap.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod anchor;
mod error;
mod inventory;

pub use anchor::{provision_anchor, AnchorStatus, PqAnchorRecord, MAX_LABEL_BYTES};
pub use error::{PqAnchorError, Result};
pub use inventory::{InventorySummary, PqAnchorInventory, MAX_ANCHORS_PER_OWNER};

//! `mini-airdrop` -- testnet-scale airdrop eligibility snapshot and
//! signed claim-redemption verification.
//!
//! ## Scope: what this crate is
//!
//! Two pieces, composed from already-reviewed primitives, no new
//! cryptography:
//!
//! 1. [`snapshot`] -- an eligibility snapshot builder enforcing one entry
//!    per identity root, bounded sizes, and a content-derived
//!    [`snapshot::AirdropSnapshot::digest`] a claim binds itself to.
//! 2. [`claim`] -- verifying a claimant's signed [`claim::ClaimRequest`]
//!    against a snapshot, the claimant's real `did-mini` KEL (proving
//!    control with the same keys the identity already trusts, not a
//!    bespoke scheme), and a [`registry::ClaimedRegistry`] for
//!    double-claim prevention.
//!
//! ## What this crate is not
//!
//! - **Not eligibility policy.** This crate has no opinion on *who*
//!   belongs in a snapshot or *why* -- that is a campaign-operator
//!   decision made entirely outside this crate, the same way
//!   `mini-provider`'s protocol never judges whether a declaration is
//!   honest.
//! - **Not Sybil-resistant.** `mini-uniqueness`'s own docs are explicit:
//!   identity root != verified human. [`snapshot::AllocationEntry::
//!   human_status`] carries a `mini-uniqueness::HumanStatus` signal
//!   purely as advisory campaign-operator information; this crate never
//!   reads it to decide eligibility, and one identity root claiming
//!   successfully proves nothing about how many humans control it. A
//!   real Sybil-resistant airdrop needs the still-unsolved personhood
//!   work tracked at roadmap issue #18 and the Frontier Trust Program
//!   (issues #222-#225), not this crate.
//! - **Not a settlement executor.** [`claim::verify_and_resolve_claim`]
//!   returns a [`claim::ClaimOutcome`] -- an amount and a recipient --
//!   never a signed `mini_settlement::PaymentClaim`. This crate never
//!   holds treasury signing authority and never could: whatever real
//!   custody mechanism controls the airdrop treasury (a `mini-treasury`
//!   FROST quorum, in production) is a separate system that takes a
//!   `ClaimOutcome` and builds its own settlement claim from it. FD-05
//!   applies unchanged: nothing here is ever final ownership by itself.
//! - **Not audited, not production-ready.** Gated behind D-0047 (external
//!   cryptographic/protocol audit) before any mainnet/real-value use,
//!   exactly like `mini-value` and `mini-treasury`'s own prototypes. This
//!   crate introduces no new cryptography (only already-reviewed
//!   `did-mini` KEL signature verification and `mini-crypto` hashing),
//!   but the audit gate covers the composition and the eligibility/claim
//!   protocol itself, not just novel primitives.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod claim;
mod error;
mod registry;
mod snapshot;

pub use claim::{
    message_to_sign, verify_and_resolve_claim, ClaimOutcome, ClaimRequest, MAX_RECIPIENT_BYTES,
};
pub use error::{AirdropError, Result};
pub use registry::{ClaimedRegistry, InMemoryClaimedRegistry};
pub use snapshot::{
    AirdropSnapshot, AllocationEntry, SnapshotBuilder, MAX_CAMPAIGN_ID_BYTES, MAX_ENTRIES,
    MAX_REASON_BYTES,
};

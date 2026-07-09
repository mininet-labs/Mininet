//! # mini-settlement
//!
//! Offline transaction settlement (SPEC-05, Directive 5, roadmap
//! [#41](https://github.com/britak420/Mininet/issues/41)): the protocol for
//! *"during outages, users exchange signed promises — not final ownership.
//! Ownership changes only when accepted into canonical consensus."*
//!
//! This crate implements the three frozen invariants that make offline-
//! first and single-canonical-truth reconcilable (`docs/INVARIANTS.md` §4,
//! D-0045):
//!
//! - **M1** — money never CRDT-merges. There is no merge function anywhere
//!   in this crate; [`reconcile::reconcile`] only ever answers "did this
//!   exact claim win," never "combine these claims."
//! - **M2** — a local/offline payment is a signed pending claim, never
//!   final, until canonical inclusion. [`state::SettlementState`] makes the
//!   pending/accepted/finalized distinction a type; only
//!   [`state::SettlementState::Finalized`] is final, and
//!   [`state::SettlementState::is_final`] is the one function that should
//!   ever answer "is this money mine."
//! - **M3** — canonical ordering alone resolves conflicting spends.
//!   [`reconcile::reconcile`] only ever reads a [`ledger::CanonicalLedgerView`]
//!   to decide; nothing in this crate has the authority to finalize a
//!   claim on its own.
//!
//! ## What this crate is not
//!
//! - **Not a ledger.** [`ledger::CanonicalLedgerView`] is a trait — the
//!   real chain-execution engine that tracks actual finalized balances is
//!   [roadmap #36-#45](https://github.com/britak420/Mininet/issues/36)'s
//!   job, not built here. This crate's protocol logic is fully specified
//!   and tested against [`ledger::InMemoryLedgerView`] today, and plugs
//!   into a real ledger later with no change to the reconciliation rules
//!   themselves — the same seam `mini-forge::KelDirectory` uses for
//!   identity lookups.
//! - **Not a payment channel.** A payer/payee pair here exchanges direct
//!   signed claims, not a bilaterally-signed, revocable channel state.
//!   Directive 5's own wording ("signed promises," not "channel states")
//!   is the simpler primitive this crate implements; a channel
//!   construction, if ever wanted, is future work layered on top, not a
//!   redesign of this crate.
//! - **Not confidential.** Amounts are plain `u64` micro-MINI (the same
//!   convention `mini-bounty`/`mini-reward` already use), not
//!   `mini-value`'s Bulletproofs-hidden confidential amounts. Wiring
//!   confidential amounts into settlement is real future work, not
//!   silently assumed away.
//! - **Not gated behind D-0047** the way `mini-value`/`mini-treasury`
//!   prototypes are, because this crate introduces no new cryptography at
//!   all — only `mini-crypto`'s already-reviewed Ed25519 signing and
//!   BLAKE3 hashing, composed into a state machine. The audit gate that
//!   matters here is on whatever real `CanonicalLedgerView` eventually
//!   backs it.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod claim;
mod error;
mod ledger;
mod reconcile;
mod state;
mod watcher;

pub use claim::{claim_digest, sign_claim, verify_claim_signature, PaymentClaim};
pub use error::{Result, SettlementError};
pub use ledger::{CanonicalLedgerView, InMemoryLedgerView};
pub use reconcile::{evaluate_local_acceptance, reconcile, LocalAcceptancePolicy};
pub use state::{SettlementState, WalletLabel};
pub use watcher::{ClaimWatcher, InMemoryClaimWatcher};

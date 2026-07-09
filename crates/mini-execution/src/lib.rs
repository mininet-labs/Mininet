//! # mini-execution
//!
//! The smallest deterministic state machine that turns
//! [`mini_chain`]'s finality verification into a real, chain-backed
//! [`mini_settlement::CanonicalLedgerView`] — the piece
//! `mini_settlement`'s own docs and D-0055's required follow-up named as
//! [roadmap #36-#45](../../issues/36)'s job, and the concrete mechanism
//! [roadmap #40](../../issues/40) ("double-spend reconciliation rules")
//! asks for.
//!
//! ## What this crate is
//!
//! - [`body::SettlementBlockBody`] — an ordered list of
//!   [`mini_settlement::PaymentClaim`]s proposed at one height; order is
//!   the canonical order M3 requires.
//! - [`state::LedgerState`] — for each payer, only the latest finalized
//!   `(sequence, digest)` pair (exactly what `CanonicalLedgerView` is ever
//!   asked for — see the module's own docs for why nothing more is kept),
//!   implementing [`mini_settlement::CanonicalLedgerView`] directly.
//! - [`state::apply_block`] — the state-transition function: a claim wins
//!   its slot only by strictly exceeding that payer's current
//!   high-water-mark; a bad signature, a stale sequence, or a second claim
//!   at an already-decided slot is silently dropped, never merged (M1).
//! - [`chain::LedgerChain`] — the one thing that matters most: state only
//!   ever advances behind a *real, verified* [`mini_chain::QuorumCertificate`]
//!   ([`mini_chain::verify_finality`]). There is no path to apply a
//!   block's claims without first proving it final.
//!
//! ## What this crate is not
//!
//! - **Not networked consensus.** No proposer rotation, no vote gossip, no
//!   round timeouts/view-change — [`mini_chain`]'s own docs name these as
//!   its explicit non-goals, and this crate inherits that boundary rather
//!   than closing it. Given a `(header, body, qc)` triple from *somewhere*
//!   (a real network, eventually), this crate answers "is this the next
//!   state" precisely and deterministically.
//! - **Not a general execution engine.** This state machine knows exactly
//!   one transaction type — [`mini_settlement::PaymentClaim`]. A real
//!   chain's state machine (governance, storage receipts, bounty claims,
//!   whatever else eventually anchors here) is further, separate work,
//!   the same way `mini-forge`'s own docs describe "the chain replaces
//!   the counting, not the objects."
//! - **Not gated behind D-0047.** No new cryptography — this composes
//!   `mini_chain`'s existing finality verification and
//!   `mini_settlement`'s existing claim verification; the only new
//!   content is deterministic bookkeeping and one content hash
//!   ([`state::LedgerState::commitment`]).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod body;
mod chain;
mod error;
mod state;

pub use body::{SettlementBlockBody, MAX_CLAIMS_PER_BLOCK};
pub use chain::LedgerChain;
pub use error::{ExecutionError, Result};
pub use state::{apply_block, LedgerState};

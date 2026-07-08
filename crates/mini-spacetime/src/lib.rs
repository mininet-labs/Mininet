//! Block-production selection weight from committed storage capacity —
//! the proof-of-space-time half of the whitepaper's hybrid consensus
//! (SS8.1), deliberately separate from `mini-chain`'s equal-weight-per-
//! human finality voting (D-0035 point 3).
//!
//! The whitepaper splits consensus into two unrelated axes:
//!
//! - **Block production** (this crate): weighted by *proven* storage
//!   capacity, with a concave curve, a per-identity cap, and a diversity
//!   bonus, so "doubling one's capacity yields less than double the
//!   reward." [`weight::proposer_weight`] is that formula.
//! - **Finality** (`mini-chain`, already shipped): a committee sampled from
//!   verified humans, equal weight per human, never stake — `ValidatorSet`
//!   states in its own docs "there is deliberately no weight field anywhere
//!   in this module."
//!
//! **These must never be confused.** [`weight::proposer_weight`] returns a
//! plain `u64` with no connection to `did_mini::Capabilities::VOTE`, no
//! shared type with `mini_chain::ValidatorSet`, and no path into governance
//! — storage capacity may make a node more likely to *propose* a block, it
//! can never make a human's vote count for more (P1 [FREEZE], unchanged).
//!
//! ## Honest limits
//!
//! This crate computes weight from *already-proven* capacity; it does not
//! prove capacity itself. [`proof::ProofOfSpaceTimeSource`] is the seam a
//! real proof-of-space-time/proof-of-replication protocol fills in — see
//! that module's honest limit and D-0035 point 5: the whitepaper explicitly
//! requires human authorship and external audit for this component, not
//! AI-authored code. Proposer *rotation* (turning weights into an actual
//! leader-election/lottery mechanism), the state machine, and networking
//! are further work, the same "finality math done, networked protocol
//! pending" honesty boundary `mini-chain` already states for its half.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod isqrt;
mod proof;
mod weight;

pub use error::{Result, SpaceTimeError};
pub use isqrt::isqrt;
pub use proof::{NoProof, ProofOfSpaceTimeSource};
pub use weight::{proposer_weight, ProposerParams};

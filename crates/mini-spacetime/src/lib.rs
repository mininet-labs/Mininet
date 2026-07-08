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
//! ## Proving capacity: start simple, real proof-of-replication later
//!
//! This crate computes weight from *already-proven* capacity.
//! [`proof::ProofOfSpaceTimeSource`] is the seam; [`storage_proof::MerkleStorageProof`]
//! is a real, working implementation (D-0037/D-0038's founder direction:
//! start with the simpler, well-documented construction now). It is a
//! Merkle/PDP-style challenge-response scheme — see that module's honest
//! limit for exactly what it does and does not prove. In particular: it
//! does **not** prove replication uniqueness (that a warehouse isn't
//! answering on behalf of many claimed small devices), which is the
//! stronger guarantee the whitepaper's "thousand cheap machines beat one
//! warehouse" thesis actually needs. Full proof-of-replication
//! (Filecoin-style sequential/time-locked encoding) is deliberately
//! treated as a separate, later, dedicated project, not compressed into
//! this pass. [`NoProof`] remains available as the fail-closed reference.
//!
//! Proposer *rotation* (turning weights into an actual leader-election/
//! lottery mechanism), the state machine, and networking are further
//! work, the same "finality math done, networked protocol pending"
//! honesty boundary `mini-chain` already states for its half.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod isqrt;
mod merkle;
mod proof;
mod storage_proof;
mod weight;

pub use error::{Result, SpaceTimeError};
pub use isqrt::isqrt;
pub use merkle::{MerkleProof, MerkleTree};
pub use proof::{NoProof, ProofOfSpaceTimeSource};
pub use storage_proof::{
    MerkleStorageProof, ProofHistory, StorageChallenge, StorageChallengeResponse,
    StorageCommitment, StorageWindowPolicy,
};
pub use weight::{proposer_weight, ProposerParams};

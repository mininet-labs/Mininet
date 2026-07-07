//! # mini-chain
//!
//! The finality-verification core of a custom Rust chain adapting a proven
//! Tendermint/CometBFT-style BFT design (Founder Decision A1,
//! `docs/DECISION_LOG.md` D-0008): **equal validator power per verified
//! identity root, never stake** (P1/P2 \[FREEZE\]).
//!
//! ## What this batch implements, honestly
//!
//! This crate is the **finality math**: [`ValidatorSet`] (equal weight, no
//! weight field anywhere to misuse), [`BlockHeader`] (canonical, self-
//! certifying content hash), [`Vote`]/[`sign_vote`]/[`verify_vote`] (a
//! validator device's signed commitment, gated on
//! `did_mini::Capabilities::VOTE` — this crate is that capability's first
//! real consumer), and [`QuorumCertificate`]/[`verify_finality`] (`>2/3`
//! distinct validator roots precommitting the same block is what makes a
//! Tendermint-style chain final instantly, without waiting for
//! probabilistic confirmations).
//!
//! **Not implemented here, and not claimed to be:** proposer rotation,
//! round timeouts/view-change, vote gossip/networking, and state-machine
//! execution — the actual networked consensus protocol. This crate answers
//! one question, offline and precisely: *given this candidate set of votes,
//! is this block final?* — the same relationship `mini-forge`'s
//! attestation-counting already has to the eventual chain (see
//! `mini-forge`'s crate docs: "the chain replaces the counting, not the
//! objects"). Value settlement, the release registry, and constitution-
//! guard enforcement (`docs/ROADMAP.md` Pack 9) build on this later.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod block;
mod error;
mod finality;
mod validator;
mod vote;

pub use block::BlockHeader;
pub use error::{ChainError, Result};
pub use finality::{verify_finality, QuorumCertificate, ValidatorOracle};
pub use validator::ValidatorSet;
pub use vote::{sign_vote, verify_vote, Vote, VoteKind};

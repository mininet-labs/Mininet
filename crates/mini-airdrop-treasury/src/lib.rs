//! `mini-airdrop-treasury` -- bridges a `mini-airdrop` `ClaimOutcome` to a
//! real treasury payout *approval*, composing already-existing pieces
//! rather than inventing anything new.
//!
//! ## Scope
//!
//! [`verify_payout_approvals`] checks candidate approvals (a treasury
//! signer's real `did-mini` KEL plus signatures over
//! [`payout_message`]) against a `mini_treasury::TreasurySignerSet`,
//! using `mini_treasury::meets_threshold`'s existing distinct-identity
//! counting -- the same "did enough authorized people agree" discipline
//! `mini-forge`'s governance approval counting and `mini_treasury::
//! signers` itself already use safely, composed here rather than
//! reimplemented.
//!
//! ## What this crate deliberately does not do
//!
//! - **Does not touch `mini_treasury::frost_sign`.** That module's own
//!   docs name it the "permanent honeypot" component: generating and
//!   combining real threshold-signature shares over actual treasury
//!   funds, which the whitepaper (§11) and D-0035 require external
//!   cryptographic audit for before any production use. This crate's
//!   approval counting is ordinary identity counting, not a cryptographic
//!   threshold scheme, and produces no signature at all.
//! - **Does not produce a `mini_settlement::PaymentClaim`.** A
//!   [`TreasuryApprovedPayout`] is evidence that enough authorized signers
//!   agreed to a specific payout -- not a signed settlement claim.
//!   Building and signing the real `PaymentClaim` from an approved payout
//!   is separate, later work, and still requires solving the same
//!   signature-suite question `mini_treasury::frost_sign`'s Ristretto
//!   Schnorr output raises against `mini_crypto::SignatureSuite`'s
//!   Ed25519/ML-DSA-65-only enumeration -- deliberately not decided here.
//! - **No new cryptography.** Only real `did-mini` KEL signature
//!   verification, composed exactly like `mini-airdrop`'s own claimant
//!   verification.
//! - **Not audited, not production-ready.** Gated behind D-0047 like
//!   every other crate touching treasury custody.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod approval;
mod error;
mod reconciliation;

pub use approval::{
    payout_message, verify_payout_approvals, CandidateApproval, TreasuryApprovedPayout,
};
pub use error::{PayoutApprovalError, ReconciliationError, ReconciliationResult, Result};
pub use reconciliation::{check_snapshot_within_treasury_balance, total_allocated_micro};

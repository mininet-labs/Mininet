//! Community-governed BTC/XMR-to-MINI contribution bookkeeping (whitepaper
//! §8.2 "how the rich contribute" / §10 treasury custody), split by risk
//! class the same way `mini-uniqueness` and `mini-spacetime` split their
//! own novel-cryptography pieces (D-0035 point 5).
//!
//! **Safe to build now (ordinary bookkeeping and arithmetic):**
//!
//! - [`rate`] — a governed exchange-rate history and the multiplication
//!   that turns a contribution into a minted amount at whatever rate was in
//!   effect. Has no opinion on how the rate is set (ordinary flat-vote
//!   governance) or whether a contribution actually arrived.
//! - [`receipt::ContributionReceipt`] — the bookkeeping record of a claimed
//!   contribution (asset, amount, rate, minted MINI).
//! - [`signers`] — **who** is authorized to approve treasury actions and
//!   whether enough of them agreed, mirroring `mini_forge`'s governance
//!   approval-counting pattern: distinct-identity counting only, no
//!   weight field, no path to extra voting power for being a signer (P1
//!   unchanged).
//!
//! **Deliberately not built here (whitepaper: "a permanent honeypot by
//! nature"; D-0035 point 5 requires human authorship + external audit):**
//!
//! - [`receipt::ExternalReceiptOracle`] — verifying a Bitcoin or Monero
//!   transaction actually paid the treasury is real cross-chain
//!   engineering. `NoExternalReceiptOracle` is the correct, permanent stand-in.
//! - Real threshold-signature custody (e.g. FROST) over actual treasury
//!   funds. [`signers::meets_threshold`] answers "did enough authorized
//!   people agree," never "here is a valid signature the treasury would
//!   accept" — that scheme does not exist in this crate.
//!
//! This crate is bookkeeping and governance-membership data, not a
//! deployable treasury.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod rate;
mod receipt;
mod signers;

pub use error::{Result, TreasuryError};
pub use rate::{mint_amount_micro, RateEntry, RateHistory, RATE_SCALE};
pub use receipt::{
    ContributionKind, ContributionReceipt, ExternalReceiptOracle, NoExternalReceiptOracle,
};
pub use signers::{count_valid_approvals, meets_threshold, TreasurySignerSet, MAX_SIGNERS};

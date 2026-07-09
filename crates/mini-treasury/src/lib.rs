//! Community-governed BTC/XMR-to-MINI contribution bookkeeping and
//! threshold-signature custody (whitepaper §8.2 "how the rich contribute" /
//! §10 treasury custody).
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
//!   unchanged). This is identity-level authorization ("is this person on
//!   the committee"), a separate question from [`frost_sign`]'s
//!   cryptographic signing ("here is a valid signature the committee
//!   produced").
//!
//! **Founder-overridden (D-0037), AI-authored prototype — real threshold-
//! signature custody, not a stub, but explicitly pending external
//! cryptography audit:**
//!
//! - [`frost_keygen`]/[`frost_sign`] — FROST (Flexible Round-Optimized
//!   Schnorr Threshold signatures, Komlo & Goldberg): any `threshold`-sized
//!   subset of a committee can jointly produce one ordinary Schnorr
//!   signature under a shared group public key, without any single device
//!   ever holding the full secret key. See `examples/frost_live_demo.rs`
//!   for a runnable multi-device signing session over real (simulated)
//!   message-passing.
//!
//! **Still deliberately not built here (whitepaper: "a permanent honeypot
//! by nature"; D-0035 point 5's external-audit requirement stands even
//! under D-0037's authorship policy change for this specific gap):**
//!
//! - [`receipt::ExternalReceiptOracle`] — verifying a Bitcoin or Monero
//!   transaction actually paid the treasury is real cross-chain
//!   engineering (confirmation depth, reorg safety, Monero's view-key
//!   scanning). `NoExternalReceiptOracle` is the correct, permanent
//!   stand-in — this is not a cryptographic-design gap FROST or any other
//!   primitive here closes, it is a whole separate integration surface.
//!
//! [FREEZE reminder — D-0037] The FROST prototype above is founder-
//! reviewed, not externally audited. Nothing in this crate should be read
//! as "custody solved" for real funds until that audit happens, and until
//! trusted-dealer keygen is replaced by real distributed key generation
//! (see [`frost_keygen`]'s honest limit — every call site must name
//! [`frost_keygen::AcknowledgedPrototypeOnly`] explicitly, so this is never
//! reachable by accident). [`frost_sign::SigningNonces`] zeroizes on drop
//! and redacts its `Debug` output (issue #93).
//!
//! This crate is bookkeeping, governance-membership data, and a threshold-
//! signature prototype — not a deployable treasury.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod curve;
mod error;
mod frost_keygen;
mod frost_sign;
mod rate;
mod receipt;
mod signers;

pub use error::{Result, TreasuryError};
pub use frost_keygen::{
    trusted_dealer_keygen, AcknowledgedPrototypeOnly, KeyPackage, PublicKeyPackage,
    MAX_PARTICIPANTS as MAX_FROST_PARTICIPANTS,
};
pub use frost_sign::{
    aggregate, round1_commit, round2_sign, verify, verify_signature_share, NonceCommitment,
    Signature, SigningNonces, SigningPackage,
};
pub use rate::{mint_amount_micro, RateEntry, RateHistory, RATE_SCALE};
pub use receipt::{
    ContributionKind, ContributionReceipt, ExternalReceiptOracle, NoExternalReceiptOracle,
};
pub use signers::{count_valid_approvals, meets_threshold, TreasurySignerSet, MAX_SIGNERS};

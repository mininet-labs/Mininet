//! Transaction privacy for the one MINI ledger (whitepaper §8, D-0035
//! point 4) — the most conservative crate of the D-0034 batch, built last,
//! per founder direction (highest risk: real value, real cryptography).
//!
//! **Ordinary bookkeeping, no cryptography:**
//!
//! - [`fee`] — the governed fee mechanism (whitepaper §8.4): a real-world
//!   value target converted into a MINI amount at a governed price, so a
//!   view costs a steady fraction of a cent regardless of MINI's market
//!   price. Same shape and safety class as `mini_treasury::rate`.
//!
//! **Founder-overridden (D-0036/D-0037), AI-authored prototypes — real
//! implementations, not stubs, but explicitly pending external
//! cryptography audit:**
//!
//! - [`stealth_impl::MininetStealthAddress`] — a CryptoNote-style stealth-
//!   address scheme: a fresh one-time output address per payment,
//!   unlinkable to the recipient's real address, recognized by the
//!   recipient's view key alone.
//! - [`ring_impl::MininetRingSignature`] — a single-layer MLSAG/AOS-style
//!   linkable ring signature: proves one of N public keys authorized a
//!   spend without revealing which, plus a key image for double-spend
//!   detection.
//! - [`confidential_impl::MininetConfidentialAmount`] — a single-value
//!   Bulletproofs range proof: a Pedersen commitment plus an `O(log n)`-
//!   size proof that the committed value lies in `[0, 2^64)`, via bit
//!   decomposition and the inner product argument ([`bp_ipa`]).
//!   `verify_balance` needs no separate proof: Pedersen commitments are
//!   additively homomorphic, so checking inputs balance outputs is
//!   exactly an elliptic-curve point-sum equality check.
//!
//! All three are built on `curve25519-dalek`'s Ristretto group (the same
//! audited primitive-layer crate `ed25519-dalek`/`x25519-dalek` already
//! use, D-0014's precedent) — the group arithmetic is depended on, the
//! protocols on top are Mininet-owned. [`NoStealthAddress`]/
//! [`NoRingSignature`]/[`NoConfidentialAmount`] remain available as
//! fail-closed references for anyone not opting into the prototypes.
//!
//! [FREEZE reminder — D-0036/D-0037] The prototypes above are founder-
//! reviewed, not externally audited. Nothing in this crate should be read
//! as "privacy achieved" for real value until that audit happens. See each
//! module's own honest limit.
//!
//! None of this is a second currency (D-0035 point 1): these are
//! transaction-privacy primitives for MINI, the same one currency
//! `mini-reward`'s vesting accrual feeds into.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod bp_generators;
mod bp_ipa;
mod bp_range;
mod confidential;
mod confidential_impl;
mod curve;
mod error;
mod fee;
mod ring;
mod ring_impl;
mod stealth;
mod stealth_impl;

pub use bp_range::RangeProof;
pub use confidential::{ConfidentialAmountScheme, NoConfidentialAmount};
pub use confidential_impl::MininetConfidentialAmount;
pub use error::{Result, ValueError};
pub use fee::{fee_in_micro_mini, PriceEntry, PriceHistory, PRICE_SCALE};
pub use ring::{NoRingSignature, RingSignature, RingSignatureScheme};
pub use ring_impl::MininetRingSignature;
pub use stealth::{NoStealthAddress, StealthAddressScheme, StealthOutput};
pub use stealth_impl::{derive_spend_scalar, MininetStealthAddress, StealthKeypair};

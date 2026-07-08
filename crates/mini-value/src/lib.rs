//! Transaction privacy for the one MINI ledger (whitepaper §8, D-0035
//! point 4) — deliberately the most conservative crate of this batch, and
//! the last one built, per founder direction (highest risk: real value,
//! real cryptography).
//!
//! **Safe to build now (ordinary bookkeeping, no cryptography):**
//!
//! - [`fee`] — the governed fee mechanism (whitepaper §8.4): a real-world
//!   value target converted into a MINI amount at a governed price, so a
//!   view costs a steady fraction of a cent regardless of MINI's market
//!   price. Same shape and safety class as `mini_treasury::rate`.
//!
//! **Deliberately not built here — trait seams only, per D-0035 point 5:**
//!
//! - [`ring`] — ring signatures (prove one of N keys authorized a spend,
//!   without revealing which, plus a key image for double-spend
//!   detection). `NoRingSignature`.
//! - [`stealth`] — stealth addresses (a fresh one-time output address per
//!   payment, unlinkable to the recipient's real address).
//!   `NoStealthAddress`.
//! - [`confidential`] — RingCT-style confidential amounts (homomorphic
//!   commitments + range proofs hiding amounts while still proving no
//!   value was created). `NoConfidentialAmount`.
//!
//! Every stub in this crate **fails closed**: none of them sign, derive,
//! commit, or verify anything as valid. An absent real implementation can
//! never be mistaken for a working one, and nothing in this crate should
//! be read as "privacy achieved" — it is the shape a human-authored,
//! externally-audited implementation fills in, nothing more. See each
//! module's own honest limit for why that specific primitive is
//! genuinely dangerous to get subtly wrong.
//!
//! None of this is a second currency (D-0035 point 1): these are
//! transaction-privacy primitives for MINI, the same one currency
//! `mini-reward`'s vesting accrual feeds into.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod confidential;
mod error;
mod fee;
mod ring;
mod stealth;

pub use confidential::{ConfidentialAmountScheme, NoConfidentialAmount};
pub use error::{Result, ValueError};
pub use fee::{fee_in_micro_mini, PriceEntry, PriceHistory, PRICE_SCALE};
pub use ring::{NoRingSignature, RingSignature, RingSignatureScheme};
pub use stealth::{NoStealthAddress, StealthAddressScheme};

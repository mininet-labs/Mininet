//! Mutually-signed storage-served receipts.
//!
//! A [`ServeReceipt`] closes the gap `mini-reward`/`mini-store` both
//! flagged as `pending`: the receipt-signing/verification pipeline that
//! connects `mini-store::CacheTier::CommittedStorage` to a real, witnessed
//! `mini_reward::accrue_storage` input. Two delegated devices — a host and
//! a witness, each bound to a `did:mini` identity root — sign one
//! transcript ([`ReceiptFields::transcript`]) that binds: the content id,
//! bytes served, a digest of what the witness actually received, both
//! device ids, fresh nonces, and a timestamp.
//!
//! [`verify_serve`] then requires, for *both* sides:
//!
//! - the device KEL verifies and is a delegated, unrevoked device of an
//!   identity root, with the `ATTEST` capability (the same capability
//!   `mini-presence` uses for co-presence — attesting a claim about lived
//!   reality, not posting content);
//! - the signature verifies against the device's current keys;
//! - fresh, non-replayed nonces and a receipt within the freshness policy;
//! - the two identity roots are distinct (a host cannot witness, and be
//!   rewarded for, its own storage).
//!
//! The resulting [`ServeVerdict`] is the same shape
//! `mini_reward::accrue_storage` consumes directly — mirrors exactly how
//! `mini_presence::PresenceVerdict` feeds `mini_reward::accrue`.
//!
//! ## Honest limit
//!
//! A receipt proves a serve *happened*, once, at a point in time. It does
//! **not** prove the host keeps serving that content tomorrow — durable
//! storage-over-time is a harder property (challenge-response
//! proof-of-storage) and remains `pending`, the same honest limit
//! `mini-presence` states for its distance-bounding. This crate provides
//! the signed, bound, replay-checked envelope such a proof can slot into
//! later. Automatic receipt emission during a real `mini-sync` exchange is
//! also `pending` — this crate verifies receipts, it does not yet produce
//! them as a side effect of serving.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod receipt;
mod verify;

pub use error::{Result, StorageProofError};
pub use receipt::{ReceiptFields, ServeReceipt, RECEIPT_VERSION};
pub use verify::{
    verify_serve, FreshnessPolicy, InMemoryReplayGuard, ReplayGuard, ServeVerdict, VerifyContext,
};

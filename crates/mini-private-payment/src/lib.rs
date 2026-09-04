//! `mini-private-payment` — the shielded settlement path.
//!
//! **Maturity: experimental prototype cryptography, unaudited, gated behind
//! D-0047/#72.** It composes `mini-value`'s stealth addresses, ring
//! signatures, and Bulletproofs, all three of which are founder-overridden
//! AI-authored prototypes (D-0036/D-0040). Nothing here may carry real
//! value until an external cryptographic audit says otherwise. Getting a
//! privacy construction subtly wrong does not fail loudly — it produces
//! payments that look private and are not.
//!
//! # The gap this closes
//!
//! `mini-value` has had real stealth addresses, ring signatures, and range
//! proofs for some time. **Nothing composed them into a payment.** Every
//! payment this tree can actually make goes through
//! `mini_settlement::PaymentClaim`, which carries a stable payer key, a
//! stable payee address, a cleartext `amount_micro`, and a per-payer
//! `sequence` counter. Every crate that pays anyone — `mini-contribution`
//! for creator and seeder payouts, `mini-engagement` for escrowed work,
//! `mini-bounty` — settles through it.
//!
//! So the complete transaction graph is public by construction: who paid
//! whom, how much, in what order. The `sequence` field alone hands an
//! observer each payer's ordered payment history, no cryptanalysis
//! required. That is not a gap in a privacy feature; it is the absence of
//! one, in a project whose Directive 9 says privacy is *architecture*, not
//! a promise, and whose stated purpose is returning ownership of data to
//! the people it describes.
//!
//! This crate is the composition that was missing.
//!
//! # What a private payment hides
//!
//! | | transparent claim | here |
//! |---|---|---|
//! | payer | a stable public key | nothing — a ring signature over an anonymity set |
//! | payee | a stable address | a fresh one-time [`mini_value::StealthOutput`] |
//! | amount | `u64`, in the clear | a Pedersen commitment with a range proof |
//! | ordering | `sequence`, per payer | nothing |
//! | purpose | (absent, or it would leak) | [`SealedMemo`], readable only by the recipient |
//!
//! # What it does **not** hide, stated plainly
//!
//! - **The key image is linkable, by design.** Two spends of the same
//!   output produce the same [`VerifiedPrivateClaim::key_image`], which is
//!   exactly what makes double-spend detection possible without a public
//!   payer. This crate therefore never says "unlinkable" without
//!   qualification.
//! - **Decoys are chosen by the protocol, not by the wallet** (D-0449).
//!   [`select_ring`] samples them under one rule, recency-weighted to match
//!   how real spends are distributed, in integer arithmetic so every machine
//!   computes the same ring. That closes two things: uniform decoys made the
//!   newest ring member the real one with high probability, and per-wallet
//!   sampling let an observer identify *which wallet* made a payment from
//!   the shape of its ring. What it does not close: the weights are a
//!   legible starting shape, not a distribution fitted to measured traffic,
//!   because no such traffic exists yet. Known statistical attacks on
//!   ring-based anonymity are reduced here, not eliminated.
//! - **The output set must be local.** A peer that serves you decoy keys
//!   learns your ring, and your ring contains your real output — there is no
//!   way to ask that does not hand over the answer. So a device that cannot
//!   hold an [`OutputSet`] does not make private payments from that device.
//! - **Network-level privacy is elsewhere.** Timing, IP, and traffic
//!   analysis are `mini-relay`'s and `mini-transport-security`'s job. A
//!   payment that is cryptographically private and broadcast from a fixed
//!   IP immediately after viewing one post is not private in practice.
//! - **Counts and timing remain visible.** How many payments exist, and
//!   when, is public. Amounts and parties are not.
//! - **Sybil is still unsolved.** Nothing here counts humans, and nothing
//!   here should ever be cited as if it did (roadmap #18).
//! - **No consensus.** Reconciliation asks a [`PrivateLedgerView`]; no such
//!   implementation is chain-backed yet, exactly as `mini-settlement`
//!   waited for `mini_execution::LedgerChain`. Until one exists, no private
//!   payment can reach [`mini_settlement::SettlementState::Finalized`] in
//!   production.
//!
//! # Auditability without a transparent format (D-0451)
//!
//! Some payments should be checkable by anyone — a treasury disbursement
//! most obviously. The tempting answer is to keep the transparent
//! `mini_settlement::PaymentClaim` path alongside this one and use it for
//! those. That is the wrong shape: **if both formats exist, choosing the
//! private one is itself a signal.** Privacy that must be opted into is
//! privacy for nobody, because the people who most need it are exactly the
//! ones whose opting-in stands out.
//!
//! So auditability is a property a party asserts *about itself*, not a
//! property of the format. Nothing is public by default; an account that
//! wants to be auditable publishes its view key ([`ViewKeyDisclosure`]) and
//! anyone can then [`audit`] its income. The treasury can be fully
//! accountable without exposing anyone else.
//!
//! Disclosure is retroactive, irrevocable, and exposes the memos of senders
//! who never agreed to it, so [`ViewKeyDisclosure::create`] takes a typed
//! [`AcknowledgedIrreversibleDisclosure`] rather than a flag. It reveals
//! *income only* — a view key cannot show what an account spent — and it
//! proves nothing about completeness, since no cryptography can show a
//! disclosed account is the only one its holder controls.
//!
//! # The voice/value wall
//!
//! This is a **value** crate (P1, Directive 16). It depends on
//! `mini-value`, `mini-crypto`, and `mini-settlement`, and on nothing
//! governance-shaped: no `mini-forge`, no `mini-chain` voting, in either
//! direction. It has no field, method, or path that turns a payment into
//! weight of any kind, and it must never acquire one — private money buying
//! quiet governance would be the worst version of the failure Directive 16
//! exists to prevent.
//!
//! It also does **not** depend on `mini-social`, though paying a creator for
//! a post is its motivating use. A payment layer that knew what a post was
//! would be a payment layer that could be made to treat some posts
//! differently. [`PaymentPurpose`] carries opaque caller bytes instead, and
//! the end-to-end social vertical lives in `tests/unity.rs` where it proves
//! the composition works without making it permanent.
//!
//! # Position in the lifecycle
//!
//! ```text
//! mini-social      publish a post           -> ObjectId
//! mini-store       a reader views it        -> local cache tier, no viewer identity
//! mini-private-payment  the reader pays     -> stealth output + ring sig + range proof
//!                                              + the ObjectId sealed in the memo
//! mini-settlement  the shared vocabulary    -> Pending / AcceptedLocal / Finalized
//! (pending)        canonical consensus      -> the only thing that makes it final
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod claim;
mod codec;
mod decoy;
mod disclosure;
mod error;
mod memo;
mod nullifier;
mod reconcile;
mod scan;

pub use claim::{
    build, canonicalize_ring, ring_is_canonical, verify, PaymentRequest, PrivatePaymentClaim,
    VerifiedPrivateClaim, ABSOLUTE_MIN_RING_SIZE, CLAIM_TRANSCRIPT_DOMAIN, CLAIM_VERSION,
    MAX_RING_SIZE, MIN_RING_SIZE,
};
pub use codec::MAX_FIELD_BYTES;
pub use decoy::{select_ring, InMemoryOutputSet, OutputSet, AGE_WEIGHTS, DECOY_DOMAIN};
pub use disclosure::{
    audit, verify_disclosure, AcknowledgedIrreversibleDisclosure, VerifiedDisclosure,
    ViewKeyDisclosure, DISCLOSURE_DOMAIN, DISCLOSURE_VERSION,
};
pub use error::{DecodeFailure, PrivatePaymentError, Result};
pub use memo::{PaymentPurpose, SealedMemo, MAX_MEMO_BYTES, MEMO_KDF_INFO, MEMO_PADDED_BYTES};
pub use nullifier::{KeyImageSet, SpendOutcome};
pub use reconcile::{reconcile, InMemoryPrivateLedger, PrivateLedgerView};
pub use scan::{recognizes, scan, scan_one, RecognizedPayment, ScanOutcome};

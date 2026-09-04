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
//! | payer | a stable public key | nothing — an MLSAG over an anonymity set |
//! | payee | a stable address | a fresh one-time [`mini_value::StealthOutput`] |
//! | amount | `u64`, in the clear | a Pedersen commitment with a range proof |
//! | ordering | `sequence`, per payer | nothing |
//! | purpose | (absent, or it would leak) | [`SealedMemo`], readable only by the recipient |
//! | change | (implicit: the payer's balance) | an output like any other, indistinguishable |
//!
//! # Value conservation, which is what makes hidden amounts safe
//!
//! Hiding an amount is worth nothing on its own. If a claim can commit to
//! any number it likes, "private money" is money anybody can mint, and the
//! privacy is the thing that stops you noticing.
//!
//! So every claim proves `Σ inputs = Σ outputs + fee` *without opening a
//! single one of those amounts*, exactly as RingCT does. Pedersen
//! commitments are additively homomorphic, so a verifier sums the input
//! commitments, sums the output commitments plus a commitment to the public
//! fee, and checks the difference is a commitment to zero. Two things make
//! that sound:
//!
//! - **Range proofs.** Without them a "negative" output balances the
//!   equation while minting value, so [`verify`] checks every output's
//!   Bulletproof before it checks the sum.
//! - **Pseudo-output commitments.** A spent output's own commitment cannot
//!   appear in the sum — publishing it would say which ring member was
//!   real, and the ring would stop hiding anyone. Each input instead carries
//!   a *re-blinded* commitment to the same value, and its
//!   [`mini_value::MlsagSignature`] proves, in one ring, both that the
//!   signer controls some member and that the pseudo-commitment hides that
//!   member's value. The blinding factors are chosen so the differences
//!   cancel across the claim.
//!
//! A claim may spend up to [`MAX_INPUTS`] outputs and create up to
//! [`MAX_OUTPUTS`], which is what makes real payments possible: you rarely
//! hold an output worth exactly what you owe. Change is **not** a field or a
//! flag — it is an output paying yourself, built the same way as any other,
//! so nothing on the wire says which output was the payment and which was
//! the change.
//!
//! # What it does **not** hide, stated plainly
//!
//! - **The key image is linkable, by design.** Two spends of the same
//!   output produce the same key image — see
//!   [`VerifiedPrivateClaim::key_images`] — which is exactly what makes
//!   double-spend detection possible without a public payer. This crate
//!   therefore never says "unlinkable" without qualification.
//! - **A claim's inputs are linked to each other.** Spending several
//!   outputs in one claim tells an observer those outputs share an owner,
//!   without telling them who. That is inherent to the construction, not an
//!   oversight: the alternative is a separate claim per input, which leaks
//!   through timing instead.
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
//! - **The fee is public, and so is the shape of a claim.** A verifier must
//!   be able to check that the fee charged is the fee declared, and a hidden
//!   fee would need its own range proof and still leave the network unable
//!   to prioritize — so `fee_micro` is in the clear. An observer therefore
//!   learns each claim's fee, its number of inputs, and its number of
//!   outputs. That is a real fingerprint: an unusual fee, or an unusual
//!   input count, narrows which claims could be yours. It is stated here
//!   rather than papered over, and it is why a wallet should prefer the
//!   ordinary shapes.
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
//! A view key enumerates payments; it does not open a Pedersen commitment,
//! so it cannot add them up. [`AmountDisclosure`] closes that separately:
//! the recipient publishes an output's `(amount, blinding)` opening and
//! anyone recomputes the commitment. Because openings are **chosen**,
//! [`audit_amounts`] never returns a bare total — [`AuditedIncome`] carries
//! the opened sum beside the number of payments left unopened, and only
//! [`AuditedIncome::is_complete`] licenses reading the first as an account's
//! income. A sum that quietly meant "the part they chose to show" would be
//! the most useful-looking dishonest number this crate could produce.
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
//! mini-private-payment  the reader pays     -> stealth outputs + MLSAG per input
//!                                              + range proofs + a balance that sums
//!                                              + the ObjectId sealed in the memo
//! mini-settlement  the shared vocabulary    -> Pending / AcceptedLocal / Finalized
//! (pending)        canonical consensus      -> the only thing that makes it final
//! ```

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod amount;
mod claim;
mod codec;
mod decoy;
mod disclosure;
mod error;
mod memo;
mod nullifier;
mod reconcile;
mod scan;

pub use amount::{
    audit_amounts, AcknowledgedAmountDisclosure, AmountDisclosure, AuditedIncome, OpenedPayment,
    AMOUNT_DISCLOSURE_DOMAIN, AMOUNT_DISCLOSURE_VERSION,
};
pub use claim::{
    build, canonicalize_ring, ring_is_canonical, verify, BuiltOutput, ClaimInput, ClaimOutput,
    PaymentRequest, PrivatePaymentClaim, Recipient, SpendableOutput, VerifiedPrivateClaim,
    ABSOLUTE_MIN_RING_SIZE, CLAIM_TRANSCRIPT_DOMAIN, CLAIM_VERSION, MAX_INPUTS, MAX_OUTPUTS,
    MAX_RING_SIZE, MIN_RING_SIZE,
};
pub use codec::MAX_FIELD_BYTES;
pub use decoy::{
    select_ring, select_ring_indices, InMemoryOutputSet, OutputSet, AGE_WEIGHTS, DECOY_DOMAIN,
};
pub use disclosure::{
    audit, verify_disclosure, AcknowledgedIrreversibleDisclosure, VerifiedDisclosure,
    ViewKeyDisclosure, DISCLOSURE_DOMAIN, DISCLOSURE_VERSION,
};
pub use error::{DecodeFailure, PrivatePaymentError, Result};
pub use memo::{
    PaymentNote, PaymentPurpose, SealedMemo, MAX_MEMO_BYTES, MEMO_KDF_INFO, MEMO_PADDED_BYTES,
    NOTE_OVERHEAD_BYTES,
};
pub use nullifier::{KeyImageSet, SpendOutcome};
pub use reconcile::{
    reconcile, ChainBackedPrivateLedger, InMemoryPrivateLedger, PrivateLedgerView,
};
pub use scan::{recognizes, scan, scan_one, RecognizedPayment, ScanOutcome};

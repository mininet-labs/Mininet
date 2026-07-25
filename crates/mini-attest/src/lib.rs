//! `mini-attest` -- FD-18 Wave 4 engagement-proven reviews.
//!
//! This first slice implements only **Tier 0**, whose stable API label is
//! [`AssuranceTier::LinkableTier0`]. A Tier-0 review publicly discloses the
//! exact completion receipt it relies on, so the provider, verifier, and
//! observers can correlate the engagement and review. Nothing in this crate
//! is anonymous, unlinkable, zero knowledge, or a personhood proof.
//!
//! ## Authority boundary
//!
//! This crate is an edge leaf. It creates no `HumanEvidence`, human status,
//! unique-human credential, governance role, vote, settlement finality, or
//! provider registry. A valid review proves only that:
//!
//! 1. a provider signed a minimal, content-addressed receipt;
//! 2. the supplied original engagement is locally complete and its payment
//!    claim is finalized in the caller's [`mini_settlement::CanonicalLedgerView`];
//! 3. the receipt matches that engagement and its provider grant;
//! 4. the pairwise grant subject signed the review payload; and
//! 5. the presenter knows the secret behind the grant's holder commitment.
//!
//! The canonical-settlement check deliberately requires the original
//! [`mini_engagement::Engagement`] and ledger view. The current ledger API has
//! no standalone inclusion proof keyed only by a claim digest; a provider
//! signature is not silently promoted into consensus evidence.
//!
//! ## Public-copy privacy boundary
//!
//! Receipt and review wire formats contain no payment amount, payer/payee
//! address, free-form engagement terms, or human-root DID. They do contain a
//! pairwise reviewer pseudonym, provider DID, receipt id, claim digest, and
//! subject commitments. That is still linkable metadata. The holder token is
//! presented separately to a verifier and is never serialized into a review.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod error;
mod holder;
mod receipt;
mod review;

pub use error::{AttestError, Result};
pub use holder::{EngagementHolderToken, HolderCommitment};
pub use receipt::{
    engagement_id, issue_tier0_receipt, verify_tier0_receipt, EngagementCompletionReceiptV1,
    ReceiptId, ReviewSubjectCommitment, SettlementReference, Tier0ReceiptContext, RECEIPT_VERSION,
};
pub use review::{
    issue_tier0_review, verify_and_record_tier0_review, AssuranceTier, InMemoryReviewRegistry,
    ReviewPayload, ReviewRegistry, SignedReviewV1, Tier0ReviewVerification, Verdict,
    MAX_REVIEW_BODY_BYTES, REVIEW_VERSION,
};

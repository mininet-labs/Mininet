//! Three-signal personhood/uniqueness fusion (whitepaper SS5), superseding
//! D-0034 point 2's "left to us" framing now that the founding whitepaper
//! specifies a concrete design (D-0035 point 2).
//!
//! Sybil resistance without a central authority is, per the whitepaper, "the
//! hardest problem in the entire system." Mininet's answer fuses three
//! independent signals into one confidence score:
//!
//! - **(a) Social-vouching graph** — [`vouch`]/[`verify`] build mutual,
//!   signed vouch attestations between identity roots (mirroring
//!   `mini-presence`'s two-party attestation pattern exactly); [`graph`]
//!   propagates trust outward from a small trusted seed set (SybilRank-
//!   style), so a Sybil cluster's internal edges don't help it — only edges
//!   *into* the trusted region do.
//! - **(b) On-device behavioral/location entropy** — a zero-knowledge proof
//!   of genuine human movement. **Not implemented.** [`confidence::BehavioralEntropySource`]
//!   is the seam; see its honest limit and D-0035 point 5 for why this
//!   specifically requires human cryptographic authorship, not AI code.
//! - **(c) Physical-presence attestation** — already `mini_presence::PresenceVerdict`,
//!   the whitepaper's named *strongest* signal.
//!
//! [`confidence::fuse_confidence`] fuses exactly these three fixed signals,
//! matching the whitepaper's original description. [`status`] generalizes
//! this into an **open-ended** multi-signal system per later founder
//! direction: any number of verification methods (Mininet's own, or
//! external/future ones) can each contribute weighted evidence toward one
//! [`status::HumanStatus`], with an identity starting `Unverified`,
//! reaching `VouchedHuman` quickly from modest trusted evidence, and being
//! promoted to `EvidenceQualifiedHuman` only automatically — requiring a high fused
//! score, several distinct live sources, and a minimum elapsed time since
//! first evidence all at once. This sidesteps needing any single signal
//! (particularly the still-unimplemented behavioral-entropy one) to be a
//! cryptographic silver bullet: Sybil resistance instead comes from
//! stacking independent, individually-costly-to-fake signals plus a
//! mandatory re-earning window, the same "no longer cheap" property the
//! whitepaper describes (SS11), generalized beyond exactly three signals.
//!
//! ## What this crate deliberately does not decide
//!
//! Seed-set governance (who is in the founding cohort, how that set's
//! influence dilutes as the graph grows — whitepaper SS12), the acceptance
//! thresholds at which status changes, and the exact trust/fusion weights
//! are all left as caller-supplied parameters, not frozen here. This crate
//! provides the verified, tested primitives; calibrating them against a
//! real network is separate, later work.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod confidence;
mod error;
mod graph;
mod status;
mod verify;
mod vouch;

pub use confidence::{
    fuse_confidence, BehavioralEntropySource, ConfidenceInputs, ConfidenceWeights, DecayPolicy,
    NoEntropySource,
};
pub use error::{Result, UniquenessError};
pub use graph::{recommended_iterations, trust_scores, VouchGraph, TRUST_SCALE};
pub use status::{
    HumanRecord, HumanStatus, PromotionPolicy, SignalEvidence, SignalSource, TrustWeights,
};
pub use verify::{verify_vouch, InMemoryReplayGuard, ReplayGuard, VerifyContext, VouchVerdict};
pub use vouch::{VouchAttestation, VouchFields, VoucherParty, VOUCH_VERSION};

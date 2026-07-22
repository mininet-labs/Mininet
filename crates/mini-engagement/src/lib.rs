//! `mini-engagement` -- the escrowed engagement state machine, the general
//! work primitive FD-18's edge/provider layer builds on top of.
//!
//! Founder Directive 18 (D-0402, D-0352), FD-18 Part II.2. This crate is a
//! LEAF: no core crate may ever depend on it. It is deliberately generic --
//! there is no `CardIssuance` variant, no `Courier` variant, no per-industry
//! logic (non-negotiable #10). Any edge service that needs escrowed,
//! milestone-releasable work (a conversion provider, a courier, a
//! professional service) uses this same [`EngagementState`] shape.
//!
//! FD-05 applies unchanged: **a signed promise is never final ownership.**
//! [`Engagement::escrow_claim`] is a real `mini_settlement::PaymentClaim`;
//! this crate tracks how much of it has been released through which state
//! transitions, but actually submitting a release through
//! `mini_settlement::reconcile` against a `CanonicalLedgerView` -- so a
//! release becomes canonical, not just locally recorded -- is separate,
//! later wiring, honestly not built here.
//!
//! [`transitions::timeout`] is the one obligation encoded as a function
//! rather than left to a caller's discipline: every non-terminal state has
//! an edge back to the payer once `Engagement::deadline_ms` passes, so a
//! provider that disappears mid-engagement cannot strand funds (FD-02,
//! FD-06).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod state;
mod transitions;

pub use error::{EngagementError, Result};
pub use state::{Engagement, EngagementState, Party};
pub use transitions::{accept, complete, dispute, release_milestone, timeout};

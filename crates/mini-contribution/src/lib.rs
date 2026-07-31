//! `mini-contribution` -- the Contribution and Settlement Coordinator
//! (D-0417): composes already-shipped `mini-media`/`mini-provider`/
//! `mini-engagement`/`mini-storage`/`mini-settlement` primitives into one
//! publish -> seed -> request -> deliver -> receipt -> settle -> reward
//! lifecycle for content/resources. See
//! `docs/design/contribution-and-settlement-coordinator.md` for the full
//! doctrine and the honest limits this crate does not solve.
//!
//! LEAF crate: no network client, no signing key material of its own
//! beyond composing an already-supplied `mini_crypto::SigningKey` into
//! `mini-settlement`'s existing signing functions, and it must never
//! depend on `mini-forge` or `mini-chain` voting (P1, the voice/value
//! wall).
//!
//! Every payout this crate ever builds is funded exclusively by the
//! requester's own existing balance -- never treasury, never new issuance
//! -- and only from a verified `mini_storage::ServeVerdict`
//! ([`bind_delivery_evidence`]). The reward-evidence floor is exactly
//! `mini_storage::verify_serve`'s existing checks; this crate does not
//! raise it, and it does not solve the open personhood/Sybil question
//! (roadmap #18).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod evidence;
mod offer;
mod role;
mod settle;
mod split;

pub use error::{ContributionError, Result};
pub use evidence::{bind_delivery_evidence, DeliveryEvidence};
pub use offer::ContributionOffer;
pub use role::DeliveryRole;
pub use settle::settle_completed_engagement;
pub use split::{split_amount, PayeeShare, RewardSplit};

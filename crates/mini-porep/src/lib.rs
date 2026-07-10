//! Real proof-of-replication (PoRep): Filecoin-style Stacked Depth-Robust
//! Graph (SDR) sealing, coded here in-house from the published, peer-
//! reviewed, real-world-deployed construction rather than depended on as a
//! library (D-0063) -- closing roadmap [#31](../../issues/31).
//!
//! ## Why this crate exists
//!
//! `mini_spacetime::storage_proof` already proves *possession*: a node
//! that answers a Merkle/PDP challenge must genuinely hold the bytes it
//! claims. What that scheme's own docs name explicitly as an open gap is
//! *replication uniqueness*: it cannot tell a thousand honest small
//! devices each holding their own copy apart from one well-resourced
//! warehouse machine holding a single copy and answering every challenge
//! on behalf of many claimed identities -- exactly the attack the
//! whitepaper's "a thousand cheap, scattered machines outcompete one
//! warehouse" thesis depends on resisting. This crate closes that gap:
//! [`seal`] transforms data into a replica through work that is
//! genuinely, provably sequential and depth-robust (see [`drg`] and
//! [`seal`]'s own module docs for the construction), so producing `k`
//! replicas actually costs approximately `k` times the sealing work --
//! there is no shortcut a warehouse can take to cheaply fake holding many
//! independent copies.
//!
//! ## The three pieces
//!
//! - [`seal`] -- [`seal::seal`] does the one-time sealing work: stacked
//!   layered labeling over a [`drg`]-generated depth-robust graph, then a
//!   final XOR encoding. Produces a [`seal::SealedReplica`] and a public
//!   [`seal::SealCommitment`].
//! - [`audit`] -- the registration-time probabilistic audit
//!   ([`audit::sample_challenges`], [`audit::answer_challenge`],
//!   [`audit::verify_audit_response`]): the honest substitute for a
//!   zk-SNARK sealing circuit, which was judged too large and too risky to
//!   build correctly from scratch in this pass. See that module's own docs
//!   for exactly what tradeoff this makes (non-ZK: it reveals plaintext
//!   intermediate labels for challenged indices, which is fine here since
//!   sealing isn't trying to keep data confidential).
//! - [`challenge`] -- ongoing possession challenge-response, composing
//!   `mini_spacetime`'s existing PDP machinery against the sealed
//!   replica's root rather than duplicating it.
//!   [`challenge::PorepStorageProof`] implements
//!   `mini_spacetime::ProofOfSpaceTimeSource`, so
//!   `mini_spacetime::proposer_weight` needs zero changes to consume proof
//!   sourced from real replication instead of mere possession.
//!
//! ## Honest limits
//!
//! - This is a **simplified** SDR construction (see [`drg`]'s module docs):
//!   structurally similar to, but not parameter-identical with, Filecoin's
//!   production `BucketGraph` sampling distribution.
//! - The registration audit is **probabilistic, not a succinct proof**:
//!   sampling enough challenges makes skipping a meaningful fraction of the
//!   sealing work exponentially unlikely to go undetected, but (unlike a
//!   SNARK) it is not a single, small, universally-checkable proof, and it
//!   reveals plaintext intermediate labels for every challenged index.
//! - **Unaudited.** Real, tested, founder-reviewed AI-authored cryptography
//!   prototype code -- not audit-equivalent. Gated behind D-0047 before any
//!   real value depends on it, the same posture every other `mini-value`/
//!   `mini-treasury` prototype in this tree already carries.
//! - Sealing an entire large file through many stacked layers is real CPU
//!   work by design (that is the point) -- this crate makes no attempt at
//!   GPU/hardware acceleration; that is a later, separate concern from the
//!   construction's correctness.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod audit;
mod challenge;
mod drg;
mod error;
mod seal;

pub use audit::{
    answer_challenge, sample_challenges, verify_audit_response, AuditChallenge, AuditResponse,
};
pub use challenge::{replica_commitment, respond, PorepStorageProof};
pub use drg::{parents, DRG_DEGREE};
pub use error::{PorepError, Result};
pub use seal::{seal, SealCommitment, SealParams, SealedReplica, NODE_SIZE};

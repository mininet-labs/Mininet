//! Cross-identity storage-fraud detection (roadmap [issue #42](https://github.com/mininet-labs/Mininet/issues/42),
//! Phase 5.7, D-0437): signed [`StorageCommitmentClaim`]s and
//! [`CollisionEvidence`] proving two distinct identity roots each
//! published the identical `mini_porep` replica commitment -- direct,
//! cryptographically checkable evidence of single-copy sharing/collusion,
//! since `mini_porep::seal`'s own DRG construction guarantees two
//! genuinely independent, honest sealers under distinct identity-bound
//! `replica_id`s can never end up at the same committed Merkle root.
//!
//! This crate mirrors `mini_consensus::evidence::EquivocationEvidence`'s
//! own restrained scope exactly: it detects and proves, and stops there.
//! No penalty, no exclusion, no reward clawback, no consensus authority --
//! see `docs/design/storage-fraud-detection.md` for the full doctrine,
//! including what this deliberately does *not* attempt (network-timing/
//! latency-based "fast fetch, not genuine possession" fraud, which needs a
//! live network deployment to calibrate against and is not composed from
//! already-reviewed primitives the way collision evidence is).
//!
//! No crate outside `mini-storage-fraud` depends on it; it creates no
//! voice/value wall edge (only `did-mini`/`mini-crypto`/`mini-porep`/
//! `mini-spacetime` dependencies) and no generic `sign(bytes)` surface --
//! [`StorageCommitmentClaim::issue`] takes a specific typed request, not
//! raw bytes.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod codec;
mod collision;
mod commitment_claim;
mod error;

pub use collision::{verify_collision, CollisionEvidence};
pub use commitment_claim::{
    derive_replica_id, StorageCommitmentClaim, STORAGE_COMMITMENT_CLAIM_VERSION,
};
pub use error::{FraudError, Result};

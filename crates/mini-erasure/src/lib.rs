//! Erasure coding and self-healing storage, closing roadmap
//! [#30](../../issues/30) (erasure coding & replication strategy) and
//! [#32](../../issues/32) (self-healing storage design).
//!
//! ## Why erasure coding, not just replication
//!
//! Plain replication (`N` full copies of a file) tolerates `N - 1` losses
//! at a storage cost of `N x`. Systematic Reed-Solomon erasure coding
//! ([`code`]) splits a file into `data_shards` pieces and computes
//! `parity_shards` additional pieces, tolerating up to `parity_shards`
//! losses at a storage cost of only `(data_shards + parity_shards) /
//! data_shards x` — for typical parameters (e.g. 10 data + 4 parity)
//! this is dramatically cheaper than replication for the same loss
//! tolerance, the standard reason every large-scale storage system
//! (RAID6, Backblaze, Ceph, IPFS's own optional erasure coding) uses it
//! instead of naive copies.
//!
//! [`matrix`]/[`gf256`] implement the standard `GF(2^8)`-based systematic
//! Reed-Solomon construction — the same field and Vandermonde-generator-
//! matrix technique described in RFC 5510 and used by QR codes, RAID6,
//! and every production erasure-coding library. Coded in-house here for
//! the same reason D-0063 gives for `mini-porep`'s cryptography: composing
//! an already-published, real-world-deployed construction ourselves keeps
//! it inside this repo's own governance boundary rather than depending on
//! an external crate indefinitely. Erasure coding is coding theory, not
//! cryptography, so CLAUDE.md's crypto-invention rule doesn't technically
//! apply here — but the same Directive-14 "prefer the well-trodden
//! construction" reasoning does, and is followed the same way.
//!
//! ## Self-healing
//!
//! [`health`] is what turns loss-tolerance into actual healing: given
//! which shards a network of holders can currently vouch for (present
//! *and* passing a BLAKE3 integrity check — corruption is treated the same
//! as absence, never silently trusted), [`health::plan_repair`] reports
//! what's missing and whether enough shards survive to recover at all, and
//! [`health::repair`] reconstructs the original data and regenerates
//! exactly the missing shards, ready for a caller to redistribute.
//!
//! **Scope boundary:** this crate proves the erasure-coding and repair
//! *logic* is correct. Deciding which peer should hold a regenerated
//! shard and transferring it to them is `mini-net`/`mini-store`'s job — a
//! distribution problem, not a coding-theory one — and is not attempted
//! here, unstarted.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod code;
mod error;
mod gf256;
mod health;
mod matrix;

pub use code::{encode, reconstruct, EncodedData, ErasureParams, Shard};
pub use error::{ErasureError, Result};
pub use health::{digest, plan_repair, repair, verify_shard, RepairPlan};

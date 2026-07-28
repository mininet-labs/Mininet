//! Suppression-resistant replication policy (D-0311/D-0312 doctrine,
//! `docs/research/
//! MININET_NATIVE_INTAKE_PUBLIC_COMMONS_AND_OPEN_WEB_SEARCH_20260718.md`
//! §27, "PR D5 — Suppression-resistant replication: connect erasure
//! coding, provider diversity, repair, and retrieval").
//!
//! [`mini_erasure`] already proves the coding-theory half of this: split a
//! file into shards, tolerate losing some of them, detect and regenerate
//! exactly what went missing. Its own module docs name the gap this crate
//! closes verbatim: *"deciding which peer should hold a regenerated
//! shard... is a distribution problem, not a coding-theory one, and is not
//! attempted here."* This crate is that distribution *policy* — never the
//! distribution itself:
//!
//! - [`plan_placement`] assigns each shard of a [`mini_erasure::ErasureParams`]
//!   encoding to its own distinct [`did_mini::Did`] holder, so no single
//!   holder's removal, freeze, or coercion can cost more than one shard;
//! - [`plan_repair_placement`] replaces exactly the holders of shards
//!   [`mini_erasure::health::plan_repair`] found missing, with fresh
//!   holders distinct from everyone still holding a shard in the same
//!   plan — preserving the diversity invariant across repairs, not just at
//!   first publication;
//! - [`select_retrieval_set`] picks a deterministic default subset of
//!   holders a retrieval client can query to reconstruct the original data.
//!
//! **What this crate deliberately does not do:** no network I/O, no
//! shard-byte handling (that stays in [`mini_erasure`]), no signing or
//! transport (that is `mini-relay`/`mini-bridge`/`mini-net`'s job, per the
//! same scope boundary `mini-erasure`'s own docs draw), and no judgment
//! about *which* candidates are trustworthy — a caller supplies the
//! candidate `Did`s (e.g. from a discovery layer this crate does not
//! define) and this crate only enforces that whoever is chosen ends up
//! holding at most one shard each. No new cryptography: identity
//! comparison is ordinary `Did` equality, already defined by `did-mini`.
//!
//! **Honest limit, stated once here rather than at every call site:**
//! distinctness is checked by `Did` equality alone. A single operator
//! controlling many `Did`s defeats the diversity property this crate
//! provides — the same Sybil-resistance gap `docs/INVARIANTS.md`'s hard
//! limitations already name project-wide, not something a placement policy
//! can solve on its own.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod placement;

pub use error::{ReplicationError, Result};
pub use placement::{
    plan_placement, plan_repair_placement, select_retrieval_set, HolderId, ReplicationPlan,
    ShardAssignment,
};

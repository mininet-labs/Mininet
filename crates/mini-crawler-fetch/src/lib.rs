//! Bounded HTTP(S) execution for MiniSearch crawl requests.
//!
//! `mini-crawler` remains the deterministic frontier and admission core. This
//! crate executes one already-admitted request. It manually follows redirects,
//! resolves and validates every hop before connecting, pins the approved DNS
//! result into the HTTP client, requests identity transfer encoding, and stops
//! reading at a hard body limit. It does not parse robots.txt, execute
//! JavaScript, persist a frontier, extract HTML, index, rank, or pay a crawler.
//!
//! The caller must supply an explicit robots decision. `Unknown` fails closed:
//! a scheduler must fetch/cache robots policy before ordinary page execution.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod address;
mod backend;
mod policy;
mod runtime;

pub use address::{address_is_public, validate_resolved_addresses};
pub use backend::{BackendError, BackendFuture, FetchBackend, RawResponse, ReqwestBackend};
pub use policy::{FetchLimits, RobotsDecision};
pub use runtime::{derive_observation_id, FetchOutcome, FetchRuntime, RuntimeError};

//! `TransportRequest` policy router (D-0301, L2 of `docs/design/
//! privacy-cost-doctrine-parallel-execution-plan.md`, closes tracking
//! issue #134 / `MN-201`).
//!
//! Routing *decisions* only — see [`router`]'s module doc for the honest
//! limits (no transport, no socket, declared cost not measured cost).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod router;

pub use error::{Result, TransportPolicyError};
pub use router::{route, PayloadSizeClass, RouteDecision, TransportRequest};

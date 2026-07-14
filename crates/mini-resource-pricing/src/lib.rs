//! Resource price vector and quote engine (D-0302, L4 of `docs/design/
//! privacy-cost-doctrine-parallel-execution-plan.md`, closes tracking
//! issue #136 / `MN-601`).
//!
//! Quoting logic only — see [`quote`]'s module doc for the honest limits
//! (no payment execution, no e-cash, no ledger write).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod error;
mod quote;

pub use error::{PricingError, Result};
pub use quote::{quote, PriceVector, Quote};

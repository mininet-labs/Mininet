//! Deterministic monetary-policy primitives for MINI.
//!
//! This crate turns the accepted D-0074 issuance envelope into integer-only
//! executable policy. It does not move funds, establish personhood, choose
//! service winners, verify external contributions, or activate genesis.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod amount;
mod error;
mod genesis;
mod issuance;

pub use amount::Amount;
pub use error::{EconomyError, Result};
pub use genesis::{build_genesis, GenesisManifest, GenesisPolicy};
pub use issuance::{
    plan_epoch, plan_human_share, Allocation, Channel, EpochPlan, EpochRequest, HumanSharePlan,
    HumanSnapshot, IssuancePolicy, VestingGrant, MILLION, YEAR_MS,
};

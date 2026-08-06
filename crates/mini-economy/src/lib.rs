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
mod ledger;
mod scalable_epoch_wire;
mod snapshot;

pub use amount::Amount;
pub use error::{EconomyError, Result};
pub use genesis::{build_genesis, GenesisManifest, GenesisPolicy};
pub use issuance::{
    plan_epoch, plan_human_share, plan_scalable_epoch, Allocation, Channel, EpochPlan,
    EpochRequest, HumanSharePlan, HumanSnapshot, IssuancePolicy, ScalableEpochPlan,
    ScalableEpochRequest, VestingGrant, MILLION, YEAR_MS,
};
pub use ledger::{MonetaryLedger, VestingPosition, VestingSubject};
pub use scalable_epoch_wire::{
    MAX_EPOCH_BENEFICIARY_BYTES, MAX_SCALABLE_EPOCH_GRANTS, MAX_SCALABLE_EPOCH_PLAN_BYTES,
};
pub use snapshot::{MonetaryLedgerSnapshot, MAX_SNAPSHOT_BENEFICIARY_BYTES, MAX_VESTING_POSITIONS};

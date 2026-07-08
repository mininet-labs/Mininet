//! The seam a platform shell fills in to supply hardware ranging evidence.
//!
//! This crate cannot reach a phone's UWB stack itself — that lives behind
//! platform APIs (iOS Nearby Interaction, Android's UWB APIs) only a native
//! shell can call (D-0020's UniFFI architecture: the Rust core defines the
//! trait and result type, each platform shell supplies the measurement).
//! [`RangingSource`] is that seam, mirroring how [`crate::attestation`]'s
//! `TransportKind` and `mini_bearer::Bearer` define traits for adapters that
//! don't exist in this repo yet.
//!
//! ## Honest limit
//!
//! No real implementation ships here. [`NoUwb`] is the reference/fallback
//! implementation — every device works correctly with it (software RTT bound
//! only, per [`crate::verify::RangePolicy::max_uwb_distance_cm`] staying
//! `None`); a real platform-backed `RangingSource` is `pending`, the same
//! honest-limit shape `mini-bearer`'s real BLE adapter has carried since
//! D-0015.

use crate::attestation::UwbRanging;

/// A source of hardware ranging measurements for one side of a presence
/// session. Implementations live in platform shells, not this crate.
pub trait RangingSource {
    /// Attempt a hardware ranging measurement against the peer this presence
    /// session is with. `Ok(None)` means no UWB chip/measurement is
    /// available for this session — a normal, unremarkable outcome that
    /// falls back to the software RTT bound alone. `Err` is reserved for a
    /// source that should have a measurement but failed to obtain one.
    fn range(&mut self) -> Result<Option<UwbRanging>, RangingError>;
}

/// Why a [`RangingSource`] failed to produce a measurement it expected to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangingError;

impl core::fmt::Display for RangingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "hardware ranging measurement failed")
    }
}

impl std::error::Error for RangingError {}

/// The reference [`RangingSource`]: no hardware chip, always falls back to
/// the software RTT bound. Every device is a `NoUwb` device until a platform
/// shell supplies a real one — this is not a stub to delete later, it is the
/// permanent, correct behavior for devices without a UWB chip.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoUwb;

impl RangingSource for NoUwb {
    fn range(&mut self) -> Result<Option<UwbRanging>, RangingError> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_uwb_always_falls_back_to_none() {
        let mut source = NoUwb;
        assert_eq!(source.range(), Ok(None));
    }
}

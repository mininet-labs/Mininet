use crate::{EconomyError, Result};

/// Atomic MINI units. Existing `u64` micro-MINI APIs remain wire-compatible;
/// this wider core type gives century-scale supply accounting headroom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(u128);

impl Amount {
    pub const ZERO: Self = Self(0);
    pub const MICRO_PER_MINI: u128 = 1_000_000;

    pub const fn from_micro(value: u128) -> Self {
        Self(value)
    }

    pub const fn as_micro(self) -> u128 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Result<Self> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(EconomyError::Overflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(EconomyError::Underflow)
    }
}

impl From<u64> for Amount {
    fn from(value: u64) -> Self {
        Self(value as u128)
    }
}

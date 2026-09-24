//! Deterministic *target* schedule, not a wall-clock consensus rule.
//! A driver may aim for short settlement rounds while Human Share release uses
//! a slower cadence. Only finalized rounds advance canonical state; elapsed time
//! alone must never fabricate blocks, maturity, issuance or payment finality.

use crate::{EconomyError, Result, YEAR_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementCadence {
    target_round_ms: u64,
    rounds_per_issuance: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementTarget {
    pub round: u64,
    /// Target elapsed time since the host's monotonic scheduling origin.
    pub elapsed_ms: u64,
    /// Zero-based issuance epoch proposed at this finalized-round boundary.
    pub issuance_epoch: Option<u64>,
}

impl SettlementCadence {
    pub fn new(target_round_ms: u64, rounds_per_issuance: u64) -> Result<Self> {
        let duration = target_round_ms
            .checked_mul(rounds_per_issuance)
            .ok_or(EconomyError::Overflow)?;
        if target_round_ms == 0 || rounds_per_issuance == 0 || duration > YEAR_MS {
            return Err(EconomyError::InvalidDuration);
        }
        Ok(Self {
            target_round_ms,
            rounds_per_issuance,
        })
    }
    pub fn target_round_ms(self) -> u64 {
        self.target_round_ms
    }
    pub fn issuance_duration_ms(self) -> u64 {
        self.target_round_ms * self.rounds_per_issuance
    }
    /// One next target, never a loop that mints missed epochs after an outage.
    /// Governance must separately approve any mapping to canonical policy time.
    pub fn next_after(self, finalized_round: u64) -> Result<SettlementTarget> {
        let round = finalized_round
            .checked_add(1)
            .ok_or(EconomyError::Overflow)?;
        let elapsed_ms = round
            .checked_mul(self.target_round_ms)
            .ok_or(EconomyError::Overflow)?;
        let issuance_epoch = if round % self.rounds_per_issuance == 0 {
            Some(round / self.rounds_per_issuance - 1)
        } else {
            None
        };
        Ok(SettlementTarget {
            round,
            elapsed_ms,
            issuance_epoch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_settlement_does_not_mint_every_round() {
        let cadence = SettlementCadence::new(4_000, 21_600).unwrap();
        assert_eq!(
            cadence.next_after(0).unwrap(),
            SettlementTarget {
                round: 1,
                elapsed_ms: 4_000,
                issuance_epoch: None
            }
        );
        assert_eq!(cadence.issuance_duration_ms(), 86_400_000);
        assert_eq!(cadence.next_after(21_599).unwrap().issuance_epoch, Some(0));
        assert_eq!(cadence.next_after(43_199).unwrap().issuance_epoch, Some(1));
        assert_eq!(cadence.next_after(21_600).unwrap().issuance_epoch, None);
    }
    #[test]
    fn invalid_and_overflow_schedules_fail_closed() {
        assert!(SettlementCadence::new(0, 1).is_err());
        assert!(SettlementCadence::new(1, 0).is_err());
        assert!(SettlementCadence::new(YEAR_MS, 2).is_err());
        assert!(SettlementCadence::new(u64::MAX, 2).is_err());
        assert!(SettlementCadence::new(4_000, 1)
            .unwrap()
            .next_after(u64::MAX)
            .is_err());
        assert!(SettlementCadence::new(4_000, 1)
            .unwrap()
            .next_after(u64::MAX / 4_000)
            .is_err());
    }
}

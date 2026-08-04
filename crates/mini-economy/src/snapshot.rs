//! Validated export/import of the deterministic monetary ledger state.
//!
//! This is a local state-transfer vocabulary, not a minting or governance
//! authority. Import reconstructs only a state that could satisfy the same
//! supply/vesting invariants as an ordinarily executed [`MonetaryLedger`].

use crate::{Amount, EconomyError, MonetaryLedger, Result, VestingPosition, VestingSubject};

/// Hard cap on vesting positions carried by one state snapshot.
///
/// The exact-state codec in `mini-execution` applies an independent byte cap;
/// this count cap prevents a syntactically tiny hostile input from requesting
/// an unbounded allocation before those positions are validated.
pub const MAX_VESTING_POSITIONS: usize = 65_536;

/// Maximum UTF-8 bytes in a beneficiary name restored from a snapshot.
pub const MAX_SNAPSHOT_BENEFICIARY_BYTES: usize = 4_096;

/// A complete, non-secret representation of [`MonetaryLedger`] state.
///
/// Construction is public so a decoder in another crate can populate the
/// fields, but only [`MonetaryLedger::import_snapshot`] turns it into live
/// monetary state. That import validates the aggregate issued amount,
/// position bounds, subject shape, epoch bounds, and arithmetic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonetaryLedgerSnapshot {
    pub genesis_circulating: Amount,
    pub total_issued: Amount,
    pub policy_time_ms: u128,
    pub last_epoch: Option<u64>,
    pub positions: Vec<VestingPosition>,
}

impl MonetaryLedger {
    /// Export the exact deterministic monetary state for authenticated local
    /// persistence or consensus snapshot transport.
    pub fn export_snapshot(&self) -> MonetaryLedgerSnapshot {
        MonetaryLedgerSnapshot {
            genesis_circulating: self.genesis_circulating,
            total_issued: self.total_issued,
            policy_time_ms: self.policy_time_ms,
            last_epoch: self.last_epoch,
            positions: self.positions.clone(),
        }
    }

    /// Reconstruct monetary state after validating every snapshot invariant.
    ///
    /// This does not authorize issuance. The snapshot must later be bound to a
    /// quorum-finalized block header by the consensus/execution layers.
    pub fn import_snapshot(snapshot: MonetaryLedgerSnapshot) -> Result<Self> {
        if snapshot.positions.len() > MAX_VESTING_POSITIONS {
            return Err(EconomyError::InvalidSnapshot);
        }

        if snapshot.last_epoch.is_none()
            && (snapshot.total_issued != Amount::ZERO
                || snapshot.policy_time_ms != 0
                || !snapshot.positions.is_empty())
        {
            return Err(EconomyError::InvalidSnapshot);
        }

        let mut issued_from_positions = Amount::ZERO;
        let mut greatest_epoch = None;
        for position in &snapshot.positions {
            validate_position(position, snapshot.policy_time_ms)?;
            issued_from_positions = issued_from_positions.checked_add(position.amount)?;
            greatest_epoch =
                Some(greatest_epoch.map_or(position.epoch, |epoch: u64| epoch.max(position.epoch)));
        }
        if issued_from_positions != snapshot.total_issued {
            return Err(EconomyError::InvalidSnapshot);
        }
        if let (Some(greatest), Some(last)) = (greatest_epoch, snapshot.last_epoch) {
            if greatest > last {
                return Err(EconomyError::InvalidSnapshot);
            }
        }

        let ledger = MonetaryLedger {
            genesis_circulating: snapshot.genesis_circulating,
            total_issued: snapshot.total_issued,
            policy_time_ms: snapshot.policy_time_ms,
            last_epoch: snapshot.last_epoch,
            positions: snapshot.positions,
        };

        // Force all checked arithmetic paths once before the state is accepted.
        let _ = ledger.total_supply()?;
        let _ = ledger.circulating_supply()?;
        let _ = ledger.locked_supply()?;
        Ok(ledger)
    }
}

fn validate_position(position: &VestingPosition, policy_time_ms: u128) -> Result<()> {
    if position.starts_at_policy_ms > policy_time_ms {
        return Err(EconomyError::InvalidSnapshot);
    }
    match &position.subject {
        VestingSubject::HumanSnapshot(snapshot) => {
            if snapshot.root == [0; 32] || snapshot.eligible_count == 0 {
                return Err(EconomyError::InvalidSnapshot);
            }
        }
        VestingSubject::Beneficiary(beneficiary) => {
            if beneficiary.is_empty() || beneficiary.len() > MAX_SNAPSHOT_BENEFICIARY_BYTES {
                return Err(EconomyError::InvalidSnapshot);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, HumanSnapshot};

    fn one_position() -> VestingPosition {
        VestingPosition {
            epoch: 0,
            subject: VestingSubject::HumanSnapshot(HumanSnapshot {
                root: [7; 32],
                eligible_count: 10,
            }),
            channel: Channel::HumanShare,
            amount: Amount::from_micro(500),
            starts_at_policy_ms: 1_000,
            duration_ms: 10_000,
        }
    }

    #[test]
    fn export_import_round_trips_genesis() {
        let ledger = MonetaryLedger::new(Amount::from_micro(123));
        let restored = MonetaryLedger::import_snapshot(ledger.export_snapshot()).unwrap();
        assert_eq!(restored, ledger);
        assert_eq!(restored.commitment(), ledger.commitment());
    }

    #[test]
    fn a_valid_position_snapshot_imports() {
        let snapshot = MonetaryLedgerSnapshot {
            genesis_circulating: Amount::from_micro(1_000),
            total_issued: Amount::from_micro(500),
            policy_time_ms: 1_000,
            last_epoch: Some(0),
            positions: vec![one_position()],
        };
        let ledger = MonetaryLedger::import_snapshot(snapshot.clone()).unwrap();
        assert_eq!(ledger.export_snapshot(), snapshot);
    }

    #[test]
    fn issued_total_must_equal_positions() {
        let mut snapshot = MonetaryLedgerSnapshot {
            genesis_circulating: Amount::from_micro(1_000),
            total_issued: Amount::from_micro(499),
            policy_time_ms: 1_000,
            last_epoch: Some(0),
            positions: vec![one_position()],
        };
        assert_eq!(
            MonetaryLedger::import_snapshot(snapshot.clone()),
            Err(EconomyError::InvalidSnapshot)
        );
        snapshot.total_issued = Amount::from_micro(500);
        snapshot.positions[0].starts_at_policy_ms = 1_001;
        assert_eq!(
            MonetaryLedger::import_snapshot(snapshot),
            Err(EconomyError::InvalidSnapshot)
        );
    }

    #[test]
    fn malformed_subjects_are_rejected() {
        let mut position = one_position();
        position.subject = VestingSubject::Beneficiary(String::new());
        let snapshot = MonetaryLedgerSnapshot {
            genesis_circulating: Amount::from_micro(1_000),
            total_issued: position.amount,
            policy_time_ms: 1_000,
            last_epoch: Some(0),
            positions: vec![position],
        };
        assert_eq!(
            MonetaryLedger::import_snapshot(snapshot),
            Err(EconomyError::InvalidSnapshot)
        );
    }
}

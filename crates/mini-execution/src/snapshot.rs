//! Canonical, bounded serialization of the complete deterministic ledger state.
//!
//! A decoded state is not authoritative merely because it parses. Consensus
//! snapshots bind these bytes to a quorum-finalized block header whose
//! `state_root` equals [`LedgerState::commitment`]. This module only provides
//! exact-state persistence plus strict structural validation.

use std::collections::BTreeMap;

use mini_economy::{
    Amount, Channel, HumanSnapshot, MonetaryLedger, MonetaryLedgerSnapshot, VestingPosition,
    VestingSubject, MAX_SNAPSHOT_BENEFICIARY_BYTES, MAX_VESTING_POSITIONS,
};
use mini_settlement::CanonicalRejection;

use crate::error::{ExecutionError, Result};
use crate::state::{is_supported_account, LedgerState};

/// Maximum bytes in one exact ledger-state snapshot.
///
/// This leaves substantial room inside the bearer/channel frame for a finality
/// certificate and response framing. Larger production state needs a future
/// chunked/Merkle snapshot format; it must not silently bypass this bound.
pub const MAX_LEDGER_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

/// Maximum rows in each map encoded by one snapshot.
pub const MAX_LEDGER_SNAPSHOT_ENTRIES: usize = 65_536;

const DOMAIN: &[u8] = b"mini-execution/ledger-snapshot/v1";

impl LedgerState {
    /// Encode the entire deterministic state in one canonical byte sequence.
    pub fn to_snapshot_bytes(&self) -> Result<Vec<u8>> {
        for count in [self.finalized.len(), self.rejected.len(), self.balances.len()] {
            if count > MAX_LEDGER_SNAPSHOT_ENTRIES {
                return Err(ExecutionError::SnapshotTooLarge);
            }
        }

        let monetary = self.monetary.export_snapshot();
        if monetary.positions.len() > MAX_VESTING_POSITIONS {
            return Err(ExecutionError::SnapshotTooLarge);
        }

        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
        out.extend_from_slice(&self.network_id);

        put_count(&mut out, self.finalized.len())?;
        for (payer, (sequence, digest)) in &self.finalized {
            put_bytes(&mut out, payer)?;
            out.extend_from_slice(&sequence.to_be_bytes());
            out.extend_from_slice(digest);
        }

        put_count(&mut out, self.rejected.len())?;
        for (digest, reason) in &self.rejected {
            out.extend_from_slice(digest);
            out.push(rejection_tag(*reason));
        }

        encode_monetary(&mut out, &monetary)?;

        put_count(&mut out, self.balances.len())?;
        for (account, amount) in &self.balances {
            put_bytes(&mut out, account)?;
            out.extend_from_slice(&amount.as_micro().to_be_bytes());
        }
        out.extend_from_slice(&self.allocated_circulating.as_micro().to_be_bytes());
        out.extend_from_slice(&self.unallocated_circulating.as_micro().to_be_bytes());

        if out.len() > MAX_LEDGER_SNAPSHOT_BYTES {
            return Err(ExecutionError::SnapshotTooLarge);
        }
        Ok(out)
    }

    /// Decode and independently validate exact ledger state.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_LEDGER_SNAPSHOT_BYTES {
            return Err(ExecutionError::SnapshotTooLarge);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(DOMAIN.len())? != DOMAIN {
            return Err(ExecutionError::SnapshotMalformed);
        }
        let network_id = reader.array_32()?;

        let finalized_count = reader.count(MAX_LEDGER_SNAPSHOT_ENTRIES)?;
        let mut finalized = BTreeMap::new();
        let mut previous_account: Option<Vec<u8>> = None;
        for _ in 0..finalized_count {
            let payer = reader.bytes(crate::MAX_ACCOUNT_BYTES)?.to_vec();
            if !is_supported_account(&payer) || !strictly_after(&previous_account, &payer) {
                return Err(ExecutionError::SnapshotMalformed);
            }
            previous_account = Some(payer.clone());
            let sequence = reader.u64()?;
            let digest = reader.array_32()?;
            finalized.insert(payer, (sequence, digest));
        }

        let rejected_count = reader.count(MAX_LEDGER_SNAPSHOT_ENTRIES)?;
        let mut rejected = BTreeMap::new();
        let mut previous_digest: Option<[u8; 32]> = None;
        for _ in 0..rejected_count {
            let digest = reader.array_32()?;
            if previous_digest.is_some_and(|previous| previous >= digest) {
                return Err(ExecutionError::SnapshotMalformed);
            }
            previous_digest = Some(digest);
            rejected.insert(digest, decode_rejection(reader.u8()?)?);
        }

        let monetary_snapshot = decode_monetary(&mut reader)?;
        let monetary = MonetaryLedger::import_snapshot(monetary_snapshot)
            .map_err(ExecutionError::InvalidMonetaryEpoch)?;

        let balance_count = reader.count(MAX_LEDGER_SNAPSHOT_ENTRIES)?;
        let mut balances = BTreeMap::new();
        previous_account = None;
        for _ in 0..balance_count {
            let account = reader.bytes(crate::MAX_ACCOUNT_BYTES)?.to_vec();
            if !is_supported_account(&account) || !strictly_after(&previous_account, &account) {
                return Err(ExecutionError::SnapshotMalformed);
            }
            previous_account = Some(account.clone());
            let amount = Amount::from_micro(reader.u128()?);
            if amount == Amount::ZERO {
                return Err(ExecutionError::SnapshotMalformed);
            }
            balances.insert(account, amount);
        }
        let allocated_circulating = Amount::from_micro(reader.u128()?);
        let unallocated_circulating = Amount::from_micro(reader.u128()?);
        if !reader.finished() {
            return Err(ExecutionError::SnapshotMalformed);
        }

        let state = LedgerState {
            network_id,
            finalized,
            rejected,
            monetary,
            balances,
            allocated_circulating,
            unallocated_circulating,
        };
        state.verify_supply_conservation()?;
        state.verify_balance_map_total()?;

        // Re-encoding proves the input was the one canonical representation,
        // not merely one of several byte strings that decode to the same maps.
        if state.to_snapshot_bytes()?.as_slice() != bytes {
            return Err(ExecutionError::SnapshotMalformed);
        }
        Ok(state)
    }
}

fn encode_monetary(out: &mut Vec<u8>, snapshot: &MonetaryLedgerSnapshot) -> Result<()> {
    out.extend_from_slice(&snapshot.genesis_circulating.as_micro().to_be_bytes());
    out.extend_from_slice(&snapshot.total_issued.as_micro().to_be_bytes());
    out.extend_from_slice(&snapshot.policy_time_ms.to_be_bytes());
    match snapshot.last_epoch {
        Some(epoch) => {
            out.push(1);
            out.extend_from_slice(&epoch.to_be_bytes());
        }
        None => out.push(0),
    }
    put_count(out, snapshot.positions.len())?;
    for position in &snapshot.positions {
        out.extend_from_slice(&position.epoch.to_be_bytes());
        match &position.subject {
            VestingSubject::HumanSnapshot(snapshot) => {
                out.push(0);
                out.extend_from_slice(&snapshot.root);
                out.extend_from_slice(&snapshot.eligible_count.to_be_bytes());
            }
            VestingSubject::Beneficiary(beneficiary) => {
                out.push(1);
                put_bytes(out, beneficiary.as_bytes())?;
            }
        }
        out.push(channel_tag(position.channel));
        out.extend_from_slice(&position.amount.as_micro().to_be_bytes());
        out.extend_from_slice(&position.starts_at_policy_ms.to_be_bytes());
        out.extend_from_slice(&position.duration_ms.to_be_bytes());
    }
    Ok(())
}

fn decode_monetary(reader: &mut Reader<'_>) -> Result<MonetaryLedgerSnapshot> {
    let genesis_circulating = Amount::from_micro(reader.u128()?);
    let total_issued = Amount::from_micro(reader.u128()?);
    let policy_time_ms = reader.u128()?;
    let last_epoch = match reader.u8()? {
        0 => None,
        1 => Some(reader.u64()?),
        _ => return Err(ExecutionError::SnapshotMalformed),
    };
    let count = reader.count(MAX_VESTING_POSITIONS)?;
    let mut positions = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let epoch = reader.u64()?;
        let subject = match reader.u8()? {
            0 => VestingSubject::HumanSnapshot(HumanSnapshot {
                root: reader.array_32()?,
                eligible_count: reader.u64()?,
            }),
            1 => {
                let bytes = reader.bytes(MAX_SNAPSHOT_BENEFICIARY_BYTES)?;
                let beneficiary = core::str::from_utf8(bytes)
                    .map_err(|_| ExecutionError::SnapshotMalformed)?
                    .to_string();
                VestingSubject::Beneficiary(beneficiary)
            }
            _ => return Err(ExecutionError::SnapshotMalformed),
        };
        let channel = decode_channel(reader.u8()?)?;
        let amount = Amount::from_micro(reader.u128()?);
        let starts_at_policy_ms = reader.u128()?;
        let duration_ms = reader.u64()?;
        positions.push(VestingPosition {
            epoch,
            subject,
            channel,
            amount,
            starts_at_policy_ms,
            duration_ms,
        });
    }
    Ok(MonetaryLedgerSnapshot {
        genesis_circulating,
        total_issued,
        policy_time_ms,
        last_epoch,
        positions,
    })
}

fn channel_tag(channel: Channel) -> u8 {
    match channel {
        Channel::HumanShare => 0,
        Channel::Service => 1,
        Channel::TreasuryContribution => 2,
    }
}

fn decode_channel(tag: u8) -> Result<Channel> {
    match tag {
        0 => Ok(Channel::HumanShare),
        1 => Ok(Channel::Service),
        2 => Ok(Channel::TreasuryContribution),
        _ => Err(ExecutionError::SnapshotMalformed),
    }
}

fn rejection_tag(reason: CanonicalRejection) -> u8 {
    match reason {
        CanonicalRejection::WrongNetwork => 0,
        CanonicalRejection::UnsupportedPayee => 1,
        CanonicalRejection::StaleSequence => 2,
        CanonicalRejection::InsufficientFunds => 3,
    }
}

fn decode_rejection(tag: u8) -> Result<CanonicalRejection> {
    match tag {
        0 => Ok(CanonicalRejection::WrongNetwork),
        1 => Ok(CanonicalRejection::UnsupportedPayee),
        2 => Ok(CanonicalRejection::StaleSequence),
        3 => Ok(CanonicalRejection::InsufficientFunds),
        _ => Err(ExecutionError::SnapshotMalformed),
    }
}

fn strictly_after(previous: &Option<Vec<u8>>, current: &[u8]) -> bool {
    previous.as_deref().is_none_or(|prior| prior < current)
}

fn put_count(out: &mut Vec<u8>, count: usize) -> Result<()> {
    let count = u32::try_from(count).map_err(|_| ExecutionError::SnapshotTooLarge)?;
    out.extend_from_slice(&count.to_be_bytes());
    Ok(())
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| ExecutionError::SnapshotTooLarge)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(len)
            .ok_or(ExecutionError::SnapshotMalformed)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ExecutionError::SnapshotMalformed)?;
        self.position = end;
        Ok(value)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        let mut bytes = [0; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64> {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(self.take(8)?);
        Ok(u64::from_be_bytes(bytes))
    }

    fn u128(&mut self) -> Result<u128> {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(self.take(16)?);
        Ok(u128::from_be_bytes(bytes))
    }

    fn array_32(&mut self) -> Result<[u8; 32]> {
        let mut bytes = [0; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(bytes)
    }

    fn bytes(&mut self, max: usize) -> Result<&'a [u8]> {
        let len = self.u32()? as usize;
        if len > max {
            return Err(ExecutionError::SnapshotTooLarge);
        }
        self.take(len)
    }

    fn count(&mut self, max: usize) -> Result<usize> {
        let count = self.u32()? as usize;
        if count > max {
            return Err(ExecutionError::SnapshotTooLarge);
        }
        Ok(count)
    }

    fn finished(&self) -> bool {
        self.position == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use mini_crypto::SigningKey;
    use mini_settlement::sign_claim;

    use super::*;
    use crate::{apply_block, SettlementBlockBody};

    fn account(seed: u8) -> Vec<u8> {
        SigningKey::from_seed(&[seed; 32])
            .verifying_key()
            .to_bytes()
            .to_vec()
    }

    fn populated_state() -> LedgerState {
        let payer = SigningKey::from_seed(&[11; 32]);
        let payer_account = payer.verifying_key().to_bytes().to_vec();
        let genesis = LedgerState::with_genesis_balances(
            Amount::from_micro(1_000),
            vec![(payer_account, Amount::from_micro(1_000))],
        )
        .unwrap();
        let claim = sign_claim(
            &payer,
            &account(12),
            250,
            0,
            10_000,
            b"snapshot-test",
            0,
        )
        .unwrap();
        apply_block(&genesis, &SettlementBlockBody::new(vec![claim])).unwrap()
    }

    #[test]
    fn populated_state_round_trips_byte_identically() {
        let state = populated_state();
        let bytes = state.to_snapshot_bytes().unwrap();
        let restored = LedgerState::from_snapshot_bytes(&bytes).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.to_snapshot_bytes().unwrap(), bytes);
        assert_eq!(restored.commitment(), state.commitment());
    }

    #[test]
    fn truncation_is_rejected_at_every_boundary() {
        let bytes = populated_state().to_snapshot_bytes().unwrap();
        for cut in 0..bytes.len() {
            assert!(LedgerState::from_snapshot_bytes(&bytes[..cut]).is_err());
        }
    }

    #[test]
    fn tampering_is_rejected() {
        let mut bytes = populated_state().to_snapshot_bytes().unwrap();
        let index = DOMAIN.len() + 7;
        bytes[index] ^= 0x80;
        assert!(LedgerState::from_snapshot_bytes(&bytes).is_err());
    }

    #[test]
    fn oversized_input_fails_before_decode() {
        let bytes = vec![0; MAX_LEDGER_SNAPSHOT_BYTES + 1];
        assert_eq!(
            LedgerState::from_snapshot_bytes(&bytes),
            Err(ExecutionError::SnapshotTooLarge)
        );
    }
}

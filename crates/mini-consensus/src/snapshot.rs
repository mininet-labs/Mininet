//! Quorum-certificate-bound execution snapshots.
//!
//! The serving peer has no authority. A snapshot is accepted only when its
//! exact [`mini_execution::LedgerState`] commitment equals the finalized block
//! header's `state_root` and that header is independently finalized by the
//! locally supplied static validator set and KEL oracle.

use mini_chain::{BlockHeader, QuorumCertificate, ValidatorOracle, ValidatorSet};
use mini_execution::{LedgerChain, LedgerState, MAX_LEDGER_SNAPSHOT_BYTES};

use crate::catchup::{decode_qc, encode_qc};
use crate::error::{ConsensusError, Result};
use crate::wire::{decode_header, encode_header, put_bytes, Reader};

const DOMAIN: &[u8] = b"mini-consensus/state-snapshot/v1";

/// A complete execution state at one quorum-finalized height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsensusSnapshot {
    pub header: BlockHeader,
    pub qc: QuorumCertificate,
    pub state: LedgerState,
}

impl ConsensusSnapshot {
    /// Construct an internally consistent snapshot. Finality and the deeper
    /// monetary/supply invariants are verified later by [`Self::into_chain`]
    /// inside `mini-execution`, where those invariants are owned.
    pub fn new(header: BlockHeader, qc: QuorumCertificate, state: LedgerState) -> Result<Self> {
        if header.height == 0
            || header.timestamp_ms != header.height
            || header.state_root != state.commitment()
            || qc.height != header.height
            || qc.block_hash != header.hash()
        {
            return Err(ConsensusError::SnapshotProofMismatch);
        }
        Ok(Self { header, qc, state })
    }

    pub fn height(&self) -> u64 {
        self.header.height
    }

    pub fn network_id(&self) -> [u8; 32] {
        self.state.network_id()
    }

    /// Canonical bounded snapshot bytes. A snapshot too large for one
    /// encrypted bearer frame is rejected; chunked/Merkle state transfer is a
    /// separately scoped future format, never an implicit unbounded fallback.
    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        let state = self.state.to_snapshot_bytes()?;
        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
        encode_header(&mut out, &self.header);
        encode_qc(&mut out, &self.qc);
        put_bytes(&mut out, &state);
        if out.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(out)
    }

    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(DOMAIN.len())? != DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        let header = decode_header(&mut reader)?;
        let qc = decode_qc(&mut reader)?;
        let state_bytes = reader.bytes(MAX_LEDGER_SNAPSHOT_BYTES)?;
        if !reader.finished() {
            return Err(ConsensusError::Malformed);
        }
        let state = LedgerState::from_snapshot_bytes(state_bytes)?;
        let snapshot = Self::new(header, qc, state)?;
        if snapshot.to_wire_bytes()?.as_slice() != bytes {
            return Err(ConsensusError::Malformed);
        }
        Ok(snapshot)
    }

    /// Verify finality and turn this checkpoint into a live execution chain.
    pub fn into_chain(
        self,
        expected_network_id: [u8; 32],
        validators: &ValidatorSet,
        oracle: &dyn ValidatorOracle,
    ) -> Result<LedgerChain> {
        LedgerChain::from_finalized_snapshot(
            &self.header,
            self.state,
            &self.qc,
            validators,
            oracle,
            expected_network_id,
        )
        .map_err(ConsensusError::Execution)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use did_mini::{Capabilities, Controller, Did, Kel};
    use mini_chain::{sign_vote, ValidatorOracle, VoteKind};
    use mini_economy::Amount;
    use mini_execution::LedgerState;

    use super::*;

    #[derive(Default)]
    struct Directory(BTreeMap<String, Kel>);

    impl ValidatorOracle for Directory {
        fn kel(&self, did: &Did) -> Option<&Kel> {
            self.0.get(did.scid())
        }
    }

    fn proof() -> (ConsensusSnapshot, ValidatorSet, Directory) {
        let mut roots = Vec::new();
        let mut directory = Directory::default();
        let state = LedgerState::with_genesis_supply(Amount::from_micro(10));
        let header = BlockHeader {
            height: 1,
            prev_hash: [0; 32],
            state_root: state.commitment(),
            timestamp_ms: 1,
            proposer: Controller::incept_single_from_seeds(&[90; 32], &[91; 32])
                .unwrap()
                .did(),
        };
        let block_hash = header.hash();
        let mut votes = Vec::new();
        for seed in [10u8, 20, 30, 40] {
            let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32])
                .unwrap();
            let device = Controller::incept_device_single_from_seeds(
                &root.did(),
                &[seed + 2; 32],
                &[seed + 3; 32],
            )
            .unwrap();
            root.delegate_device(&device.did(), Capabilities::primary())
                .unwrap();
            roots.push(root.did());
            directory.0.insert(root.did().scid().to_string(), root.kel());
            directory
                .0
                .insert(device.did().scid().to_string(), device.kel());
            votes.push(sign_vote(
                VoteKind::Precommit,
                1,
                0,
                block_hash,
                &root.did(),
                &device,
            ));
        }
        let validators = ValidatorSet::new(roots).unwrap();
        let qc = QuorumCertificate {
            height: 1,
            round: 0,
            block_hash,
            votes,
        };
        (
            ConsensusSnapshot::new(header, qc, state).unwrap(),
            validators,
            directory,
        )
    }

    #[test]
    fn snapshot_round_trips_and_reconstructs_chain() {
        let (snapshot, validators, directory) = proof();
        let bytes = snapshot.to_wire_bytes().unwrap();
        let decoded = ConsensusSnapshot::from_wire_bytes(&bytes).unwrap();
        let expected = decoded.state.commitment();
        let chain = decoded
            .into_chain(mini_settlement::MININET_NETWORK_ID, &validators, &directory)
            .unwrap();
        assert_eq!(chain.height(), 1);
        assert_eq!(chain.state().commitment(), expected);
    }

    #[test]
    fn wrong_network_and_tampered_state_fail_closed() {
        let (snapshot, validators, directory) = proof();
        assert!(snapshot
            .clone()
            .into_chain([9; 32], &validators, &directory)
            .is_err());

        let mut bytes = snapshot.to_wire_bytes().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(ConsensusSnapshot::from_wire_bytes(&bytes).is_err());
    }
}

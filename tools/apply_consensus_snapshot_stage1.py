#!/usr/bin/env python3
"""Wire the snapshot/archive modules into their crates, test, and self-remove."""

from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/consensus-snapshot-stage1.yml"


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


# mini-economy: expose state only within the crate and register the validated
# snapshot adapter.
replace_once(
    "crates/mini-economy/src/ledger.rs",
    """pub struct MonetaryLedger {
    genesis_circulating: Amount,
    total_issued: Amount,
    policy_time_ms: u128,
    last_epoch: Option<u64>,
    positions: Vec<VestingPosition>,
}
""",
    """pub struct MonetaryLedger {
    pub(crate) genesis_circulating: Amount,
    pub(crate) total_issued: Amount,
    pub(crate) policy_time_ms: u128,
    pub(crate) last_epoch: Option<u64>,
    pub(crate) positions: Vec<VestingPosition>,
}
""",
)
replace_once(
    "crates/mini-economy/src/lib.rs",
    """mod ledger;
mod policy;
mod simulation;
""",
    """mod ledger;
mod policy;
mod simulation;
mod snapshot;
""",
)
replace_once(
    "crates/mini-economy/src/lib.rs",
    """pub use simulation::{run_scenario, Scenario, ScenarioReport, YearReport};
""",
    """pub use simulation::{run_scenario, Scenario, ScenarioReport, YearReport};
pub use snapshot::{
    MonetaryLedgerSnapshot, MAX_SNAPSHOT_BENEFICIARY_BYTES, MAX_VESTING_POSITIONS,
};
""",
)

# mini-execution: make the state visible to the sibling codec module, add
# snapshot errors, expose the codec, and add the QC-bound chain constructor.
replace_once(
    "crates/mini-execution/src/state.rs",
    """pub struct LedgerState {
    network_id: [u8; 32],
    finalized: BTreeMap<Vec<u8>, (u64, [u8; 32])>,
    rejected: BTreeMap<[u8; 32], CanonicalRejection>,
    monetary: MonetaryLedger,
    balances: BTreeMap<Vec<u8>, Amount>,
    allocated_circulating: Amount,
    unallocated_circulating: Amount,
}
""",
    """pub struct LedgerState {
    pub(crate) network_id: [u8; 32],
    pub(crate) finalized: BTreeMap<Vec<u8>, (u64, [u8; 32])>,
    pub(crate) rejected: BTreeMap<[u8; 32], CanonicalRejection>,
    pub(crate) monetary: MonetaryLedger,
    pub(crate) balances: BTreeMap<Vec<u8>, Amount>,
    pub(crate) allocated_circulating: Amount,
    pub(crate) unallocated_circulating: Amount,
}
""",
)
replace_once(
    "crates/mini-execution/src/lib.rs",
    """mod error;
mod state;
""",
    """mod error;
mod snapshot;
mod state;
""",
)
replace_once(
    "crates/mini-execution/src/lib.rs",
    """pub use error::{ExecutionError, Result};
pub use state::{apply_block, LedgerState, MAX_ACCOUNT_BYTES};
""",
    """pub use error::{ExecutionError, Result};
pub use snapshot::{MAX_LEDGER_SNAPSHOT_BYTES, MAX_LEDGER_SNAPSHOT_ENTRIES};
pub use state::{apply_block, LedgerState, MAX_ACCOUNT_BYTES};
""",
)
replace_once(
    "crates/mini-execution/src/error.rs",
    """    SupplyConservationViolation,
    /// A candidate block's `timestamp_ms` did not equal its own height.
""",
    """    SupplyConservationViolation,
    /// Exact ledger snapshot bytes were truncated, non-canonical, or
    /// structurally inconsistent.
    SnapshotMalformed,
    /// A snapshot or one of its bounded collections exceeded the declared cap.
    SnapshotTooLarge,
    /// A snapshot belongs to a different settlement/consensus network.
    SnapshotWrongNetwork,
    /// The snapshot header/QC/state commitment do not describe one finalized
    /// checkpoint.
    SnapshotProofMismatch,
    /// A candidate block's `timestamp_ms` did not equal its own height.
""",
)
replace_once(
    "crates/mini-execution/src/error.rs",
    """            ExecutionError::SupplyConservationViolation => {
                write!(f, "account balances plus unallocated value do not equal circulating supply")
            }
            ExecutionError::TimestampNotDeterministic { expected, got } => write!(
""",
    """            ExecutionError::SupplyConservationViolation => {
                write!(f, "account balances plus unallocated value do not equal circulating supply")
            }
            ExecutionError::SnapshotMalformed => write!(f, "ledger snapshot is malformed"),
            ExecutionError::SnapshotTooLarge => write!(f, "ledger snapshot exceeds its cap"),
            ExecutionError::SnapshotWrongNetwork => {
                write!(f, "ledger snapshot belongs to another network")
            }
            ExecutionError::SnapshotProofMismatch => {
                write!(f, "ledger snapshot proof does not match its finalized header")
            }
            ExecutionError::TimestampNotDeterministic { expected, got } => write!(
""",
)
replace_once(
    "crates/mini-execution/src/chain.rs",
    """    /// The current finalized height.
    pub fn height(&self) -> u64 {
""",
    """    /// Restore a chain from a complete state whose commitment is bound to
    /// one locally-verified quorum-finalized header. This is the only state-
    /// replacement path; an unsigned state blob can never become canonical.
    pub fn from_finalized_snapshot(
        header: &BlockHeader,
        state: LedgerState,
        qc: &QuorumCertificate,
        validators: &ValidatorSet,
        oracle: &dyn ValidatorOracle,
        expected_network_id: [u8; 32],
    ) -> Result<Self> {
        if state.network_id() != expected_network_id {
            return Err(ExecutionError::SnapshotWrongNetwork);
        }
        state.verify_supply_conservation()?;
        state.verify_balance_map_total()?;
        verify_finality(qc, validators, oracle)?;
        if header.height == 0
            || header.timestamp_ms != header.height
            || qc.height != header.height
            || qc.block_hash != header.hash()
        {
            return Err(ExecutionError::SnapshotProofMismatch);
        }
        if header.state_root != state.commitment() {
            return Err(ExecutionError::StateRootMismatch);
        }
        Ok(LedgerChain {
            height: header.height,
            tip_hash: header.hash(),
            state,
        })
    }

    /// The current finalized height.
    pub fn height(&self) -> u64 {
""",
)

# mini-consensus: standalone finalized-block records, archive/state-sync error
# surfaces, and module exports.
replace_once(
    "crates/mini-consensus/src/catchup.rs",
    """const TAG_REQUEST: u8 = 0;
const TAG_RESPONSE: u8 = 1;
""",
    """const TAG_REQUEST: u8 = 0;
const TAG_RESPONSE: u8 = 1;
const FINALIZED_BLOCK_DOMAIN: &[u8] = b"mini-consensus/finalized-block/v1";
""",
)
replace_once(
    "crates/mini-consensus/src/catchup.rs",
    """pub struct FinalizedBlock {
    /// The finalized block's header.
    pub header: BlockHeader,
    /// The ordered claim body the header's `state_root` commits to.
    pub body: SettlementBlockBody,
    /// The quorum certificate proving this block finalized.
    pub qc: QuorumCertificate,
}

/// \"Send me every block after `from_height`.\"
""",
    """pub struct FinalizedBlock {
    /// The finalized block's header.
    pub header: BlockHeader,
    /// The ordered claim body the header's `state_root` commits to.
    pub body: SettlementBlockBody,
    /// The quorum certificate proving this block finalized.
    pub qc: QuorumCertificate,
}

impl FinalizedBlock {
    /// Canonical standalone bytes used by persistent history and state sync.
    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.extend_from_slice(FINALIZED_BLOCK_DOMAIN);
        encode_header(&mut out, &self.header);
        encode_body(&mut out, &self.body);
        encode_qc(&mut out, &self.qc);
        if out.len() > crate::MAX_MESSAGE_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(out)
    }

    pub fn from_wire_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > crate::MAX_MESSAGE_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        let mut reader = Reader::new(bytes);
        if reader.take(FINALIZED_BLOCK_DOMAIN.len())? != FINALIZED_BLOCK_DOMAIN {
            return Err(ConsensusError::Malformed);
        }
        let block = FinalizedBlock {
            header: decode_header(&mut reader)?,
            body: decode_body(&mut reader)?,
            qc: decode_qc(&mut reader)?,
        };
        if !reader.finished() || block.to_wire_bytes()?.as_slice() != bytes {
            return Err(ConsensusError::Malformed);
        }
        Ok(block)
    }
}

/// \"Send me every block after `from_height`.\"
""",
)
replace_once(
    "crates/mini-consensus/src/catchup.rs",
    "fn encode_qc(w: &mut Vec<u8>, qc: &QuorumCertificate) {",
    "pub(crate) fn encode_qc(w: &mut Vec<u8>, qc: &QuorumCertificate) {",
)
replace_once(
    "crates/mini-consensus/src/catchup.rs",
    "fn decode_qc(r: &mut Reader<'_>) -> Result<QuorumCertificate> {",
    "pub(crate) fn decode_qc(r: &mut Reader<'_>) -> Result<QuorumCertificate> {",
)
replace_once(
    "crates/mini-consensus/src/error.rs",
    """    CatchupOutOfOrder {
        /// The height this node actually needed next.
        expected: u64,
        /// The height the supplied block claimed.
        got: u64,
    },
""",
    """    CatchupOutOfOrder {
        /// The height this node actually needed next.
        expected: u64,
        /// The height the supplied block claimed.
        got: u64,
    },
    /// Local persistent consensus archive I/O or structural failure.
    Storage(String),
    /// The peer/archive belongs to a different settlement network.
    StateSyncWrongNetwork,
    /// The peer no longer retains a checkpoint covering the request.
    StateSyncUnavailable {
        earliest_height: u64,
        tip_height: u64,
    },
    /// A snapshot did not match its header, QC, or state commitment.
    SnapshotProofMismatch,
    /// A snapshot would move a node backward or replace the same height.
    SnapshotNotNewer { current: u64, got: u64 },
    /// Persistent history already contains different bytes for this height.
    ArchiveConflict { height: u64 },
""",
)
replace_once(
    "crates/mini-consensus/src/error.rs",
    """            ConsensusError::CatchupOutOfOrder { expected, got } => {
                write!(
                    f,
                    \"catch-up block out of order: expected height {expected}, got {got}\"
                )
            }
""",
    """            ConsensusError::CatchupOutOfOrder { expected, got } => {
                write!(
                    f,
                    \"catch-up block out of order: expected height {expected}, got {got}\"
                )
            }
            ConsensusError::Storage(message) => write!(f, \"consensus archive: {message}\"),
            ConsensusError::StateSyncWrongNetwork => {
                write!(f, \"state-sync response belongs to another network\")
            }
            ConsensusError::StateSyncUnavailable {
                earliest_height,
                tip_height,
            } => write!(
                f,
                \"state sync unavailable: earliest checkpoint {earliest_height}, tip {tip_height}\"
            ),
            ConsensusError::SnapshotProofMismatch => {
                write!(f, \"snapshot header, QC, and state commitment disagree\")
            }
            ConsensusError::SnapshotNotNewer { current, got } => write!(
                f,
                \"snapshot height {got} does not advance current height {current}\"
            ),
            ConsensusError::ArchiveConflict { height } => write!(
                f,
                \"persistent archive contains conflicting bytes at height {height}\"
            ),
""",
)
replace_once(
    "crates/mini-consensus/src/error.rs",
    """impl From<mini_bearer::BearerError> for ConsensusError {
    fn from(e: mini_bearer::BearerError) -> Self {
        ConsensusError::Transport(e)
    }
}
""",
    """impl From<mini_bearer::BearerError> for ConsensusError {
    fn from(e: mini_bearer::BearerError) -> Self {
        ConsensusError::Transport(e)
    }
}

impl From<std::io::Error> for ConsensusError {
    fn from(error: std::io::Error) -> Self {
        ConsensusError::Storage(error.to_string())
    }
}
""",
)
replace_once(
    "crates/mini-consensus/src/lib.rs",
    """mod round;
mod wire;

pub mod net;
""",
    """mod round;
mod snapshot;
mod state_sync;
mod store;
mod wire;

pub mod net;
""",
)
replace_once(
    "crates/mini-consensus/src/lib.rs",
    """pub use round::{proposer_for, Action, Round, Step, NIL};
pub use wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal, MAX_MESSAGE_BYTES};
""",
    """pub use round::{proposer_for, Action, Round, Step, NIL};
pub use snapshot::ConsensusSnapshot;
pub use state_sync::{
    StateSyncPayload, StateSyncRequest, StateSyncResponse, MAX_STATE_SYNC_BLOCKS,
};
pub use store::{ConsensusArchive, ConsensusArchiveConfig};
pub use wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal, MAX_MESSAGE_BYTES};
""",
)

# Remove this one-shot machinery before testing/navigation so the committed
# tree contains only permanent implementation files.
SELF.unlink()
WORKFLOW.unlink()

run("cargo", "fmt", "--all")
run("cargo", "test", "-p", "mini-economy", "-p", "mini-execution", "-p", "mini-consensus", "--lib")
run(
    "cargo",
    "clippy",
    "-p",
    "mini-economy",
    "-p",
    "mini-execution",
    "-p",
    "mini-consensus",
    "--lib",
    "--",
    "-D",
    "warnings",
)

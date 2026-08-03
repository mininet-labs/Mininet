//! Local persistent finalized-history and snapshot archive.
//!
//! The archive is not consensus authority. It stores quorum-verifiable blocks
//! and snapshots so a node can recover after restart or serve a long-offline
//! peer without retaining unbounded in-memory history. Receivers still verify
//! every QC and state commitment using their own validator set and KEL oracle.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mini_execution::{LedgerState, MAX_LEDGER_SNAPSHOT_BYTES};

use crate::catchup::{FinalizedBlock, MAX_CATCHUP_BLOCKS};
use crate::error::{ConsensusError, Result};
use crate::snapshot::ConsensusSnapshot;
use crate::state_sync::{
    StateSyncPayload, StateSyncRequest, StateSyncResponse, MAX_STATE_SYNC_BLOCKS,
};

const SNAPSHOT_FILE: &str = "snapshot.bin";
const BLOCKS_DIR: &str = "blocks";
const LOCK_FILE: &str = "archive.lock";
const INSTALL_PENDING_FILE: &str = "install.pending";
const INSTALL_PENDING_DOMAIN: &[u8] = b"mini-consensus/archive-install/v1";
const TEMP_SUFFIX: &str = "tmp-write";
/// A stable suffix must fit one encrypted state-sync response by itself.
const MAX_ARCHIVE_SUFFIX_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES;
/// A crash can leave the old stable suffix plus one journaled response batch.
/// Recovery may inspect both, but successful replay compacts back below the
/// stable bound before removing the journal.
const MAX_ARCHIVE_RECOVERY_BYTES: usize =
    MAX_ARCHIVE_SUFFIX_BYTES + mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES;
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize = MAX_CATCHUP_BLOCKS + MAX_STATE_SYNC_BLOCKS;
const MAX_PENDING_INSTALL_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES
    + MAX_LEDGER_SNAPSHOT_BYTES
    + INSTALL_PENDING_DOMAIN.len()
    + 8;

/// Local retention policy. It affects availability only, never finality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsensusArchiveConfig {
    pub network_id: [u8; 32],
    /// Prefer a new snapshot when a finalized height is a multiple of this.
    pub snapshot_interval: u64,
    /// Never retain more than this many blocks after the latest snapshot.
    pub max_suffix_blocks: usize,
}

impl ConsensusArchiveConfig {
    pub fn validate(self) -> Result<Self> {
        if self.snapshot_interval == 0
            || self.max_suffix_blocks == 0
            || self.max_suffix_blocks > MAX_CATCHUP_BLOCKS
        {
            return Err(ConsensusError::Storage(
                "invalid consensus archive retention policy".to_string(),
            ));
        }
        Ok(self)
    }
}

impl Default for ConsensusArchiveConfig {
    fn default() -> Self {
        Self {
            network_id: mini_settlement::MININET_NETWORK_ID,
            snapshot_interval: 64,
            max_suffix_blocks: MAX_STATE_SYNC_BLOCKS,
        }
    }
}

/// A cloneable handle to one local archive directory.
#[derive(Debug, Clone)]
pub struct ConsensusArchive {
    root: PathBuf,
    config: ConsensusArchiveConfig,
}

#[derive(Debug)]
struct RecordPlan {
    block_writes: Vec<(u64, Vec<u8>)>,
    snapshot: Option<(u64, Vec<u8>)>,
}

#[derive(Debug)]
struct SnapshotInstallPlan {
    snapshot_bytes: Vec<u8>,
    block_writes: Vec<(u64, Vec<u8>)>,
    compacted_snapshot_bytes: Option<Vec<u8>>,
}

impl ConsensusArchive {
    pub fn open(root: impl AsRef<Path>, config: ConsensusArchiveConfig) -> Result<Self> {
        let config = config.validate()?;
        let root = root.as_ref().to_path_buf();
        ensure_directory(&root, "consensus archive root")?;
        ensure_directory(&root.join(BLOCKS_DIR), "consensus archive blocks")?;
        let _ = open_archive_lock(&root.join(LOCK_FILE))?;
        Ok(Self { root, config })
    }

    pub fn network_id(&self) -> [u8; 32] {
        self.config.network_id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Recover the best locally retained state from genesis.
    pub fn recovery_response(&self) -> Result<StateSyncResponse> {
        self.response(StateSyncRequest {
            network_id: self.config.network_id,
            from_height: 0,
        })
    }

    /// Build one bounded response for a peer. The caller is untrusted and no
    /// peer identity is consulted; only the explicit network/height matter.
    pub fn response(&self, request: StateSyncRequest) -> Result<StateSyncResponse> {
        if request.network_id != self.config.network_id {
            return Ok(StateSyncResponse::wrong_network(self.config.network_id));
        }
        self.with_lock(|archive| archive.response_locked(request.from_height))
    }

    /// Persist a batch only after the caller has independently verified every
    /// block and produced `final_state` by deterministic execution.
    pub(crate) fn record_verified_batch(
        &self,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<()> {
        if blocks.len() > MAX_STATE_SYNC_BLOCKS {
            return Err(ConsensusError::TooLarge);
        }
        if blocks.is_empty() {
            return Ok(());
        }
        if final_state.network_id() != self.config.network_id
            || blocks
                .last()
                .is_none_or(|block| block.header.state_root != final_state.commitment())
        {
            return Err(ConsensusError::SnapshotProofMismatch);
        }
        let batch_bytes = blocks.iter().try_fold(0usize, |total, block| {
            total
                .checked_add(block.to_wire_bytes()?.len())
                .ok_or(ConsensusError::TooLarge)
        })?;
        if batch_bytes > MAX_ARCHIVE_SUFFIX_BYTES {
            return Err(ConsensusError::TooLarge);
        }

        // A live node persists before swapping its chain, but does not rewrite
        // the full state journal per block. Each block row and any periodic
        // snapshot are atomic; peer snapshot/suffix replacement retains the
        // heavier replayable exact journal below.
        self.with_lock(|archive| archive.record_locked(blocks, final_state))
    }

    /// Replace local recovery state with an already verified response.
    ///
    /// The exact response plus resulting state are journaled before any
    /// snapshot/block replacement. Reopening the archive replays the same
    /// operation idempotently; a process crash can delay availability but
    /// cannot leave an unjournaled half-install that is mistaken for truth.
    pub(crate) fn install_verified_response(
        &self,
        response: &StateSyncResponse,
        final_state: &LedgerState,
    ) -> Result<()> {
        if response.network_id != self.config.network_id
            || final_state.network_id() != self.config.network_id
        {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        self.with_lock(|archive| {
            // Every deterministic rejection is checked before the durable
            // journal exists. Under the same archive lock, no ordinary writer
            // can advance the tip between this preflight and application, so a
            // stale/conflicting response cannot poison recovery forever.
            archive.validate_install_locked(response, final_state)?;
            let pending = encode_pending_install(response, final_state)?;
            atomic_write(&archive.root.join(INSTALL_PENDING_FILE), &pending)?;
            archive.apply_install_locked(response, final_state)?;
            archive.clear_pending_install_locked()
        })
    }

    fn with_lock<T>(&self, operation: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        let path = self.root.join(LOCK_FILE);
        let lock = open_archive_lock(&path)?;
        #[allow(clippy::incompatible_msrv)]
        lock.lock()?;
        let result = (|| {
            self.recover_pending_install_locked()?;
            operation(self)
        })();
        #[allow(clippy::incompatible_msrv)]
        let _ = lock.unlock();
        result
    }

    fn recover_pending_install_locked(&self) -> Result<()> {
        let Some(bytes) = read_regular_limited(
            &self.root.join(INSTALL_PENDING_FILE),
            "consensus archive install journal",
            MAX_PENDING_INSTALL_BYTES,
        )?
        else {
            return Ok(());
        };
        let (response, final_state) = decode_pending_install(&bytes)?;
        if response.network_id != self.config.network_id
            || final_state.network_id() != self.config.network_id
        {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        self.apply_install_locked(&response, &final_state)?;
        self.clear_pending_install_locked()
    }

    fn clear_pending_install_locked(&self) -> Result<()> {
        remove_regular_if_present(
            &self.root.join(INSTALL_PENDING_FILE),
            "consensus archive install journal",
        )?;
        sync_parent_directory(&self.root)
    }

    fn validate_install_locked(
        &self,
        response: &StateSyncResponse,
        final_state: &LedgerState,
    ) -> Result<()> {
        match &response.payload {
            StateSyncPayload::Blocks(blocks) => {
                self.prepare_record_locked(blocks, final_state).map(|_| ())
            }
            StateSyncPayload::Snapshot { snapshot, blocks } => self
                .prepare_snapshot_install_locked(snapshot, blocks, final_state)
                .map(|_| ()),
            StateSyncPayload::WrongNetwork => Err(ConsensusError::StateSyncWrongNetwork),
            StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            } => Err(ConsensusError::StateSyncUnavailable {
                earliest_height: *earliest_height,
                tip_height: *tip_height,
            }),
        }
    }

    fn apply_install_locked(
        &self,
        response: &StateSyncResponse,
        final_state: &LedgerState,
    ) -> Result<()> {
        match &response.payload {
            StateSyncPayload::Blocks(blocks) => self.record_locked(blocks, final_state),
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                self.install_snapshot_locked(snapshot, blocks, final_state)
            }
            StateSyncPayload::WrongNetwork => Err(ConsensusError::StateSyncWrongNetwork),
            StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            } => Err(ConsensusError::StateSyncUnavailable {
                earliest_height: *earliest_height,
                tip_height: *tip_height,
            }),
        }
    }

    fn response_locked(&self, from_height: u64) -> Result<StateSyncResponse> {
        let snapshot = self.read_snapshot_locked()?;
        if snapshot
            .as_ref()
            .is_some_and(|value| value.network_id() != self.config.network_id)
        {
            return Err(ConsensusError::Storage(
                "stored consensus snapshot belongs to another network".to_string(),
            ));
        }
        let mut blocks = self.read_blocks_locked()?;
        let snapshot_height = snapshot.as_ref().map_or(0, ConsensusSnapshot::height);
        let snapshot_hash = snapshot
            .as_ref()
            .map_or([0u8; 32], |value| value.header.hash());
        blocks.retain(|block| block.header.height > snapshot_height);
        verify_chain(snapshot_height, snapshot_hash, &blocks)?;

        let tip_height = blocks
            .last()
            .map_or(snapshot_height, |block| block.header.height);
        if from_height >= tip_height {
            return Ok(StateSyncResponse::blocks(
                self.config.network_id,
                Vec::new(),
            ));
        }

        let use_snapshot = snapshot
            .as_ref()
            .is_some_and(|value| value.height() > from_height);
        let base_height = if use_snapshot {
            snapshot_height
        } else {
            from_height
        };
        let earliest_height = snapshot.as_ref().map_or(0, ConsensusSnapshot::height);

        let candidates: Vec<_> = blocks
            .into_iter()
            .filter(|block| block.header.height > base_height)
            .collect();
        if tip_height > base_height
            && candidates
                .first()
                .is_none_or(|block| block.header.height != base_height.saturating_add(1))
        {
            return Ok(StateSyncResponse::unavailable(
                self.config.network_id,
                earliest_height,
                tip_height,
            ));
        }

        // Account the base once without cloning a potentially 8 MiB
        // `LedgerState`. Each candidate is encoded once for its length; the
        // final response is constructed once after selection.
        let mut wire_size = StateSyncResponse::base_wire_len(
            use_snapshot.then(|| snapshot.as_ref().expect("use_snapshot implies snapshot")),
        )?;
        let mut selected = Vec::new();
        for block in candidates.into_iter().take(MAX_STATE_SYNC_BLOCKS) {
            let expected = base_height
                .checked_add(selected.len() as u64 + 1)
                .ok_or(ConsensusError::TooLarge)?;
            if block.header.height != expected {
                return Err(ConsensusError::Storage(
                    "consensus archive suffix is not contiguous".to_string(),
                ));
            }
            let block_len = block.to_wire_bytes()?.len();
            let next_size = wire_size
                .checked_add(4)
                .and_then(|size| size.checked_add(block_len))
                .ok_or(ConsensusError::TooLarge)?;
            if next_size > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
                break;
            }
            wire_size = next_size;
            selected.push(block);
        }

        if selected.is_empty() && tip_height > base_height && !use_snapshot {
            return Err(ConsensusError::TooLarge);
        }

        if use_snapshot {
            Ok(StateSyncResponse::snapshot(
                self.config.network_id,
                snapshot.expect("use_snapshot implies snapshot"),
                selected,
            ))
        } else {
            Ok(StateSyncResponse::blocks(self.config.network_id, selected))
        }
    }

    fn record_locked(&self, blocks: &[FinalizedBlock], final_state: &LedgerState) -> Result<()> {
        let plan = self.prepare_record_locked(blocks, final_state)?;
        self.apply_record_plan_locked(plan)
    }

    fn prepare_record_locked(
        &self,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<RecordPlan> {
        if blocks.len() > MAX_STATE_SYNC_BLOCKS {
            return Err(ConsensusError::StateSyncTooManyBlocks {
                maximum: MAX_STATE_SYNC_BLOCKS,
                got: blocks.len(),
            });
        }
        if blocks.is_empty() {
            return Ok(RecordPlan {
                block_writes: Vec::new(),
                snapshot: None,
            });
        }
        if final_state.network_id() != self.config.network_id {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        let last = blocks.last().expect("non-empty checked above");
        if last.header.state_root != final_state.commitment() {
            return Err(ConsensusError::SnapshotProofMismatch);
        }

        let current_snapshot = self.read_snapshot_locked()?;
        let snapshot_height = current_snapshot
            .as_ref()
            .map_or(0, ConsensusSnapshot::height);
        let snapshot_hash = current_snapshot
            .as_ref()
            .map_or([0u8; 32], |value| value.header.hash());
        let mut existing = self.read_blocks_locked()?;
        existing.retain(|block| block.header.height > snapshot_height);
        verify_chain(snapshot_height, snapshot_hash, &existing)?;
        let mut suffix_bytes = existing.iter().try_fold(0usize, |total, block| {
            total
                .checked_add(block.to_wire_bytes()?.len())
                .ok_or(ConsensusError::TooLarge)
        })?;
        let mut tip_height = existing
            .last()
            .map_or(snapshot_height, |block| block.header.height);
        let mut tip_hash = existing
            .last()
            .map_or(snapshot_hash, |block| block.header.hash());
        let mut block_writes = Vec::new();

        for block in blocks {
            let height = block.header.height;
            if height <= snapshot_height {
                continue;
            }
            let bytes = block.to_wire_bytes()?;
            if height <= tip_height {
                if let Some((_, planned)) = block_writes
                    .iter()
                    .find(|(planned_height, _)| *planned_height == height)
                {
                    if planned != &bytes {
                        return Err(ConsensusError::ArchiveConflict { height });
                    }
                    continue;
                }
                let existing = read_regular_limited(
                    &self.block_path(height),
                    "consensus finalized block",
                    crate::MAX_MESSAGE_BYTES,
                )?
                .ok_or_else(|| {
                    ConsensusError::Storage(
                        "archive tip names a missing finalized block".to_string(),
                    )
                })?;
                if existing != bytes {
                    return Err(ConsensusError::ArchiveConflict { height });
                }
                continue;
            }
            let expected = tip_height.checked_add(1).ok_or(ConsensusError::TooLarge)?;
            if height != expected {
                return Err(ConsensusError::CatchupOutOfOrder {
                    expected,
                    got: height,
                });
            }
            if block.header.prev_hash != tip_hash {
                return Err(ConsensusError::Storage(
                    "finalized block parent does not match archive tip".to_string(),
                ));
            }
            suffix_bytes = suffix_bytes
                .checked_add(bytes.len())
                .ok_or(ConsensusError::TooLarge)?;
            if suffix_bytes > MAX_ARCHIVE_RECOVERY_BYTES {
                return Err(ConsensusError::TooLarge);
            }
            block_writes.push((height, bytes));
            tip_height = height;
            tip_hash = block.header.hash();
        }

        let distance = last.header.height.saturating_sub(snapshot_height);
        let snapshot = if last.header.height % self.config.snapshot_interval == 0
            || distance >= self.config.max_suffix_blocks as u64
            || suffix_bytes >= MAX_ARCHIVE_SUFFIX_BYTES
        {
            let snapshot =
                ConsensusSnapshot::new(last.header.clone(), last.qc.clone(), final_state.clone())?;
            Some((snapshot.height(), snapshot.to_wire_bytes()?))
        } else {
            None
        };

        Ok(RecordPlan {
            block_writes,
            snapshot,
        })
    }

    fn apply_record_plan_locked(&self, plan: RecordPlan) -> Result<()> {
        for (height, bytes) in plan.block_writes {
            atomic_write(&self.block_path(height), &bytes)?;
        }
        if let Some((height, bytes)) = plan.snapshot {
            atomic_write(&self.root.join(SNAPSHOT_FILE), &bytes)?;
            self.prune_blocks_through_locked(height)?;
        }
        Ok(())
    }

    fn install_snapshot_locked(
        &self,
        snapshot: &ConsensusSnapshot,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<()> {
        let plan = self.prepare_snapshot_install_locked(snapshot, blocks, final_state)?;
        self.apply_snapshot_install_plan_locked(plan)
    }

    fn prepare_snapshot_install_locked(
        &self,
        snapshot: &ConsensusSnapshot,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<SnapshotInstallPlan> {
        if snapshot.network_id() != self.config.network_id {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        if blocks.len() > MAX_STATE_SYNC_BLOCKS {
            return Err(ConsensusError::StateSyncTooManyBlocks {
                maximum: MAX_STATE_SYNC_BLOCKS,
                got: blocks.len(),
            });
        }
        verify_chain(snapshot.height(), snapshot.header.hash(), blocks)?;
        let incoming_tip = blocks
            .last()
            .map_or(snapshot.height(), |block| block.header.height);
        let current_snapshot = self.read_snapshot_locked()?;
        let current_blocks = self.read_blocks_locked()?;
        let current_snapshot_height = current_snapshot
            .as_ref()
            .map_or(0, ConsensusSnapshot::height);
        let current_block_height = current_blocks.last().map_or(0, |block| block.header.height);
        let current_tip = current_snapshot_height.max(current_block_height);
        if incoming_tip < current_tip {
            return Err(ConsensusError::SnapshotNotNewer {
                current: current_tip,
                got: incoming_tip,
            });
        }
        if let Some(current) = &current_snapshot {
            if current.height() == snapshot.height()
                && current.header.hash() != snapshot.header.hash()
            {
                return Err(ConsensusError::ArchiveConflict {
                    height: snapshot.height(),
                });
            }
        }
        let expected_root = blocks
            .last()
            .map_or(snapshot.header.state_root, |block| block.header.state_root);
        if expected_root != final_state.commitment() {
            return Err(ConsensusError::SnapshotProofMismatch);
        }

        let snapshot_bytes = snapshot.to_wire_bytes()?;
        let mut block_writes = Vec::with_capacity(blocks.len());
        for block in blocks {
            block_writes.push((block.header.height, block.to_wire_bytes()?));
        }
        let compacted_snapshot_bytes = if blocks.len() >= self.config.max_suffix_blocks {
            let last = blocks
                .last()
                .expect("non-empty when suffix reaches positive retention bound");
            let newer =
                ConsensusSnapshot::new(last.header.clone(), last.qc.clone(), final_state.clone())?;
            Some(newer.to_wire_bytes()?)
        } else {
            None
        };

        Ok(SnapshotInstallPlan {
            snapshot_bytes,
            block_writes,
            compacted_snapshot_bytes,
        })
    }

    fn apply_snapshot_install_plan_locked(&self, plan: SnapshotInstallPlan) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &plan.snapshot_bytes)?;
        self.clear_blocks_locked()?;
        for (height, bytes) in plan.block_writes {
            atomic_write(&self.block_path(height), &bytes)?;
        }
        if let Some(bytes) = plan.compacted_snapshot_bytes {
            atomic_write(&self.root.join(SNAPSHOT_FILE), &bytes)?;
            self.clear_blocks_locked()?;
        }
        Ok(())
    }

    fn read_snapshot_locked(&self) -> Result<Option<ConsensusSnapshot>> {
        let Some(bytes) = read_regular_limited(
            &self.root.join(SNAPSHOT_FILE),
            "consensus snapshot",
            mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES,
        )?
        else {
            return Ok(None);
        };
        Ok(Some(ConsensusSnapshot::from_wire_bytes(&bytes)?))
    }

    #[cfg(test)]
    fn write_snapshot_locked(&self, snapshot: &ConsensusSnapshot) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &snapshot.to_wire_bytes()?)
    }

    fn read_blocks_locked(&self) -> Result<Vec<FinalizedBlock>> {
        let dir = self.root.join(BLOCKS_DIR);
        ensure_directory(&dir, "consensus archive blocks")?;
        let mut rows = Vec::new();
        let mut stale_temps = Vec::new();
        let mut entries = 0usize;
        let mut total_bytes = 0usize;
        for entry in fs::read_dir(&dir)? {
            entries = entries.checked_add(1).ok_or(ConsensusError::TooLarge)?;
            if entries > MAX_ARCHIVE_DIRECTORY_ENTRIES {
                return Err(ConsensusError::TooLarge);
            }
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(ConsensusError::Storage(
                    "symlink in consensus block archive".to_string(),
                ));
            }
            if !file_type.is_file() {
                return Err(ConsensusError::Storage(
                    "non-file in consensus block archive".to_string(),
                ));
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let file_len =
                usize::try_from(entry.metadata()?.len()).map_err(|_| ConsensusError::TooLarge)?;
            total_bytes = total_bytes
                .checked_add(file_len)
                .ok_or(ConsensusError::TooLarge)?;
            if total_bytes > MAX_ARCHIVE_RECOVERY_BYTES {
                return Err(ConsensusError::TooLarge);
            }
            if parse_temp_block_name(&name)?.is_some() {
                if file_len > crate::MAX_MESSAGE_BYTES {
                    return Err(ConsensusError::TooLarge);
                }
                stale_temps.push(entry.path());
                continue;
            }
            let height = parse_block_name(&name)?;
            let bytes = read_regular_limited(
                &entry.path(),
                "consensus finalized block",
                crate::MAX_MESSAGE_BYTES,
            )?
            .ok_or_else(|| {
                ConsensusError::Storage("finalized block disappeared during read".to_string())
            })?;
            let block = FinalizedBlock::from_wire_bytes(&bytes)?;
            if block.header.height != height {
                return Err(ConsensusError::Storage(
                    "finalized block filename/height mismatch".to_string(),
                ));
            }
            rows.push(block);
        }
        let removed_temps = !stale_temps.is_empty();
        for path in stale_temps {
            remove_regular_if_present(&path, "interrupted consensus block write")?;
        }
        if removed_temps {
            sync_parent_directory(&dir)?;
        }
        rows.sort_by_key(|block| block.header.height);
        if rows
            .windows(2)
            .any(|pair| pair[0].header.height >= pair[1].header.height)
        {
            return Err(ConsensusError::Storage(
                "duplicate or unordered finalized block files".to_string(),
            ));
        }
        Ok(rows)
    }

    fn clear_blocks_locked(&self) -> Result<()> {
        for block in self.read_blocks_locked()? {
            remove_regular_if_present(
                &self.block_path(block.header.height),
                "consensus finalized block",
            )?;
        }
        sync_parent_directory(&self.root.join(BLOCKS_DIR))
    }

    fn prune_blocks_through_locked(&self, height: u64) -> Result<()> {
        for block in self.read_blocks_locked()? {
            if block.header.height <= height {
                remove_regular_if_present(
                    &self.block_path(block.header.height),
                    "consensus finalized block",
                )?;
            }
        }
        sync_parent_directory(&self.root.join(BLOCKS_DIR))
    }

    fn block_path(&self, height: u64) -> PathBuf {
        self.root.join(BLOCKS_DIR).join(format!("{height:020}.bin"))
    }
}

fn encode_pending_install(
    response: &StateSyncResponse,
    final_state: &LedgerState,
) -> Result<Vec<u8>> {
    let response_bytes = response.to_wire_bytes()?;
    let state_bytes = final_state.to_snapshot_bytes()?;
    let response_len = u32::try_from(response_bytes.len()).map_err(|_| ConsensusError::TooLarge)?;
    let state_len = u32::try_from(state_bytes.len()).map_err(|_| ConsensusError::TooLarge)?;
    let mut out = Vec::with_capacity(
        INSTALL_PENDING_DOMAIN.len() + 8 + response_bytes.len() + state_bytes.len(),
    );
    out.extend_from_slice(INSTALL_PENDING_DOMAIN);
    out.extend_from_slice(&response_len.to_be_bytes());
    out.extend_from_slice(&response_bytes);
    out.extend_from_slice(&state_len.to_be_bytes());
    out.extend_from_slice(&state_bytes);
    if out.len() > MAX_PENDING_INSTALL_BYTES {
        return Err(ConsensusError::TooLarge);
    }
    Ok(out)
}

fn decode_pending_install(bytes: &[u8]) -> Result<(StateSyncResponse, LedgerState)> {
    if bytes.len() > MAX_PENDING_INSTALL_BYTES || !bytes.starts_with(INSTALL_PENDING_DOMAIN) {
        return Err(ConsensusError::Malformed);
    }
    let mut position = INSTALL_PENDING_DOMAIN.len();
    let response_bytes = take_len_prefixed(
        bytes,
        &mut position,
        mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES,
    )?;
    let state_bytes = take_len_prefixed(bytes, &mut position, MAX_LEDGER_SNAPSHOT_BYTES)?;
    if position != bytes.len() {
        return Err(ConsensusError::Malformed);
    }
    Ok((
        StateSyncResponse::from_wire_bytes(response_bytes)?,
        LedgerState::from_snapshot_bytes(state_bytes)?,
    ))
}

fn take_len_prefixed<'a>(
    bytes: &'a [u8],
    position: &mut usize,
    maximum: usize,
) -> Result<&'a [u8]> {
    let length_end = position.checked_add(4).ok_or(ConsensusError::Malformed)?;
    let length_bytes = bytes
        .get(*position..length_end)
        .ok_or(ConsensusError::Malformed)?;
    let length = u32::from_be_bytes(length_bytes.try_into().expect("four-byte slice")) as usize;
    if length > maximum {
        return Err(ConsensusError::TooLarge);
    }
    let end = length_end
        .checked_add(length)
        .ok_or(ConsensusError::Malformed)?;
    let value = bytes
        .get(length_end..end)
        .ok_or(ConsensusError::Malformed)?;
    *position = end;
    Ok(value)
}

fn verify_chain(base_height: u64, base_hash: [u8; 32], blocks: &[FinalizedBlock]) -> Result<()> {
    let mut expected_height = base_height;
    let mut expected_parent = base_hash;
    for block in blocks {
        expected_height = expected_height
            .checked_add(1)
            .ok_or(ConsensusError::TooLarge)?;
        if block.header.height != expected_height {
            return Err(ConsensusError::CatchupOutOfOrder {
                expected: expected_height,
                got: block.header.height,
            });
        }
        if block.header.prev_hash != expected_parent {
            return Err(ConsensusError::Storage(
                "consensus archive parent chain is broken".to_string(),
            ));
        }
        expected_parent = block.header.hash();
    }
    Ok(())
}

fn parse_temp_block_name(name: &str) -> Result<Option<u64>> {
    let Some(number) = name.strip_suffix(".tmp-write") else {
        return Ok(None);
    };
    if number.len() != 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ConsensusError::Storage(
            "malformed interrupted consensus block filename".to_string(),
        ));
    }
    number
        .parse()
        .map(Some)
        .map_err(|_| ConsensusError::Storage("invalid temporary block height".to_string()))
}

fn parse_block_name(name: &str) -> Result<u64> {
    let Some(number) = name.strip_suffix(".bin") else {
        return Err(ConsensusError::Storage(
            "unknown entry in consensus block archive".to_string(),
        ));
    };
    if number.len() != 20 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ConsensusError::Storage(
            "malformed consensus block filename".to_string(),
        ));
    }
    number
        .parse()
        .map_err(|_| ConsensusError::Storage("invalid consensus block height".to_string()))
}

fn ensure_directory(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ConsensusError::Storage(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_dir() => Err(ConsensusError::Storage(format!(
            "{label} is not a directory"
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(ConsensusError::Storage(format!(
                    "{label} was not created as a directory"
                )));
            }
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn reject_symlink_or_non_file_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ConsensusError::Storage(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => Err(ConsensusError::Storage(format!(
            "{label} is not a regular file"
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn configure_no_follow(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Win32 FILE_FLAG_OPEN_REPARSE_POINT: open the link/reparse point
        // itself rather than following it, then reject it by handle metadata.
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
}

fn open_archive_lock(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    configure_no_follow(&mut options);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConsensusError::Storage(
            "consensus archive lock is not a regular file".to_string(),
        ));
    }
    Ok(file)
}

fn open_regular_readonly(path: &Path, label: &str) -> Result<Option<File>> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConsensusError::Storage(format!(
            "{label} is not a regular file"
        )));
    }
    Ok(Some(file))
}

fn read_regular_limited(path: &Path, label: &str, max: usize) -> Result<Option<Vec<u8>>> {
    // Open first with no-follow semantics, then derive length/type from the
    // opened handle. A concurrent rename can no longer substitute a symlink
    // between an lstat check and a following File::open.
    let Some(file) = open_regular_readonly(path, label)? else {
        return Ok(None);
    };
    let len = usize::try_from(file.metadata()?.len()).map_err(|_| ConsensusError::TooLarge)?;
    if len > max {
        return Err(ConsensusError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(len);
    file.take(max.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(ConsensusError::TooLarge);
    }
    Ok(Some(bytes))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        ConsensusError::Storage("consensus archive path has no parent".to_string())
    })?;
    ensure_directory(parent, "consensus archive parent")?;
    let temp = path.with_extension(TEMP_SUFFIX);
    remove_regular_if_present(&temp, "consensus archive temporary")?;
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    reject_symlink_or_non_file_if_present(path, "consensus archive destination")?;
    fs::rename(&temp, path)?;
    sync_parent_directory(path)
}

fn remove_regular_if_present(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ConsensusError::Storage(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => Err(ConsensusError::Storage(format!(
            "{label} is not a regular file"
        ))),
        Ok(_) => {
            fs::remove_file(path)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = if path.is_dir() {
        path
    } else {
        path.parent().ok_or_else(|| {
            ConsensusError::Storage("consensus archive path has no parent".to_string())
        })?
    };
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = parent;
    Ok(())
}

#[cfg(test)]
mod tests {
    use did_mini::Controller;
    use mini_chain::{BlockHeader, QuorumCertificate};

    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "mini-consensus-archive-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        root
    }

    fn block(height: u64, previous: [u8; 32], state: &LedgerState) -> FinalizedBlock {
        let proposer = Controller::incept_single_from_seeds(&[4; 32], &[5; 32])
            .unwrap()
            .did();
        let header = BlockHeader {
            height,
            prev_hash: previous,
            state_root: state.commitment(),
            timestamp_ms: height,
            proposer,
        };
        FinalizedBlock {
            qc: QuorumCertificate {
                height,
                round: 0,
                block_hash: header.hash(),
                votes: Vec::new(),
            },
            header,
            body: mini_execution::SettlementBlockBody::new(Vec::new()),
        }
    }

    #[test]
    fn archive_snapshots_prunes_reopens_and_serves_suffix() {
        let root = temp_root("roundtrip");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 2,
            max_suffix_blocks: 2,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let mut previous = [0; 32];
        for height in 1..=5 {
            let finalized = block(height, previous, &state);
            previous = finalized.header.hash();
            archive.record_verified_batch(&[finalized], &state).unwrap();
        }

        let response = archive.recovery_response().unwrap();
        match response.payload {
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                assert_eq!(snapshot.height(), 4);
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].header.height, 5);
            }
            other => panic!("expected snapshot plus suffix, got {other:?}"),
        }

        let reopened = ConsensusArchive::open(&root, config).unwrap();
        assert_eq!(
            reopened.recovery_response().unwrap(),
            archive.recovery_response().unwrap()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn response_selection_stays_within_one_frame_without_repeated_snapshot_growth() {
        let root = temp_root("response-bound");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 1,
            max_suffix_blocks: 8,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        archive
            .record_verified_batch(core::slice::from_ref(&first), &state)
            .unwrap();
        let response = archive.recovery_response().unwrap();
        assert!(
            response.to_wire_bytes().unwrap().len() <= mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_network_is_a_control_response_not_trust() {
        let root = temp_root("network");
        let archive = ConsensusArchive::open(&root, ConsensusArchiveConfig::default()).unwrap();
        let response = archive
            .response(StateSyncRequest {
                network_id: [9; 32],
                from_height: 0,
            })
            .unwrap();
        assert!(matches!(response.payload, StateSyncPayload::WrongNetwork));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupted_retained_block_fails_closed() {
        let root = temp_root("corrupt");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let finalized = block(1, [0; 32], &state);
        archive.record_verified_batch(&[finalized], &state).unwrap();
        fs::write(archive.block_path(1), b"corrupt").unwrap();
        assert!(archive.recovery_response().is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn interrupted_snapshot_install_is_replayed_exactly_on_reopen() {
        let root = temp_root("pending-install");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        archive
            .record_verified_batch(core::slice::from_ref(&first), &state)
            .unwrap();

        let second = block(2, first.header.hash(), &state);
        let snapshot =
            ConsensusSnapshot::new(second.header.clone(), second.qc.clone(), state.clone())
                .unwrap();
        let response = StateSyncResponse::snapshot(config.network_id, snapshot.clone(), Vec::new());
        let pending = encode_pending_install(&response, &state).unwrap();

        archive
            .with_lock(|locked| {
                atomic_write(&locked.root.join(INSTALL_PENDING_FILE), &pending)?;
                // Simulate process loss after the new snapshot rename but
                // before old suffix cleanup and journal removal.
                locked.write_snapshot_locked(&snapshot)
            })
            .unwrap();

        let reopened = ConsensusArchive::open(&root, config).unwrap();
        let recovered = reopened.recovery_response().unwrap();
        match recovered.payload {
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                assert_eq!(snapshot.height(), 2);
                assert!(blocks.is_empty());
            }
            other => panic!("expected recovered snapshot, got {other:?}"),
        }
        assert!(!root.join(INSTALL_PENDING_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_install_journal_fails_closed_before_allocation() {
        let root = temp_root("oversized-install");
        let config = ConsensusArchiveConfig::default();
        let archive = ConsensusArchive::open(&root, config).unwrap();
        fs::write(
            root.join(INSTALL_PENDING_FILE),
            vec![0x41; MAX_PENDING_INSTALL_BYTES + 1],
        )
        .unwrap();
        assert_eq!(archive.recovery_response(), Err(ConsensusError::TooLarge));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn orphaned_block_temp_is_removed_during_recovery() {
        let root = temp_root("orphan-temp");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        archive
            .record_verified_batch(core::slice::from_ref(&first), &state)
            .unwrap();
        let second = block(2, first.header.hash(), &state);
        let temp = root.join(BLOCKS_DIR).join(format!("{:020}.tmp-write", 2));
        fs::write(&temp, second.to_wire_bytes().unwrap()).unwrap();

        let reopened = ConsensusArchive::open(&root, config).unwrap();
        let response = reopened.recovery_response().unwrap();
        assert_eq!(response.block_count(), 1);
        assert!(!temp.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_middle_block_prevents_any_later_append() {
        let root = temp_root("missing-middle");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        let second = block(2, first.header.hash(), &state);
        let third = block(3, second.header.hash(), &state);
        for value in [&first, &second, &third] {
            archive
                .record_verified_batch(core::slice::from_ref(value), &state)
                .unwrap();
        }
        fs::remove_file(archive.block_path(2)).unwrap();
        let fourth = block(4, third.header.hash(), &state);
        assert!(archive
            .record_verified_batch(core::slice::from_ref(&fourth), &state)
            .is_err());
        assert!(!archive.block_path(4).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_block_with_the_wrong_parent_is_never_persisted() {
        let root = temp_root("wrong-parent");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        archive
            .record_verified_batch(core::slice::from_ref(&first), &state)
            .unwrap();
        let second = block(2, [9; 32], &state);
        assert!(archive
            .record_verified_batch(core::slice::from_ref(&second), &state)
            .is_err());
        assert!(!archive.block_path(2).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_snapshot_rejection_never_poisoned_the_archive_journal() {
        let root = temp_root("stale-install");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        let second = block(2, first.header.hash(), &state);
        archive
            .record_verified_batch(core::slice::from_ref(&first), &state)
            .unwrap();
        archive
            .record_verified_batch(core::slice::from_ref(&second), &state)
            .unwrap();

        let stale =
            ConsensusSnapshot::new(first.header.clone(), first.qc.clone(), state.clone()).unwrap();
        let response = StateSyncResponse::snapshot(config.network_id, stale, Vec::new());
        assert_eq!(
            archive.install_verified_response(&response, &state),
            Err(ConsensusError::SnapshotNotNewer { current: 2, got: 1 })
        );
        assert!(!root.join(INSTALL_PENDING_FILE).exists());
        assert_eq!(archive.recovery_response().unwrap().block_count(), 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mismatched_final_state_is_rejected_before_journal_or_block_write() {
        let root = temp_root("mismatched-state");
        let config = ConsensusArchiveConfig {
            snapshot_interval: 100,
            max_suffix_blocks: 10,
            ..ConsensusArchiveConfig::default()
        };
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let mut inconsistent = block(1, [0; 32], &state);
        inconsistent.header.state_root = [9; 32];
        inconsistent.qc.block_hash = inconsistent.header.hash();
        let response = StateSyncResponse::blocks(config.network_id, vec![inconsistent]);

        assert_eq!(
            archive.install_verified_response(&response, &state),
            Err(ConsensusError::SnapshotProofMismatch)
        );
        assert!(!archive.block_path(1).exists());
        assert!(!root.join(INSTALL_PENDING_FILE).exists());
        assert_eq!(archive.recovery_response().unwrap().block_count(), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn oversized_snapshot_suffix_reports_its_actual_limit_error() {
        let root = temp_root("snapshot-block-limit");
        let config = ConsensusArchiveConfig::default();
        let archive = ConsensusArchive::open(&root, config).unwrap();
        let state = LedgerState::new();
        let first = block(1, [0; 32], &state);
        let snapshot =
            ConsensusSnapshot::new(first.header.clone(), first.qc.clone(), state.clone()).unwrap();
        let response = StateSyncResponse::snapshot(
            config.network_id,
            snapshot,
            vec![first; MAX_STATE_SYNC_BLOCKS + 1],
        );
        assert_eq!(
            archive.install_verified_response(&response, &state),
            Err(ConsensusError::StateSyncTooManyBlocks {
                maximum: MAX_STATE_SYNC_BLOCKS,
                got: MAX_STATE_SYNC_BLOCKS + 1,
            })
        );
        assert!(!root.join(INSTALL_PENDING_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_regular_archive_file_is_refused_by_the_open_itself() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink-file");
        let archive = ConsensusArchive::open(&root, ConsensusArchiveConfig::default()).unwrap();
        let target = root.join("outside-snapshot.bin");
        fs::write(&target, b"not a snapshot").unwrap();
        symlink(&target, root.join(SNAPSHOT_FILE)).unwrap();
        assert!(archive.recovery_response().is_err());
        let _ = fs::remove_file(root.join(SNAPSHOT_FILE));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_blocks_directory_is_refused() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink");
        fs::create_dir_all(&root).unwrap();
        let target = temp_root("target");
        fs::create_dir_all(&target).unwrap();
        symlink(&target, root.join(BLOCKS_DIR)).unwrap();
        assert!(ConsensusArchive::open(&root, ConsensusArchiveConfig::default()).is_err());
        let _ = fs::remove_file(root.join(BLOCKS_DIR));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(target);
    }
}

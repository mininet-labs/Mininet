//! Local persistent finalized-history and snapshot archive.
//!
//! The archive is not consensus authority. It stores quorum-verifiable blocks
//! and snapshots so a node can recover after restart or serve a long-offline
//! peer without retaining unbounded in-memory history. Receivers still verify
//! every QC and state commitment using their own validator set and KEL oracle.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use mini_execution::LedgerState;

use crate::catchup::{FinalizedBlock, MAX_CATCHUP_BLOCKS};
use crate::error::{ConsensusError, Result};
use crate::snapshot::ConsensusSnapshot;
use crate::state_sync::{
    StateSyncPayload, StateSyncRequest, StateSyncResponse, MAX_STATE_SYNC_BLOCKS,
};

const SNAPSHOT_FILE: &str = "snapshot.bin";
const BLOCKS_DIR: &str = "blocks";
const LOCK_FILE: &str = "archive.lock";
const TEMP_SUFFIX: &str = "tmp-write";
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize = 4_096;

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

impl ConsensusArchive {
    pub fn open(root: impl AsRef<Path>, config: ConsensusArchiveConfig) -> Result<Self> {
        let config = config.validate()?;
        let root = root.as_ref().to_path_buf();
        ensure_directory(&root, "consensus archive root")?;
        ensure_directory(&root.join(BLOCKS_DIR), "consensus archive blocks")?;
        reject_symlink_or_non_file_if_present(&root.join(LOCK_FILE), "consensus archive lock")?;
        let _ = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(LOCK_FILE))?;
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
        if blocks.len() > MAX_CATCHUP_BLOCKS {
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
        self.with_lock(|archive| archive.record_locked(blocks, final_state))
    }

    /// Replace local recovery state with an already verified snapshot response.
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
        self.with_lock(|archive| match &response.payload {
            StateSyncPayload::Blocks(blocks) => archive.record_locked(blocks, final_state),
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                archive.install_snapshot_locked(snapshot, blocks, final_state)
            }
            StateSyncPayload::WrongNetwork => Err(ConsensusError::StateSyncWrongNetwork),
            StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            } => Err(ConsensusError::StateSyncUnavailable {
                earliest_height: *earliest_height,
                tip_height: *tip_height,
            }),
        })
    }

    fn with_lock<T>(&self, operation: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        let path = self.root.join(LOCK_FILE);
        reject_symlink_or_non_file_if_present(&path, "consensus archive lock")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        #[allow(clippy::incompatible_msrv)]
        lock.lock()?;
        let result = operation(self);
        #[allow(clippy::incompatible_msrv)]
        let _ = lock.unlock();
        result
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
        blocks.retain(|block| block.header.height > snapshot_height);
        verify_contiguous(snapshot_height, &blocks)?;

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
            selected.push(block);
            let trial = if use_snapshot {
                StateSyncResponse::snapshot(
                    self.config.network_id,
                    snapshot.clone().expect("use_snapshot implies snapshot"),
                    selected.clone(),
                )
            } else {
                StateSyncResponse::blocks(self.config.network_id, selected.clone())
            };
            if matches!(trial.to_wire_bytes(), Err(ConsensusError::TooLarge)) {
                selected.pop();
                break;
            }
        }

        if selected.is_empty() && tip_height > base_height {
            let base_response = if use_snapshot {
                StateSyncResponse::snapshot(
                    self.config.network_id,
                    snapshot.clone().expect("use_snapshot implies snapshot"),
                    Vec::new(),
                )
            } else {
                StateSyncResponse::blocks(self.config.network_id, Vec::new())
            };
            base_response.to_wire_bytes()?;
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
        if blocks.is_empty() {
            return Ok(());
        }
        let current_snapshot = self.read_snapshot_locked()?;
        let snapshot_height = current_snapshot
            .as_ref()
            .map_or(0, ConsensusSnapshot::height);
        let existing = self.read_blocks_locked()?;
        let mut tip_height = existing
            .last()
            .map_or(snapshot_height, |block| block.header.height);

        for block in blocks {
            let height = block.header.height;
            if height <= snapshot_height {
                continue;
            }
            let bytes = block.to_wire_bytes()?;
            let path = self.block_path(height);
            if height <= tip_height {
                let existing = read_regular_limited(
                    &path,
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
            atomic_write(&path, &bytes)?;
            tip_height = height;
        }

        let last = blocks.last().expect("non-empty checked above");
        if last.header.state_root != final_state.commitment() {
            return Err(ConsensusError::SnapshotProofMismatch);
        }
        let distance = last.header.height.saturating_sub(snapshot_height);
        if last.header.height % self.config.snapshot_interval == 0
            || distance >= self.config.max_suffix_blocks as u64
        {
            let snapshot =
                ConsensusSnapshot::new(last.header.clone(), last.qc.clone(), final_state.clone())?;
            self.write_snapshot_locked(&snapshot)?;
            self.prune_blocks_through_locked(snapshot.height())?;
        }
        Ok(())
    }

    fn install_snapshot_locked(
        &self,
        snapshot: &ConsensusSnapshot,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<()> {
        if snapshot.network_id() != self.config.network_id || blocks.len() > MAX_STATE_SYNC_BLOCKS {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        verify_contiguous(snapshot.height(), blocks)?;
        let expected_root = blocks
            .last()
            .map_or(snapshot.header.state_root, |block| block.header.state_root);
        if expected_root != final_state.commitment() {
            return Err(ConsensusError::SnapshotProofMismatch);
        }

        self.write_snapshot_locked(snapshot)?;
        self.clear_blocks_locked()?;
        for block in blocks {
            atomic_write(
                &self.block_path(block.header.height),
                &block.to_wire_bytes()?,
            )?;
        }

        if blocks.len() >= self.config.max_suffix_blocks {
            if let Some(last) = blocks.last() {
                let newer = ConsensusSnapshot::new(
                    last.header.clone(),
                    last.qc.clone(),
                    final_state.clone(),
                )?;
                self.write_snapshot_locked(&newer)?;
                self.clear_blocks_locked()?;
            }
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

    fn write_snapshot_locked(&self, snapshot: &ConsensusSnapshot) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &snapshot.to_wire_bytes()?)
    }

    fn read_blocks_locked(&self) -> Result<Vec<FinalizedBlock>> {
        let dir = self.root.join(BLOCKS_DIR);
        ensure_directory(&dir, "consensus archive blocks")?;
        let mut rows = Vec::new();
        let mut entries = 0usize;
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
            let height = parse_block_name(&entry.file_name().to_string_lossy())?;
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

fn verify_contiguous(base_height: u64, blocks: &[FinalizedBlock]) -> Result<()> {
    let mut expected = base_height;
    for block in blocks {
        expected = expected.checked_add(1).ok_or(ConsensusError::TooLarge)?;
        if block.header.height != expected {
            return Err(ConsensusError::CatchupOutOfOrder {
                expected,
                got: block.header.height,
            });
        }
    }
    Ok(())
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

fn read_regular_limited(path: &Path, label: &str, max: usize) -> Result<Option<Vec<u8>>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ConsensusError::Storage(format!("{label} is a symlink")))
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(ConsensusError::Storage(format!(
                "{label} is not a regular file"
            )))
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let len = usize::try_from(metadata.len()).map_err(|_| ConsensusError::TooLarge)?;
    if len > max {
        return Err(ConsensusError::TooLarge);
    }
    let mut bytes = Vec::with_capacity(len);
    File::open(path)?
        .take(max.saturating_add(1) as u64)
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

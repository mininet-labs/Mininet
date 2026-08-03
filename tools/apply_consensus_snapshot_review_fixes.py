#!/usr/bin/env python3
"""Apply the permanent fixes requested by PR #289's archive/state-sync review.

This helper is branch-local and is deleted by the validating finalizer in the
same commit that carries the tested source changes.
"""

from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    Path(path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:120]!r}")
    write(path, text.replace(old, new, 1))


def replace_section(path: str, start: str, end: str, replacement: str) -> None:
    text = read(path)
    start_at = text.find(start)
    if start_at < 0:
        raise SystemExit(f"{path}: missing section start {start!r}")
    end_at = text.find(end, start_at)
    if end_at < 0:
        raise SystemExit(f"{path}: missing section end {end!r}")
    if text.find(start, start_at + 1) >= 0 and text.find(start, start_at + 1) < end_at:
        raise SystemExit(f"{path}: duplicate section start {start!r}")
    write(path, text[:start_at] + replacement + text[end_at:])


def append_once(path: str, marker: str, block: str) -> None:
    text = read(path)
    if marker in text:
        raise SystemExit(f"{path}: marker already present: {marker}")
    write(path, text.rstrip() + "\n\n" + block.strip() + "\n")


# O_NOFOLLOW is used only as a constant; no unsafe code is introduced.
replace_once(
    "crates/mini-consensus/Cargo.toml",
    'mini-bearer = { path = "../mini-bearer" }\n',
    'mini-bearer = { path = "../mini-bearer" }\nlibc = "0.2" # O_NOFOLLOW for race-free archive opens on Unix/Android\n',
)

replace_once(
    "crates/mini-consensus/src/error.rs",
    """    /// The peer/archive belongs to a different settlement network.
    StateSyncWrongNetwork,
    /// The peer no longer retains a checkpoint covering the request.
""",
    """    /// The peer/archive belongs to a different settlement network.
    StateSyncWrongNetwork,
    /// A state-sync response contains more finalized suffix blocks than the
    /// protocol's explicit per-response bound.
    StateSyncTooManyBlocks { maximum: usize, got: usize },
    /// The peer no longer retains a checkpoint covering the request.
""",
)
replace_once(
    "crates/mini-consensus/src/error.rs",
    """            ConsensusError::StateSyncWrongNetwork => {
                write!(f, "state-sync response belongs to another network")
            }
            ConsensusError::StateSyncUnavailable {
""",
    """            ConsensusError::StateSyncWrongNetwork => {
                write!(f, "state-sync response belongs to another network")
            }
            ConsensusError::StateSyncTooManyBlocks { maximum, got } => write!(
                f,
                "state-sync response contains {got} suffix blocks; maximum is {maximum}"
            ),
            ConsensusError::StateSyncUnavailable {
""",
)

replace_once(
    "crates/mini-consensus/src/store.rs",
    """pub struct ConsensusArchive {
    root: PathBuf,
    config: ConsensusArchiveConfig,
}

impl ConsensusArchive {
""",
    """pub struct ConsensusArchive {
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
""",
)

replace_once(
    "crates/mini-consensus/src/store.rs",
    """        reject_symlink_or_non_file_if_present(&root.join(LOCK_FILE), "consensus archive lock")?;
        let _ = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(LOCK_FILE))?;
""",
    """        let _ = open_archive_lock(&root.join(LOCK_FILE))?;
""",
)

replace_once(
    "crates/mini-consensus/src/store.rs",
    """        self.with_lock(|archive| {
            let pending = encode_pending_install(response, final_state)?;
            atomic_write(&archive.root.join(INSTALL_PENDING_FILE), &pending)?;
            archive.apply_install_locked(response, final_state)?;
            archive.clear_pending_install_locked()
        })
""",
    """        self.with_lock(|archive| {
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
""",
)

replace_once(
    "crates/mini-consensus/src/store.rs",
    """        let path = self.root.join(LOCK_FILE);
        reject_symlink_or_non_file_if_present(&path, "consensus archive lock")?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
""",
    """        let path = self.root.join(LOCK_FILE);
        let lock = open_archive_lock(&path)?;
""",
)

replace_once(
    "crates/mini-consensus/src/store.rs",
    """    fn apply_install_locked(
        &self,
        response: &StateSyncResponse,
        final_state: &LedgerState,
    ) -> Result<()> {
""",
    """    fn validate_install_locked(
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
""",
)

record_section = r'''    fn record_locked(&self, blocks: &[FinalizedBlock], final_state: &LedgerState) -> Result<()> {
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
                if let Some((_, planned)) = block_writes.iter().find(|(planned_height, _)| {
                    *planned_height == height
                }) {
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

'''
replace_section(
    "crates/mini-consensus/src/store.rs",
    "    fn record_locked(",
    "    fn install_snapshot_locked(",
    record_section,
)

snapshot_section = r'''    fn install_snapshot_locked(
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

'''
replace_section(
    "crates/mini-consensus/src/store.rs",
    "    fn install_snapshot_locked(",
    "    fn read_snapshot_locked(",
    snapshot_section,
)

read_section = r'''fn configure_no_follow(options: &mut OpenOptions) {
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
    options
        .create(true)
        .truncate(false)
        .read(true)
        .write(true);
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

'''
replace_section(
    "crates/mini-consensus/src/store.rs",
    "fn read_regular_limited(",
    "fn atomic_write(",
    read_section,
)

store_tests = r'''    #[test]
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
        let archive =
            ConsensusArchive::open(&root, ConsensusArchiveConfig::default()).unwrap();
        let target = root.join("outside-snapshot.bin");
        fs::write(&target, b"not a snapshot").unwrap();
        symlink(&target, root.join(SNAPSHOT_FILE)).unwrap();
        assert!(archive.recovery_response().is_err());
        let _ = fs::remove_file(root.join(SNAPSHOT_FILE));
        let _ = fs::remove_dir_all(root);
    }

'''
replace_once(
    "crates/mini-consensus/src/store.rs",
    """    #[cfg(unix)]
    #[test]
    fn symlinked_blocks_directory_is_refused() {
""",
    store_tests
    + """    #[cfg(unix)]
    #[test]
    fn symlinked_blocks_directory_is_refused() {
""",
)

replace_once(
    "crates/mini-consensus/src/snapshot_sync_tests.rs",
    """use crate::{
    ConsensusArchive, ConsensusArchiveConfig, ConsensusNode, NodeConfig, StateSyncResponse,
};
""",
    """use crate::{
    ConsensusArchive, ConsensusArchiveConfig, ConsensusError, ConsensusNode, ConsensusSnapshot,
    NodeConfig, StateSyncResponse,
};
""",
)

append_once(
    "crates/mini-consensus/src/snapshot_sync_tests.rs",
    "peer_block_state_sync_rejects_gap_duplicate_and_reordering_all_or_nothing",
    r'''
#[test]
fn peer_block_state_sync_rejects_gap_duplicate_and_reordering_all_or_nothing() {
    let fixture = fixture();
    let root = temp_root("peer-ordering");
    let (_archive, _source_chain, blocks) = build_archive(&root, &fixture);
    let cases = [
        (vec![blocks[0].clone(), blocks[2].clone()], 2, 3),
        (vec![blocks[0].clone(), blocks[0].clone()], 2, 1),
        (vec![blocks[1].clone(), blocks[0].clone()], 1, 2),
    ];

    for (malformed_suffix, expected, got) in cases {
        let encoded = StateSyncResponse::blocks(
            mini_settlement::MININET_NETWORK_ID,
            malformed_suffix,
        )
        .to_wire_bytes()
        .unwrap();
        let response = StateSyncResponse::from_wire_bytes(&encoded).unwrap();
        let mut destination = ConsensusNode::new(node_config(&fixture));
        let before = destination.commitment();
        assert_eq!(
            destination.apply_state_sync(response).unwrap_err(),
            ConsensusError::CatchupOutOfOrder { expected, got }
        );
        assert_eq!(destination.finalized_height(), 0);
        assert_eq!(destination.commitment(), before);
    }

    let _ = fs::remove_dir_all(root);
}

#[test]
fn peer_snapshot_state_sync_rejects_a_gapped_suffix_all_or_nothing() {
    let fixture = fixture();
    let root = temp_root("peer-snapshot-gap");
    let (_archive, _source_chain, blocks) = build_archive(&root, &fixture);
    let snapshot = ConsensusSnapshot::new(
        blocks[0].header.clone(),
        blocks[0].qc.clone(),
        LedgerChain::genesis().state().clone(),
    )
    .unwrap();
    let encoded = StateSyncResponse::snapshot(
        mini_settlement::MININET_NETWORK_ID,
        snapshot,
        vec![blocks[2].clone()],
    )
    .to_wire_bytes()
    .unwrap();
    let response = StateSyncResponse::from_wire_bytes(&encoded).unwrap();

    let mut destination = ConsensusNode::new(node_config(&fixture));
    let before = destination.commitment();
    assert_eq!(
        destination.apply_state_sync(response).unwrap_err(),
        ConsensusError::CatchupOutOfOrder {
            expected: 2,
            got: 3,
        }
    );
    assert_eq!(destination.finalized_height(), 0);
    assert_eq!(destination.commitment(), before);

    let _ = fs::remove_dir_all(root);
}
''',
)

print("applied PR #289 review fixes")

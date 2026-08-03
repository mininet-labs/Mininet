#!/usr/bin/env python3
"""Final D-0207 weak-device/archive semantics fixes; test and self-remove."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/consensus-snapshot-stage4.yml"


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:180]!r}")
    write(path, text.replace(old, new, 1))


def replace_regex(path: str, pattern: str, replacement: str) -> None:
    text = read(path)
    changed, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: expected one regex match, found {count}: {pattern[:180]!r}")
    write(path, changed)


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


# ---------------------------------------------------------------------------
# Avoid an 8 MiB full-state journal write on every ordinary finalized block.
# Atomic block rows plus snapshot-before-prune are enough for live commits:
# after a process loss the archive is either behind or contains a verified
# prefix, both of which are safely reverified/resynchronized. Peer replacement
# remains all-or-nothing under the exact install journal.
# ---------------------------------------------------------------------------
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''    pub\(crate\) fn record_verified_batch\(
        &self,
        blocks: &\[FinalizedBlock\],
        final_state: &LedgerState,
    \) -> Result<\(\)> \{
.*?
    \}

    /// Replace local recovery state''',
    '''    pub(crate) fn record_verified_batch(
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

    /// Replace local recovery state''',
)

# ---------------------------------------------------------------------------
# Compute response-base size without cloning a complete snapshot. Also permit
# a snapshot-only response when the first suffix block cannot share its frame;
# that is real progress and the caller can repeat from the snapshot height.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/state_sync.rs",
    '''    pub fn target_height(&self) -> Option<u64> {
        match &self.payload {
            StateSyncPayload::Blocks(blocks) => blocks.last().map(|block| block.header.height),
            StateSyncPayload::Snapshot { snapshot, blocks } => Some(
                blocks
                    .last()
                    .map_or(snapshot.height(), |block| block.header.height),
            ),
            StateSyncPayload::Unavailable { tip_height, .. } => Some(*tip_height),
            StateSyncPayload::WrongNetwork => None,
        }
    }

    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
''',
    '''    pub fn target_height(&self) -> Option<u64> {
        match &self.payload {
            StateSyncPayload::Blocks(blocks) => blocks.last().map(|block| block.header.height),
            StateSyncPayload::Snapshot { snapshot, blocks } => Some(
                blocks
                    .last()
                    .map_or(snapshot.height(), |block| block.header.height),
            ),
            StateSyncPayload::Unavailable { tip_height, .. } => Some(*tip_height),
            StateSyncPayload::WrongNetwork => None,
        }
    }

    /// Exact encoded length before adding any finalized suffix blocks. This
    /// serializes a snapshot once for its length but never clones its complete
    /// execution state merely to probe candidate page sizes.
    pub(crate) fn base_wire_len(snapshot: Option<&ConsensusSnapshot>) -> Result<usize> {
        // domain + network id + tag + block count
        let mut length = DOMAIN
            .len()
            .checked_add(32 + 1 + 4)
            .ok_or(ConsensusError::TooLarge)?;
        if let Some(snapshot) = snapshot {
            let snapshot_len = snapshot.to_wire_bytes()?.len();
            length = length
                .checked_add(4)
                .and_then(|value| value.checked_add(snapshot_len))
                .ok_or(ConsensusError::TooLarge)?;
        }
        if length > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(length)
    }

    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''        // Serialize the base response once\..*?
        let mut wire_size = base_response\.to_wire_bytes\(\)\?\.len\(\);
''',
    '''        // Account the base once without cloning a potentially 8 MiB
        // `LedgerState`. Each candidate is encoded once for its length; the
        // final response is constructed once after selection.
        let mut wire_size = StateSyncResponse::base_wire_len(
            use_snapshot.then(|| snapshot.as_ref().expect("use_snapshot implies snapshot")),
        )?;
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        if selected.is_empty() && tip_height > base_height {
            return Err(ConsensusError::TooLarge);
        }
''',
    '''        if selected.is_empty() && tip_height > base_height && !use_snapshot {
            return Err(ConsensusError::TooLarge);
        }
''',
)

# ---------------------------------------------------------------------------
# Monotonic archive comparison must consider both the durable snapshot and any
# suffix. Different valid QCs for the same finalized header are equivalent;
# conflict is the block/header, not certificate byte ordering.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        let current_snapshot = self.read_snapshot_locked()?;
        let current_blocks = self.read_blocks_locked()?;
        let current_tip = current_blocks.last().map_or(
            current_snapshot.as_ref().map_or(0, ConsensusSnapshot::height),
            |block| block.header.height,
        );
''',
    '''        let current_snapshot = self.read_snapshot_locked()?;
        let current_blocks = self.read_blocks_locked()?;
        let current_snapshot_height = current_snapshot
            .as_ref()
            .map_or(0, ConsensusSnapshot::height);
        let current_block_height = current_blocks
            .last()
            .map_or(0, |block| block.header.height);
        let current_tip = current_snapshot_height.max(current_block_height);
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        if let Some(current) = &current_snapshot {
            if current.height() == snapshot.height() && current != snapshot {
                return Err(ConsensusError::ArchiveConflict {
                    height: snapshot.height(),
                });
            }
        }
''',
    '''        if let Some(current) = &current_snapshot {
            if current.height() == snapshot.height()
                && current.header.hash() != snapshot.header.hash()
            {
                return Err(ConsensusError::ArchiveConflict {
                    height: snapshot.height(),
                });
            }
        }
''',
)

# Associated base-length evidence.
replace_once(
    "crates/mini-consensus/src/state_sync.rs",
    '''    #[test]
    fn control_responses_round_trip() {
''',
    '''    #[test]
    fn computed_empty_blocks_base_length_matches_the_real_wire() {
        let response = StateSyncResponse::blocks([7; 32], Vec::new());
        assert_eq!(
            StateSyncResponse::base_wire_len(None).unwrap(),
            response.to_wire_bytes().unwrap().len()
        );
    }

    #[test]
    fn control_responses_round_trip() {
''',
)

# Truth-sync the flash-wear claim while retaining the stronger peer-install
# journal guarantee.
replace_once(
    "docs/STATUS.md",
    '''  `ConsensusNode` applies a whole snapshot/suffix on a cloned chain (a bad late
  block changes nothing), caps compatibility history, journals live commits,
  and can reopen from the same verified archive path. Real TCP tests prove a
''',
    '''  `ConsensusNode` applies a whole snapshot/suffix on a cloned chain (a bad late
  block changes nothing), caps compatibility history, persists verified live
  block rows before chain swap without rewriting full-state journals per block,
  and can reopen from the same verified archive path. Real TCP tests prove a
''',
)
replace_once(
    "docs/planning/consensus-snapshot-sync.md",
    '''7. `ConsensusNode::apply_state_sync` verifies the entire response on a cloned
   chain before changing live or persistent state. A failure in the last block
   leaves the node at its original height. Live commits are also journaled
   before the node swaps its chain.
''',
    '''7. `ConsensusNode::apply_state_sync` verifies the entire response on a cloned
   chain before changing live or persistent state. A failure in the last block
   leaves the node at its original height. Verified live block rows are
   persisted before chain swap using atomic rows and snapshot-before-prune;
   peer snapshot/suffix replacement uses the replayable exact install journal.
''',
)

SELF.unlink()
WORKFLOW.unlink()

run("cargo", "fmt", "--all")
run("cargo", "test", "-p", "mini-consensus", "--all-features")
run("cargo", "test", "-p", "mini-execution", "-p", "mini-economy", "--all-features")
run(
    "cargo",
    "clippy",
    "-p",
    "mini-consensus",
    "-p",
    "mini-execution",
    "-p",
    "mini-economy",
    "--all-targets",
    "--all-features",
    "--",
    "-D",
    "warnings",
)

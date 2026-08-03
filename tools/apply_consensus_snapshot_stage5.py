#!/usr/bin/env python3
"""Close final D-0207 crash, bound, timeout, and truth-sync gaps; self-remove."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__)


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one literal match, found {count}: {old[:180]!r}")
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
# Persistent archive: remove interrupted block temp files, verify retained
# height/parent continuity before serving or appending, and refuse to advance
# a live node behind a damaged local archive.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        let mut blocks = self.read_blocks_locked()?;
        let snapshot_height = snapshot.as_ref().map_or(0, ConsensusSnapshot::height);
        blocks.retain(|block| block.header.height > snapshot_height);
        verify_contiguous(snapshot_height, &blocks)?;
''',
    '''        let mut blocks = self.read_blocks_locked()?;
        let snapshot_height = snapshot.as_ref().map_or(0, ConsensusSnapshot::height);
        let snapshot_hash = snapshot
            .as_ref()
            .map_or([0u8; 32], |value| value.header.hash());
        blocks.retain(|block| block.header.height > snapshot_height);
        verify_chain(snapshot_height, snapshot_hash, &blocks)?;
''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''        let existing = self\.read_blocks_locked\(\)\?;
        let mut suffix_bytes = existing\.iter\(\)\.try_fold\(0usize, \|total, block\| \{
            total
                \.checked_add\(block\.to_wire_bytes\(\)\?\.len\(\)\)
                \.ok_or\(ConsensusError::TooLarge\)
        \}\)\?;
        let mut tip_height = existing
            \.last\(\)
            \.map_or\(snapshot_height, \|block\| block\.header\.height\);''',
    '''        let snapshot_hash = current_snapshot
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
            .map_or(snapshot_hash, |block| block.header.hash());''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''            if height != expected {
                return Err(ConsensusError::CatchupOutOfOrder {
                    expected,
                    got: height,
                });
            }
            suffix_bytes = suffix_bytes
''',
    '''            if height != expected {
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
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''            atomic_write(&path, &bytes)?;
            tip_height = height;
''',
    '''            atomic_write(&path, &bytes)?;
            tip_height = height;
            tip_hash = block.header.hash();
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        verify_contiguous(snapshot.height(), blocks)?;
''',
    '''        verify_chain(snapshot.height(), snapshot.header.hash(), blocks)?;
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        let mut rows = Vec::new();
        let mut entries = 0usize;
        let mut total_bytes = 0usize;
''',
    '''        let mut rows = Vec::new();
        let mut stale_temps = Vec::new();
        let mut entries = 0usize;
        let mut total_bytes = 0usize;
''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''            let entry = entry\?;
            let file_type = entry\.file_type\(\)\?;
            if file_type\.is_symlink\(\) \{
                return Err\(ConsensusError::Storage\(
                    "symlink in consensus block archive"\.to_string\(\),
                \)\);
            \}
            if !file_type\.is_file\(\) \{
                return Err\(ConsensusError::Storage\(
                    "non-file in consensus block archive"\.to_string\(\),
                \)\);
            \}
            let height = parse_block_name\(&entry\.file_name\(\)\.to_string_lossy\(\)\)\?;
            let file_len =
                usize::try_from\(entry\.metadata\(\)\?\.len\(\)\)\.map_err\(\|_\| ConsensusError::TooLarge\)\?;
            total_bytes = total_bytes
                \.checked_add\(file_len\)
                \.ok_or\(ConsensusError::TooLarge\)\?;
            if total_bytes > MAX_ARCHIVE_RECOVERY_BYTES \{
                return Err\(ConsensusError::TooLarge\);
            \}
            let bytes = read_regular_limited\(''',
    '''            let entry = entry?;
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
            let bytes = read_regular_limited(''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        rows.sort_by_key(|block| block.header.height);
''',
    '''        let removed_temps = !stale_temps.is_empty();
        for path in stale_temps {
            remove_regular_if_present(&path, "interrupted consensus block write")?;
        }
        if removed_temps {
            sync_parent_directory(&dir)?;
        }
        rows.sort_by_key(|block| block.header.height);
''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''fn verify_contiguous\(base_height: u64, blocks: &\[FinalizedBlock\]\) -> Result<\(\)> \{
.*?
\}

fn parse_block_name''',
    '''fn verify_chain(
    base_height: u64,
    base_hash: [u8; 32],
    blocks: &[FinalizedBlock],
) -> Result<()> {
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

fn parse_block_name''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''    #[cfg(unix)]
    #[test]
    fn symlinked_blocks_directory_is_refused() {
''',
    '''    #[test]
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

    #[cfg(unix)]
    #[test]
    fn symlinked_blocks_directory_is_refused() {
''',
)

# ---------------------------------------------------------------------------
# Exact-state codec: calculate the complete encoded length before allocating
# the output buffer. Count caps alone are insufficient when many individually
# bounded beneficiary strings combine into a state far above 8 MiB.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-execution/src/snapshot.rs",
    '''        let mut out = Vec::new();
        out.extend_from_slice(DOMAIN);
''',
    '''        let exact_len = snapshot_encoded_len(self, &monetary)?;
        let mut out = Vec::with_capacity(exact_len);
        out.extend_from_slice(DOMAIN);
''',
)
replace_once(
    "crates/mini-execution/src/snapshot.rs",
    '''        if out.len() > MAX_LEDGER_SNAPSHOT_BYTES {
            return Err(ExecutionError::SnapshotTooLarge);
        }
        Ok(out)
    }

    /// Decode and independently validate exact ledger state.
''',
    '''        if out.len() != exact_len {
            return Err(ExecutionError::SnapshotMalformed);
        }
        Ok(out)
    }

    /// Decode and independently validate exact ledger state.
''',
)
replace_once(
    "crates/mini-execution/src/snapshot.rs",
    '''fn encode_monetary(out: &mut Vec<u8>, snapshot: &MonetaryLedgerSnapshot) -> Result<()> {
''',
    '''fn snapshot_encoded_len(
    state: &LedgerState,
    monetary: &MonetaryLedgerSnapshot,
) -> Result<usize> {
    let mut length = 0usize;
    add_snapshot_len(&mut length, DOMAIN.len() + 32)?;

    add_snapshot_len(&mut length, 4)?;
    for payer in state.finalized.keys() {
        let _ = u32::try_from(payer.len()).map_err(|_| ExecutionError::SnapshotTooLarge)?;
        add_snapshot_len(&mut length, 4 + payer.len() + 8 + 32)?;
    }

    add_snapshot_len(&mut length, 4)?;
    add_snapshot_len(
        &mut length,
        state
            .rejected
            .len()
            .checked_mul(33)
            .ok_or(ExecutionError::SnapshotTooLarge)?,
    )?;

    add_snapshot_len(&mut length, 16 + 16 + 16 + 1)?;
    if monetary.last_epoch.is_some() {
        add_snapshot_len(&mut length, 8)?;
    }
    add_snapshot_len(&mut length, 4)?;
    for position in &monetary.positions {
        add_snapshot_len(&mut length, 8 + 1)?;
        match &position.subject {
            VestingSubject::HumanSnapshot(_) => add_snapshot_len(&mut length, 32 + 8)?,
            VestingSubject::Beneficiary(beneficiary) => {
                if beneficiary.is_empty() {
                    return Err(ExecutionError::SnapshotMalformed);
                }
                if beneficiary.len() > MAX_SNAPSHOT_BENEFICIARY_BYTES {
                    return Err(ExecutionError::SnapshotTooLarge);
                }
                let _ = u32::try_from(beneficiary.len())
                    .map_err(|_| ExecutionError::SnapshotTooLarge)?;
                add_snapshot_len(&mut length, 4 + beneficiary.len())?;
            }
        }
        add_snapshot_len(&mut length, 1 + 16 + 16 + 8)?;
    }

    add_snapshot_len(&mut length, 4)?;
    for account in state.balances.keys() {
        let _ = u32::try_from(account.len()).map_err(|_| ExecutionError::SnapshotTooLarge)?;
        add_snapshot_len(&mut length, 4 + account.len() + 16)?;
    }
    add_snapshot_len(&mut length, 16 + 16)?;
    Ok(length)
}

fn add_snapshot_len(total: &mut usize, additional: usize) -> Result<()> {
    *total = total
        .checked_add(additional)
        .ok_or(ExecutionError::SnapshotTooLarge)?;
    if *total > MAX_LEDGER_SNAPSHOT_BYTES {
        return Err(ExecutionError::SnapshotTooLarge);
    }
    Ok(())
}

fn encode_monetary(out: &mut Vec<u8>, snapshot: &MonetaryLedgerSnapshot) -> Result<()> {
''',
)
replace_once(
    "crates/mini-execution/src/snapshot.rs",
    '''    #[test]
    fn oversized_input_fails_before_decode() {
''',
    '''    #[test]
    fn oversized_state_is_rejected_by_preflight_before_output_allocation() {
        let beneficiary = "b".repeat(MAX_SNAPSHOT_BENEFICIARY_BYTES);
        let count = MAX_LEDGER_SNAPSHOT_BYTES / (beneficiary.len() + 64) + 2;
        let positions: Vec<_> = (0..count)
            .map(|_| VestingPosition {
                epoch: 0,
                subject: VestingSubject::Beneficiary(beneficiary.clone()),
                channel: Channel::Service,
                amount: Amount::from_micro(1),
                starts_at_policy_ms: 0,
                duration_ms: 0,
            })
            .collect();
        let total = Amount::from_micro(count as u128);
        let monetary = MonetaryLedger::import_snapshot(MonetaryLedgerSnapshot {
            genesis_circulating: Amount::ZERO,
            total_issued: total,
            policy_time_ms: 0,
            last_epoch: Some(0),
            positions,
        })
        .unwrap();
        let mut state = LedgerState::new();
        state.monetary = monetary;
        state.unallocated_circulating = total;
        assert_eq!(
            snapshot_encoded_len(&state, &state.monetary.export_snapshot()),
            Err(ExecutionError::SnapshotTooLarge)
        );
        assert_eq!(
            state.to_snapshot_bytes(),
            Err(ExecutionError::SnapshotTooLarge)
        );
    }

    #[test]
    fn oversized_input_fails_before_decode() {
''',
)

# ---------------------------------------------------------------------------
# State-sync response encode: compute aggregate size before allocating the
# final response buffer, including locally constructed response values.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/state_sync.rs",
    '''    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
''',
    '''    pub(crate) fn wire_len(&self) -> Result<usize> {
        let mut length = DOMAIN
            .len()
            .checked_add(32 + 1)
            .ok_or(ConsensusError::TooLarge)?;
        match &self.payload {
            StateSyncPayload::WrongNetwork => {}
            StateSyncPayload::Unavailable { .. } => {
                length = length.checked_add(16).ok_or(ConsensusError::TooLarge)?;
            }
            StateSyncPayload::Blocks(blocks) => {
                length = checked_blocks_wire_len(length, blocks)?;
            }
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                let snapshot_len = snapshot.to_wire_bytes()?.len();
                length = length
                    .checked_add(4)
                    .and_then(|value| value.checked_add(snapshot_len))
                    .ok_or(ConsensusError::TooLarge)?;
                length = checked_blocks_wire_len(length, blocks)?;
            }
        }
        if length > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(length)
    }

    pub fn to_wire_bytes(&self) -> Result<Vec<u8>> {
        let exact_len = self.wire_len()?;
        let mut out = Vec::with_capacity(exact_len);
''',
)
replace_once(
    "crates/mini-consensus/src/state_sync.rs",
    '''        if out.len() > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
        Ok(out)
    }
}

fn encode_blocks''',
    '''        if out.len() != exact_len {
            return Err(ConsensusError::Malformed);
        }
        Ok(out)
    }
}

fn checked_blocks_wire_len(mut length: usize, blocks: &[FinalizedBlock]) -> Result<usize> {
    if blocks.len() > MAX_STATE_SYNC_BLOCKS {
        return Err(ConsensusError::TooLarge);
    }
    length = length.checked_add(4).ok_or(ConsensusError::TooLarge)?;
    for block in blocks {
        let block_len = block.to_wire_bytes()?.len();
        length = length
            .checked_add(4)
            .and_then(|value| value.checked_add(block_len))
            .ok_or(ConsensusError::TooLarge)?;
        if length > mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES {
            return Err(ConsensusError::TooLarge);
        }
    }
    Ok(length)
}

fn encode_blocks''',
)
replace_once(
    "crates/mini-consensus/src/state_sync.rs",
    '''mod tests {
    use super::*;

    #[test]
    fn request_round_trips_and_rejects_trailing_bytes() {
''',
    '''mod tests {
    use did_mini::Controller;
    use mini_chain::{BlockHeader, QuorumCertificate};
    use mini_execution::SettlementBlockBody;

    use super::*;

    fn block(height: u64) -> FinalizedBlock {
        let proposer = Controller::incept_single_from_seeds(&[1; 32], &[2; 32])
            .unwrap()
            .did();
        let header = BlockHeader {
            height,
            prev_hash: [0; 32],
            state_root: [0; 32],
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
            body: SettlementBlockBody::new(Vec::new()),
        }
    }

    #[test]
    fn response_count_over_cap_fails_before_final_buffer_allocation() {
        let response = StateSyncResponse::blocks(
            [3; 32],
            vec![block(1); MAX_STATE_SYNC_BLOCKS + 1],
        );
        assert_eq!(response.wire_len(), Err(ConsensusError::TooLarge));
        assert_eq!(response.to_wire_bytes(), Err(ConsensusError::TooLarge));
    }

    #[test]
    fn request_round_trips_and_rejects_trailing_bytes() {
''',
)

# ---------------------------------------------------------------------------
# TCP state sync: bound connect/read/write behavior and compute a fallible
# usize result before changing node state.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/net.rs",
    '''use crate::state_sync::{StateSyncRequest, StateSyncResponse};
''',
    '''use crate::state_sync::{StateSyncPayload, StateSyncRequest, StateSyncResponse};
''',
)
replace_once(
    "crates/mini-consensus/src/net.rs",
    '''//!   *discovery* (`mini-net`) and a bearer that redials are still separate,
//!   later work; so is state-sync for a node that was down and missed a whole
//!   height (re-gossip only re-delivers messages still circulating).
''',
    '''//!   *discovery* (`mini-net`) and a bearer that redials are still separate,
//!   later work. D-0207 state sync is available over a separately selected
//!   one-shot encrypted TCP peer, but discovery, peer selection, retry,
//!   multi-peer comparison, and background serving remain host responsibilities.
''',
)
replace_regex(
    "crates/mini-consensus/src/net.rs",
    r'''/// AEAD associated data for authenticated-snapshot state sync\..*?
const STATE_SYNC_AAD: &\[u8\] = b"mini-consensus/state-sync-channel/v1";

/// Pull one bounded blocks-or-snapshot response.*?
pub fn serve_state_sync_over_tcp\(archive: &ConsensusArchive, listener: &TcpListener\) -> Result<\(\)> \{
.*?
\}

/// AEAD associated data for every consensus frame''',
    '''/// AEAD associated data for authenticated-snapshot state sync. It is distinct
/// from live consensus and legacy block-only catch-up, preventing ciphertext
/// cross-protocol replay even though all three reuse the same Channel primitive.
const STATE_SYNC_AAD: &[u8] = b"mini-consensus/state-sync-channel/v1";

/// Maximum time one selected/accepted state-sync peer may spend on any
/// connect/read/write step. Peer selection and retry remain host policy, but a
/// malicious peer cannot hold this synchronous helper forever.
const STATE_SYNC_IO_TIMEOUT: Duration = Duration::from_secs(120);

fn configure_state_sync_stream(stream: &TcpStream, timeout: Duration) -> Result<()> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(mini_bearer::BearerError::from)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(mini_bearer::BearerError::from)?;
    Ok(())
}

fn open_state_sync_client(
    peer_addr: SocketAddr,
    timeout: Duration,
) -> Result<(TcpBearer, Channel)> {
    let stream = TcpStream::connect_timeout(&peer_addr, timeout)
        .map_err(mini_bearer::BearerError::from)?;
    configure_state_sync_stream(&stream, timeout)?;
    let mut bearer = TcpBearer::from_stream(stream)?;
    let (initiator, hello) = Initiator::start()?;
    bearer.send(&hello)?;
    let response = bearer.recv()?;
    let channel = initiator.finish(&response)?;
    Ok((bearer, channel))
}

fn accept_state_sync_server(
    listener: &TcpListener,
    timeout: Duration,
) -> Result<(TcpBearer, Channel)> {
    let (stream, _) = listener.accept().map_err(mini_bearer::BearerError::from)?;
    configure_state_sync_stream(&stream, timeout)?;
    let mut bearer = TcpBearer::from_stream(stream)?;
    let hello = bearer.recv()?;
    let (channel, hello_response) = Responder::respond(&hello)?;
    bearer.send(&hello_response)?;
    Ok((bearer, channel))
}

/// Pull one bounded blocks-or-snapshot response from an explicitly selected
/// peer and apply it only after local QC/state verification. The peer supplies
/// bytes, never trust. Repeat if the archive tip is more than one response away.
pub fn state_sync_over_tcp<O: ValidatorOracle>(
    node: &mut ConsensusNode<O>,
    peer_addr: SocketAddr,
) -> Result<usize> {
    state_sync_over_tcp_with_timeout(node, peer_addr, STATE_SYNC_IO_TIMEOUT)
}

fn state_sync_over_tcp_with_timeout<O: ValidatorOracle>(
    node: &mut ConsensusNode<O>,
    peer_addr: SocketAddr,
    timeout: Duration,
) -> Result<usize> {
    let before = node.finalized_height();
    let (mut bearer, mut channel) = open_state_sync_client(peer_addr, timeout)?;

    let request = StateSyncRequest {
        network_id: node.state().network_id(),
        from_height: before,
    };
    bearer.send(&channel.seal(&request.to_wire_bytes(), STATE_SYNC_AAD)?)?;
    let sealed_response = bearer.recv()?;
    let plaintext = channel.open(&sealed_response, STATE_SYNC_AAD)?;
    let response = StateSyncResponse::from_wire_bytes(&plaintext)?;
    let applied = match &response.payload {
        StateSyncPayload::Blocks(blocks) => blocks.len(),
        StateSyncPayload::Snapshot { snapshot, blocks } => {
            let target = blocks
                .last()
                .map_or(snapshot.height(), |block| block.header.height);
            let delta = target.checked_sub(before).unwrap_or(0);
            usize::try_from(delta).map_err(|_| ConsensusError::TooLarge)?
        }
        StateSyncPayload::WrongNetwork | StateSyncPayload::Unavailable { .. } => 0,
    };
    node.apply_state_sync(response)?;
    Ok(applied)
}

/// Serve one bounded state-sync request from a local non-authoritative archive.
/// The encrypted, anonymous transport does not authenticate the peer; it does
/// not need to, because the receiver independently verifies every payload.
pub fn serve_state_sync_over_tcp(archive: &ConsensusArchive, listener: &TcpListener) -> Result<()> {
    serve_state_sync_over_tcp_with_timeout(archive, listener, STATE_SYNC_IO_TIMEOUT)
}

fn serve_state_sync_over_tcp_with_timeout(
    archive: &ConsensusArchive,
    listener: &TcpListener,
    timeout: Duration,
) -> Result<()> {
    let (mut bearer, mut channel) = accept_state_sync_server(listener, timeout)?;
    let sealed_request = bearer.recv()?;
    let plaintext = channel.open(&sealed_request, STATE_SYNC_AAD)?;
    let request = StateSyncRequest::from_wire_bytes(&plaintext)?;
    let response = archive.response(request)?;
    bearer.send(&channel.seal(&response.to_wire_bytes()?, STATE_SYNC_AAD)?)?;
    Ok(())
}

/// AEAD associated data for every consensus frame''',
)
replace_once(
    "crates/mini-consensus/src/net.rs",
    '''mod tests {
    use super::*;

    #[test]
    fn a_peer_that_never_reads_cannot_block_us_or_grow_our_buffer_past_the_cap() {
''',
    '''mod tests {
    use super::*;

    #[test]
    fn a_state_sync_client_is_not_held_forever_by_a_silent_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });
        let started = Instant::now();
        assert!(open_state_sync_client(address, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        server.join().unwrap();
    }

    #[test]
    fn a_state_sync_server_is_not_held_forever_after_accept() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || {
            let _stream = TcpStream::connect(address).unwrap();
            std::thread::sleep(Duration::from_millis(250));
        });
        let started = Instant::now();
        assert!(accept_state_sync_server(&listener, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        client.join().unwrap();
    }

    #[test]
    fn a_peer_that_never_reads_cannot_block_us_or_grow_our_buffer_past_the_cap() {
''',
)

# ---------------------------------------------------------------------------
# Truth-sync stale claims and state the remaining filesystem/platform limits.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/lib.rs",
    '''//! - **Single-hop vote broadcast, not full gossip.** The host broadcasts each
//!   vote once; it does not re-gossip past rounds' votes. The crash-recovery
//!   path (a silent proposer) does not depend on that; the POLC-re-proposal
//!   path (paper line 28) does, so it is only as robust as the links are
//!   lossless. The *transport* no longer drops traffic to a merely-slow peer
//!   (see [`net::TcpMesh`]'s non-blocking buffered links), but a genuinely
//!   dropped or partitioned message is still not re-delivered.
''',
    '''//! - **Dedup-flooded gossip, not durable retransmission.** D-0205 re-gossips
//!   each newly seen proposal/vote across any connected topology, but there is
//!   no acknowledgement, retry queue, or historical replay after a partition.
//!   A genuinely dropped message may still require a later round to recover.
''',
)
replace_once(
    "crates/mini-consensus/src/lib.rs",
    '''//! gaps are liveness/DoS, transport security, and deployment, not correctness:
''',
    '''//! gaps are liveness/DoS, endpoint authentication/discovery, and deployment,
//! not finality correctness:
''',
)
replace_once(
    "docs/design/networked-consensus.md",
    '''remaining gaps are liveness/DoS, transport-security, and deployment, not
correctness:
''',
    '''remaining gaps are liveness/DoS, endpoint authentication/discovery, and
deployment, not finality correctness:
''',
)
replace_once(
    "docs/design/networked-consensus.md",
    '''- **`TcpMesh` is transport, not discovery or security.** Cleartext, addresses
  known up front, no reconnect, no NAT traversal. Authenticated encryption is
  `mini_bearer::Channel`'s job; overlay discovery/gossip is `mini-net`'s.
''',
    '''- **`TcpMesh` is encrypted transport, not endpoint authentication or
  discovery.** Every link uses the anonymous forward-secret
  `mini_bearer::Channel`; addresses are still known up front, peer identity is
  proved only by signed payloads, and there is no reconnect or NAT traversal.
  Overlay discovery/selection remains `mini-net`/host work.
''',
)
replace_once(
    "docs/STATUS.md",
    '''  **Implemented in this proposal (D-0207):** canonical complete
''',
    '''  **Implemented in this PR (D-0207):** canonical complete
''',
)
replace_once(
    "docs/planning/consensus-snapshot-sync.md",
    '''- archive snapshots/prunes/reopens, rejects corrupt/symlinked/oversized state,
  and replays an interrupted exact install journal idempotently;
''',
    '''- archive snapshots/prunes/reopens, rejects corrupt/symlinked/oversized state,
  removes an orphaned interrupted block temp, refuses a missing/gapped or
  wrong-parent suffix before appending, and replays an interrupted exact install
  journal idempotently;
''',
)
replace_once(
    "docs/planning/consensus-snapshot-sync.md",
    '''- response construction serializes a large snapshot base once, then accounts
  each candidate block once, rather than cloning an 8 MiB state per candidate;
''',
    '''- exact-state and state-sync response encoders calculate aggregate bounds
  before allocating their final output buffers; response selection serializes a
  large snapshot base once rather than cloning an 8 MiB state per candidate;
- selected and accepted state-sync peers are subject to connect/read/write
  deadlines, so one silent peer can delay only until the local timeout;
''',
)
replace_once(
    "docs/planning/consensus-snapshot-sync.md",
    '''- A local attacker able to erase/roll back the entire archive can deny or roll
  back local availability. QCs prevent invention of an unfinalized state, but
  no hardware monotonic counter or external checkpoint is introduced.
''',
    '''- A local attacker able to erase/roll back the entire archive can deny or roll
  back local availability. QCs prevent invention of an unfinalized state, but
  no hardware monotonic counter or external checkpoint is introduced.
- The archive directory must be owner-controlled. Static symlink/non-file checks
  fail closed, but pathname checks are not a capability-secure sandbox against a
  concurrent attacker who already controls the containing directory.
- Atomic replacement and parent-directory durability are exercised on Unix/Linux
  (including Android's filesystem model). Equivalent crash durability on
  non-Unix filesystems remains unproven and requires a platform adapter/test.
- Archive-enabled nodes fail closed: a local durability, size, or snapshot-write
  failure prevents that node from swapping to the newly finalized state. Very
  large validator certificates can therefore hit the one-frame snapshot ceiling;
  chunked state/QC transfer is the long-term solution, not silent data loss.
''',
)
replace_once(
    "docs/design/networked-consensus.md",
    '''  discovery/retry/multi-peer/eclipse policy, external audit, or physical
  weakest-device measurements. The equivocation evidence is no longer silently dropped by
''',
    '''  discovery/retry/multi-peer/eclipse policy, external audit, or physical
  weakest-device measurements. State-sync sockets have local I/O deadlines, but
  peer choice and retry remain host policy. The equivocation evidence is no longer silently dropped by
''',
) if False else None

# The final attempted replacement above belongs to STATUS, not this design file.
replace_once(
    "docs/STATUS.md",
    '''  discovery/retry/multi-peer/eclipse policy, external audit, or physical
  weakest-device measurements. The equivocation evidence is no longer silently dropped by
''',
    '''  discovery/retry/multi-peer/eclipse policy, external audit, or physical
  weakest-device measurements. State-sync sockets have local I/O deadlines, but
  peer choice and retry remain host policy. The equivocation evidence is no longer silently dropped by
''',
)

# Remove this one-shot helper before formatting/navigation so it never lands in
# the permanent tree or generated repository index.
SELF.unlink()

run("cargo", "fmt", "--all")
run(
    "cargo",
    "test",
    "-p",
    "mini-economy",
    "-p",
    "mini-execution",
    "-p",
    "mini-consensus",
    "--all-targets",
    "--all-features",
)
run(
    "cargo",
    "clippy",
    "-p",
    "mini-economy",
    "-p",
    "mini-execution",
    "-p",
    "mini-consensus",
    "--all-targets",
    "--all-features",
    "--",
    "-D",
    "warnings",
)
run("python3", "tools/mininet_nav.py", "build")

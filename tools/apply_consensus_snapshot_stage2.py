#!/usr/bin/env python3
"""Wire authenticated snapshots into nodes/TCP/archive recovery, test, self-remove."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/consensus-snapshot-stage2.yml"


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one literal match, found {count}: {old[:160]!r}")
    write(path, text.replace(old, new, 1))


def replace_regex(path: str, pattern: str, replacement: str) -> None:
    text = read(path)
    changed, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: expected one regex match, found {count}: {pattern[:160]!r}")
    write(path, changed)


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


# ---------------------------------------------------------------------------
# Persistent archive: replayable exact install journal.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/store.rs",
    "use mini_execution::LedgerState;\n",
    "use mini_execution::{LedgerState, MAX_LEDGER_SNAPSHOT_BYTES};\n",
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''const LOCK_FILE: &str = "archive.lock";
const TEMP_SUFFIX: &str = "tmp-write";
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize = 4_096;
''',
    '''const LOCK_FILE: &str = "archive.lock";
const INSTALL_PENDING_FILE: &str = "install.pending";
const INSTALL_PENDING_DOMAIN: &[u8] = b"mini-consensus/archive-install/v1";
const TEMP_SUFFIX: &str = "tmp-write";
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_PENDING_INSTALL_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES
    + MAX_LEDGER_SNAPSHOT_BYTES
    + INSTALL_PENDING_DOMAIN.len()
    + 8;
''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''    /// Replace local recovery state with an already verified snapshot response\.
    pub\(crate\) fn install_verified_response\(
        &self,
        response: &StateSyncResponse,
        final_state: &LedgerState,
    \) -> Result<\(\)> \{
.*?
    \}

    fn with_lock''',
    '''    /// Replace local recovery state with an already verified response.
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
            let pending = encode_pending_install(response, final_state)?;
            atomic_write(&archive.root.join(INSTALL_PENDING_FILE), &pending)?;
            archive.apply_install_locked(response, final_state)?;
            archive.clear_pending_install_locked()
        })
    }

    fn with_lock''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        #[allow(clippy::incompatible_msrv)]
        lock.lock()?;
        let result = operation(self);
        #[allow(clippy::incompatible_msrv)]
        let _ = lock.unlock();
        result
    }

    fn response_locked''',
    '''        #[allow(clippy::incompatible_msrv)]
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
        )? else {
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

    fn response_locked''',
)

pending_helpers = r'''
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
    if bytes.len() > MAX_PENDING_INSTALL_BYTES
        || !bytes.starts_with(INSTALL_PENDING_DOMAIN)
    {
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
    let length_end = position
        .checked_add(4)
        .ok_or(ConsensusError::Malformed)?;
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

'''
replace_once(
    "crates/mini-consensus/src/store.rs",
    "fn verify_contiguous(base_height: u64, blocks: &[FinalizedBlock]) -> Result<()> {\n",
    pending_helpers + "fn verify_contiguous(base_height: u64, blocks: &[FinalizedBlock]) -> Result<()> {\n",
)

pending_tests = r'''
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
        let snapshot = ConsensusSnapshot::new(
            second.header.clone(),
            second.qc.clone(),
            state.clone(),
        )
        .unwrap();
        let response = StateSyncResponse::snapshot(
            config.network_id,
            snapshot.clone(),
            Vec::new(),
        );
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

'''
replace_once(
    "crates/mini-consensus/src/store.rs",
    "    #[cfg(unix)]\n    #[test]\n    fn symlinked_blocks_directory_is_refused() {\n",
    pending_tests + "    #[cfg(unix)]\n    #[test]\n    fn symlinked_blocks_directory_is_refused() {\n",
)

# ---------------------------------------------------------------------------
# Consensus node: all-or-nothing application, bounded history, archive recovery.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/node.rs",
    '''use crate::round::{proposer_for, Action, Round, Step};
use crate::wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal};
''',
    '''use crate::round::{proposer_for, Action, Round, Step};
use crate::state_sync::{StateSyncPayload, StateSyncRequest, StateSyncResponse};
use crate::store::ConsensusArchive;
use crate::wire::{sign_proposal, verify_proposal, ConsensusMessage, Proposal};
''',
)
replace_once(
    "crates/mini-consensus/src/node.rs",
    '''    /// Every block this node has finalized, kept so a peer that fell behind
    /// can be served a catch-up response (see [`crate::catchup`]). First
    /// slice: unbounded, in-memory, no pruning/persistence — the same
    /// honest-limit shape `mini-net`'s `RoutingTable` documents for its own
    /// first slice.
    history: Vec<FinalizedBlock>,
''',
    '''    /// Recent finalized blocks available for the compatibility catch-up
    /// endpoint. The deque is strictly capped; durable history/snapshots live
    /// in `archive` when configured.
    history: VecDeque<FinalizedBlock>,
    /// Optional local, non-authoritative persistent recovery archive.
    archive: Option<ConsensusArchive>,
''',
)
replace_once(
    "crates/mini-consensus/src/node.rs",
    '''            values: HashMap::new(),
            pending: Vec::new(),
            history: Vec::new(),
        }
    }

    /// The height currently being decided''',
    '''            values: HashMap::new(),
            pending: Vec::new(),
            history: VecDeque::new(),
            archive: None,
        }
    }

    /// Stand up a node and independently recover the best locally retained
    /// snapshot/suffix. The archive is storage, never a trust oracle: every
    /// checkpoint QC and every suffix block are verified through the same
    /// validator set, KEL oracle, and execution path as network state sync.
    pub fn new_with_archive(
        config: NodeConfig<O>,
        archive: ConsensusArchive,
    ) -> Result<Self> {
        let mut node = Self::new(config);
        if archive.network_id() != node.state().network_id() {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        node.archive = Some(archive.clone());
        loop {
            let before = node.finalized_height();
            let response = archive.response(StateSyncRequest {
                network_id: node.state().network_id(),
                from_height: before,
            })?;
            node.apply_state_sync_internal(response, false, false)?;
            if node.finalized_height() == before {
                break;
            }
        }
        Ok(node)
    }

    /// The height currently being decided''',
)
replace_regex(
    "crates/mini-consensus/src/node.rs",
    r'''    pub fn catch_up\(&mut self, blocks: Vec<FinalizedBlock>\) -> Result<Vec<Emit>> \{
.*?
    \}

    /// Feed one message''',
    '''    pub fn catch_up(&mut self, blocks: Vec<FinalizedBlock>) -> Result<Vec<Emit>> {
        self.apply_state_sync(StateSyncResponse::blocks(
            self.state().network_id(),
            blocks,
        ))
    }

    /// Apply one peer/archive response all-or-nothing. Every block is first
    /// executed against a cloned chain; a late failure leaves live state and
    /// persistent state unchanged. A snapshot replaces state only after its
    /// QC and state commitment verify against this node's own validator data.
    pub fn apply_state_sync(&mut self, response: StateSyncResponse) -> Result<Vec<Emit>> {
        self.apply_state_sync_internal(response, true, true)
    }

    fn apply_state_sync_internal(
        &mut self,
        response: StateSyncResponse,
        start_round: bool,
        persist: bool,
    ) -> Result<Vec<Emit>> {
        if response.network_id != self.state().network_id() {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        let before = self.finalized_height();
        let (candidate, mut emits, retained, replace_history) =
            self.verify_state_sync(&response)?;
        if candidate.height() == before {
            return Ok(Vec::new());
        }

        if persist {
            if let Some(archive) = self.archive.clone() {
                archive.install_verified_response(&response, candidate.state())?;
            }
        }

        self.chain = candidate;
        if replace_history {
            self.history.clear();
        }
        for block in retained {
            self.remember_history(block);
        }
        self.round = Round::new(
            self.current_height(),
            self.validators.clone(),
            self.root.clone(),
        );
        self.values.clear();
        // Buffered live-round messages were collected relative to the old tip.
        // Re-gossip can repopulate them; replaying them after a state jump is a
        // larger ambiguity than dropping them.
        self.pending.clear();
        if start_round {
            let actions = self.round.start();
            self.drive(actions, &mut emits)?;
        }
        Ok(emits)
    }

    fn verify_state_sync(
        &self,
        response: &StateSyncResponse,
    ) -> Result<(LedgerChain, Vec<Emit>, Vec<FinalizedBlock>, bool)> {
        match &response.payload {
            StateSyncPayload::WrongNetwork => Err(ConsensusError::StateSyncWrongNetwork),
            StateSyncPayload::Unavailable {
                earliest_height,
                tip_height,
            } => Err(ConsensusError::StateSyncUnavailable {
                earliest_height: *earliest_height,
                tip_height: *tip_height,
            }),
            StateSyncPayload::Blocks(blocks) => {
                let mut candidate = self.chain.clone();
                let mut emits = Vec::new();
                for block in blocks {
                    let expected = candidate
                        .height()
                        .checked_add(1)
                        .ok_or(ConsensusError::TooLarge)?;
                    if block.header.height != expected {
                        return Err(ConsensusError::CatchupOutOfOrder {
                            expected,
                            got: block.header.height,
                        });
                    }
                    let commitment = candidate.apply_finalized_block(
                        &block.header,
                        &block.body,
                        &block.qc,
                        &self.validators,
                        &self.oracle,
                    )?;
                    emits.push(Emit::Committed {
                        height: block.header.height,
                        commitment,
                    });
                }
                Ok((candidate, emits, blocks.clone(), false))
            }
            StateSyncPayload::Snapshot { snapshot, blocks } => {
                if snapshot.height() <= self.finalized_height() {
                    return Err(ConsensusError::SnapshotNotNewer {
                        current: self.finalized_height(),
                        got: snapshot.height(),
                    });
                }
                let mut candidate = snapshot.clone().into_chain(
                    response.network_id,
                    &self.validators,
                    &self.oracle,
                )?;
                let mut emits = vec![Emit::Committed {
                    height: snapshot.height(),
                    commitment: candidate.state().commitment(),
                }];
                for block in blocks {
                    let expected = candidate
                        .height()
                        .checked_add(1)
                        .ok_or(ConsensusError::TooLarge)?;
                    if block.header.height != expected {
                        return Err(ConsensusError::CatchupOutOfOrder {
                            expected,
                            got: block.header.height,
                        });
                    }
                    let commitment = candidate.apply_finalized_block(
                        &block.header,
                        &block.body,
                        &block.qc,
                        &self.validators,
                        &self.oracle,
                    )?;
                    emits.push(Emit::Committed {
                        height: block.header.height,
                        commitment,
                    });
                }
                Ok((candidate, emits, blocks.clone(), true))
            }
        }
    }

    fn remember_history(&mut self, block: FinalizedBlock) {
        self.history.push_back(block);
        while self.history.len() > MAX_CATCHUP_BLOCKS {
            self.history.pop_front();
        }
    }

    /// Feed one message''',
)
replace_regex(
    "crates/mini-consensus/src/node.rs",
    r'''    fn commit\(&mut self, qc: QuorumCertificate, emits: &mut Vec<Emit>\) -> Result<\(\)> \{
.*?
    \}
\}

#\[cfg\(test\)\]''',
    '''    fn commit(&mut self, qc: QuorumCertificate, emits: &mut Vec<Emit>) -> Result<()> {
        let value = self
            .values
            .get(&qc.block_hash)
            .ok_or(ConsensusError::Stalled)?
            .clone();
        let mut candidate = self.chain.clone();
        let commitment = candidate.apply_finalized_block(
            &value.header,
            &value.body,
            &qc,
            &self.validators,
            &self.oracle,
        )?;
        let finalized = FinalizedBlock {
            header: value.header.clone(),
            body: value.body.clone(),
            qc,
        };
        if let Some(archive) = self.archive.clone() {
            archive.record_verified_batch(core::slice::from_ref(&finalized), candidate.state())?;
        }
        self.chain = candidate;
        self.remember_history(finalized);
        emits.push(Emit::Committed {
            height: value.header.height,
            commitment,
        });

        self.round = Round::new(
            self.current_height(),
            self.validators.clone(),
            self.root.clone(),
        );
        self.values.clear();
        let start_actions = self.round.start();
        self.drive(start_actions, emits)?;
        let replay = core::mem::take(&mut self.pending);
        for msg in replay {
            self.ingest(msg, emits)?;
        }
        Ok(())
    }
}

#[cfg(test)]''',
)

# ---------------------------------------------------------------------------
# Encrypted one-shot TCP state sync.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/net.rs",
    '''use crate::node::{ConsensusNode, Emit};
use crate::wire::ConsensusMessage;
''',
    '''use crate::node::{ConsensusNode, Emit};
use crate::state_sync::{StateSyncRequest, StateSyncResponse};
use crate::store::ConsensusArchive;
use crate::wire::ConsensusMessage;
''',
)
state_sync_net = r'''
/// AEAD associated data for authenticated-snapshot state sync. It is distinct
/// from live consensus and legacy block-only catch-up, preventing ciphertext
/// cross-protocol replay even though all three reuse the same Channel primitive.
const STATE_SYNC_AAD: &[u8] = b"mini-consensus/state-sync-channel/v1";

/// Pull one bounded blocks-or-snapshot response from an explicitly selected
/// peer and apply it only after local QC/state verification. The peer supplies
/// bytes, never trust. Repeat if the archive tip is more than one response away.
pub fn state_sync_over_tcp<O: ValidatorOracle>(
    node: &mut ConsensusNode<O>,
    peer_addr: SocketAddr,
) -> Result<usize> {
    let before = node.finalized_height();
    let stream = TcpStream::connect(peer_addr).map_err(mini_bearer::BearerError::from)?;
    let mut bearer = TcpBearer::from_stream(stream)?;

    let (initiator, hello) = Initiator::start()?;
    bearer.send(&hello)?;
    let response = bearer.recv()?;
    let mut channel = initiator.finish(&response)?;

    let request = StateSyncRequest {
        network_id: node.state().network_id(),
        from_height: before,
    };
    bearer.send(&channel.seal(&request.to_wire_bytes(), STATE_SYNC_AAD)?)?;
    let sealed_response = bearer.recv()?;
    let plaintext = channel.open(&sealed_response, STATE_SYNC_AAD)?;
    let response = StateSyncResponse::from_wire_bytes(&plaintext)?;
    node.apply_state_sync(response)?;
    usize::try_from(node.finalized_height().saturating_sub(before))
        .map_err(|_| ConsensusError::TooLarge)
}

/// Serve one bounded state-sync request from a local non-authoritative archive.
/// The encrypted, anonymous transport does not authenticate the peer; it does
/// not need to, because the receiver independently verifies every payload.
pub fn serve_state_sync_over_tcp(
    archive: &ConsensusArchive,
    listener: &TcpListener,
) -> Result<()> {
    let (stream, _) = listener.accept().map_err(mini_bearer::BearerError::from)?;
    let mut bearer = TcpBearer::from_stream(stream)?;

    let hello = bearer.recv()?;
    let (mut channel, hello_response) = Responder::respond(&hello)?;
    bearer.send(&hello_response)?;

    let sealed_request = bearer.recv()?;
    let plaintext = channel.open(&sealed_request, STATE_SYNC_AAD)?;
    let request = StateSyncRequest::from_wire_bytes(&plaintext)?;
    let response = archive.response(request)?;
    bearer.send(&channel.seal(&response.to_wire_bytes()?, STATE_SYNC_AAD)?)?;
    Ok(())
}

'''
replace_once(
    "crates/mini-consensus/src/net.rs",
    "/// AEAD associated data for every consensus frame sealed over a link's\n",
    state_sync_net + "/// AEAD associated data for every consensus frame sealed over a link's\n",
)

# Include the permanent in-crate real-TCP/restart test module.
replace_once(
    "crates/mini-consensus/src/lib.rs",
    '''mod snapshot;
mod state_sync;
mod store;
mod wire;

pub mod net;
''',
    '''mod snapshot;
mod state_sync;
mod store;
mod wire;

#[cfg(test)]
mod snapshot_sync_tests;

pub mod net;
''',
)

# One-shot machinery is not part of the permanent PR.
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

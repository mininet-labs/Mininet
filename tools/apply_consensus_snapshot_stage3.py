#!/usr/bin/env python3
"""Finish D-0207 bounds, adversarial tests, docs, and exact checks; self-remove."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/consensus-snapshot-stage3.yml"


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
# Weak-device and crash bounds: one response-sized stable suffix, at most one
# additional response-sized batch during journal replay, and O(snapshot + n)
# response construction instead of cloning/serializing the snapshot per block.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''const TEMP_SUFFIX: &str = "tmp-write";
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize = 4_096;
const MAX_PENDING_INSTALL_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES
''',
    '''const TEMP_SUFFIX: &str = "tmp-write";
/// A stable suffix must fit one encrypted state-sync response by itself.
const MAX_ARCHIVE_SUFFIX_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES;
/// A crash can leave the old stable suffix plus one journaled response batch.
/// Recovery may inspect both, but successful replay compacts back below the
/// stable bound before removing the journal.
const MAX_ARCHIVE_RECOVERY_BYTES: usize =
    MAX_ARCHIVE_SUFFIX_BYTES + mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES;
const MAX_ARCHIVE_DIRECTORY_ENTRIES: usize =
    MAX_CATCHUP_BLOCKS + MAX_STATE_SYNC_BLOCKS;
const MAX_PENDING_INSTALL_BYTES: usize = mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES
''',
)
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

        // Live commits use the same replayable exact journal as peer state
        // sync. This closes the crash window between a durable block row and
        // the snapshot/prune operation that may immediately follow it.
        let response = StateSyncResponse::blocks(self.config.network_id, blocks.to_vec());
        self.install_verified_response(&response, final_state)
    }

    /// Replace local recovery state''',
)
replace_regex(
    "crates/mini-consensus/src/store.rs",
    r'''        let mut selected = Vec::new\(\);
        for block in candidates\.into_iter\(\)\.take\(MAX_STATE_SYNC_BLOCKS\) \{
.*?
        \}

        if selected\.is_empty\(\) && tip_height > base_height \{
            let base_response = if use_snapshot \{
.*?
            base_response\.to_wire_bytes\(\)\?;
            return Err\(ConsensusError::TooLarge\);
        \}
''',
    '''        // Serialize the base response once. The previous implementation
        // cloned and re-serialized a potentially 8 MiB snapshot for every
        // candidate block, turning a 256-block page into gigabytes of needless
        // weak-device work. Each candidate is now encoded once for its length;
        // the final response is constructed once after selection.
        let base_response = if use_snapshot {
            StateSyncResponse::snapshot(
                self.config.network_id,
                snapshot.clone().expect("use_snapshot implies snapshot"),
                Vec::new(),
            )
        } else {
            StateSyncResponse::blocks(self.config.network_id, Vec::new())
        };
        let mut wire_size = base_response.to_wire_bytes()?.len();
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

        if selected.is_empty() && tip_height > base_height {
            return Err(ConsensusError::TooLarge);
        }
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''    fn install_snapshot_locked(
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
''',
    '''    fn install_snapshot_locked(
        &self,
        snapshot: &ConsensusSnapshot,
        blocks: &[FinalizedBlock],
        final_state: &LedgerState,
    ) -> Result<()> {
        if snapshot.network_id() != self.config.network_id || blocks.len() > MAX_STATE_SYNC_BLOCKS {
            return Err(ConsensusError::StateSyncWrongNetwork);
        }
        verify_contiguous(snapshot.height(), blocks)?;
        let incoming_tip = blocks
            .last()
            .map_or(snapshot.height(), |block| block.header.height);
        let current_snapshot = self.read_snapshot_locked()?;
        let current_blocks = self.read_blocks_locked()?;
        let current_tip = current_blocks.last().map_or(
            current_snapshot.as_ref().map_or(0, ConsensusSnapshot::height),
            |block| block.header.height,
        );
        if incoming_tip < current_tip {
            return Err(ConsensusError::SnapshotNotNewer {
                current: current_tip,
                got: incoming_tip,
            });
        }
        if let Some(current) = &current_snapshot {
            if current.height() == snapshot.height() && current != snapshot {
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

        self.write_snapshot_locked(snapshot)?;
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        let existing = self.read_blocks_locked()?;
        let mut tip_height = existing
            .last()
            .map_or(snapshot_height, |block| block.header.height);

        for block in blocks {
''',
    '''        let existing = self.read_blocks_locked()?;
        let mut suffix_bytes = existing.iter().try_fold(0usize, |total, block| {
            total
                .checked_add(block.to_wire_bytes()?.len())
                .ok_or(ConsensusError::TooLarge)
        })?;
        let mut tip_height = existing
            .last()
            .map_or(snapshot_height, |block| block.header.height);

        for block in blocks {
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''            atomic_write(&path, &bytes)?;
            tip_height = height;
        }

        let last = blocks.last().expect("non-empty checked above");
''',
    '''            suffix_bytes = suffix_bytes
                .checked_add(bytes.len())
                .ok_or(ConsensusError::TooLarge)?;
            if suffix_bytes > MAX_ARCHIVE_RECOVERY_BYTES {
                return Err(ConsensusError::TooLarge);
            }
            atomic_write(&path, &bytes)?;
            tip_height = height;
        }

        let last = blocks.last().expect("non-empty checked above");
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        if last.header.height % self.config.snapshot_interval == 0
            || distance >= self.config.max_suffix_blocks as u64
        {
''',
    '''        if last.header.height % self.config.snapshot_interval == 0
            || distance >= self.config.max_suffix_blocks as u64
            || suffix_bytes >= MAX_ARCHIVE_SUFFIX_BYTES
        {
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''        let mut rows = Vec::new();
        let mut entries = 0usize;
        for entry in fs::read_dir(&dir)? {
            entries = entries.checked_add(1).ok_or(ConsensusError::TooLarge)?;
            if entries > MAX_ARCHIVE_DIRECTORY_ENTRIES {
                return Err(ConsensusError::TooLarge);
            }
            let entry = entry?;
''',
    '''        let mut rows = Vec::new();
        let mut entries = 0usize;
        let mut total_bytes = 0usize;
        for entry in fs::read_dir(&dir)? {
            entries = entries.checked_add(1).ok_or(ConsensusError::TooLarge)?;
            if entries > MAX_ARCHIVE_DIRECTORY_ENTRIES {
                return Err(ConsensusError::TooLarge);
            }
            let entry = entry?;
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''            let height = parse_block_name(&entry.file_name().to_string_lossy())?;
            let bytes = read_regular_limited(
''',
    '''            let height = parse_block_name(&entry.file_name().to_string_lossy())?;
            let file_len = usize::try_from(entry.metadata()?.len())
                .map_err(|_| ConsensusError::TooLarge)?;
            total_bytes = total_bytes
                .checked_add(file_len)
                .ok_or(ConsensusError::TooLarge)?;
            if total_bytes > MAX_ARCHIVE_RECOVERY_BYTES {
                return Err(ConsensusError::TooLarge);
            }
            let bytes = read_regular_limited(
''',
)

# Encode-side symmetry: never build a state snapshot that its decoder/importer
# would reject solely because an internal beneficiary escaped the cap.
replace_once(
    "crates/mini-execution/src/snapshot.rs",
    '''            VestingSubject::Beneficiary(beneficiary) => {
                out.push(1);
                put_bytes(out, beneficiary.as_bytes())?;
            }
''',
    '''            VestingSubject::Beneficiary(beneficiary) => {
                if beneficiary.is_empty() {
                    return Err(ExecutionError::SnapshotMalformed);
                }
                if beneficiary.len() > MAX_SNAPSHOT_BENEFICIARY_BYTES {
                    return Err(ExecutionError::SnapshotTooLarge);
                }
                out.push(1);
                put_bytes(out, beneficiary.as_bytes())?;
            }
''',
)

# ---------------------------------------------------------------------------
# Permanent adversarial tests.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/snapshot.rs",
    '''    #[test]
    fn wrong_network_and_tampered_state_fail_closed() {
''',
    '''    #[test]
    fn a_structurally_matching_but_non_quorate_qc_is_rejected_locally() {
        let (snapshot, validators, directory) = proof();
        let mut qc = snapshot.qc.clone();
        qc.votes.clear();
        let structural = ConsensusSnapshot::new(
            snapshot.header.clone(),
            qc,
            snapshot.state.clone(),
        )
        .unwrap();
        assert!(structural
            .into_chain(mini_settlement::MININET_NETWORK_ID, &validators, &directory)
            .is_err());
    }

    #[test]
    fn wrong_network_and_tampered_state_fail_closed() {
''',
)
replace_once(
    "crates/mini-consensus/src/node.rs",
    '''    #[test]
    fn a_proposal_from_the_designated_proposer_is_prevoted() {
''',
    '''    #[test]
    fn compatibility_history_is_strictly_capped() {
        let fx = fixture();
        let mut node = a_node(&fx, 0);
        for height in 1..=(MAX_CATCHUP_BLOCKS as u64 + 5) {
            let header = BlockHeader {
                height,
                prev_hash: [0; 32],
                state_root: [0; 32],
                timestamp_ms: height,
                proposer: fx.signers[0].0.did(),
            };
            node.remember_history(FinalizedBlock {
                qc: QuorumCertificate {
                    height,
                    round: 0,
                    block_hash: header.hash(),
                    votes: Vec::new(),
                },
                header,
                body: SettlementBlockBody::new(Vec::new()),
            });
        }
        assert_eq!(node.history.len(), MAX_CATCHUP_BLOCKS);
        assert_eq!(node.history.front().unwrap().header.height, 6);
        assert_eq!(
            node.history.back().unwrap().header.height,
            MAX_CATCHUP_BLOCKS as u64 + 5
        );
    }

    #[test]
    fn a_proposal_from_the_designated_proposer_is_prevoted() {
''',
)
replace_once(
    "crates/mini-consensus/src/store.rs",
    '''    #[test]
    fn wrong_network_is_a_control_response_not_trust() {
''',
    '''    #[test]
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
        assert!(response.to_wire_bytes().unwrap().len()
            <= mini_bearer::MAX_CHANNEL_PLAINTEXT_BYTES);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_network_is_a_control_response_not_trust() {
''',
)

# ---------------------------------------------------------------------------
# Truth-sync crate docs.
# ---------------------------------------------------------------------------
replace_once(
    "crates/mini-consensus/src/catchup.rs",
    '''//! - **No unbounded history.** A serving node only answers from whatever it
//!   still holds in memory (see [`crate::node::ConsensusNode::history_since`]);
//!   there is no persistence or pruning policy yet — a first slice, the same
//!   honest-limit shape `mini-net`'s `RoutingTable`/`GossipRouter` document
//!   for their own first-slice bounds.
//! - **No partial-batch application.** [`CatchupResponse::from_wire_bytes`]
//!   bounds the count before allocating ([`MAX_CATCHUP_BLOCKS`]), but a
//!   response that fails partway through `catch_up` leaves the node at
//!   whatever height it reached before the failing block — never silently
//!   further, never rolled back.
''',
    '''//! - **Compatibility suffix only.** The in-memory compatibility history is
//!   now capped at [`MAX_CATCHUP_BLOCKS`]. Durable restart recovery, authenticated
//!   snapshots, and pruning live in [`crate::ConsensusArchive`] / the D-0207
//!   state-sync path; this legacy request/response remains for block-only peers.
//! - **All-or-nothing application.** [`CatchupResponse::from_wire_bytes`]
//!   bounds the count before allocating, and [`crate::ConsensusNode::catch_up`]
//!   executes the entire batch against a cloned chain before swapping live
//!   state. A bad later block leaves the node at its original height.
''',
)
replace_once(
    "crates/mini-consensus/src/lib.rs",
    '''//! - **No unbounded history.** A serving node only answers from whatever it
//!   still holds in memory (see [`crate::node::ConsensusNode::history_since`]);
//!   there is no persistence or pruning policy yet — a first slice, the same
//!   honest-limit shape `mini-net`'s `RoutingTable` documents for its own
//!   first slice.
''',
    '''//! - **State sync is bounded and persistent, but deliberately static-set.**
//!   D-0207 adds QC-bound exact execution snapshots, a local journaled archive,
//!   bounded recent history, pruning, restart recovery, and encrypted
//!   snapshot-plus-suffix transfer. The receiver verifies every QC and state
//!   commitment locally; the peer/archive has no authority. One response is
//!   limited to one bearer frame and one exact state is capped at 8 MiB.
//!   Historical validator-set transitions, long-range/weak-subjectivity rules,
//!   chunked Merkle state transfer, peer selection/retry, and physical weakest-
//!   device benchmarks remain separate work.
''',
)

# ---------------------------------------------------------------------------
# Planning, design, status, and append-only decision truth.
# ---------------------------------------------------------------------------
write(
    "docs/planning/consensus-snapshot-sync.md",
    '''# Consensus authenticated snapshots, persistent catch-up, and bounded pruning (D-0207)

**Status:** implementation complete in draft PR #289; no merge, production,
release, or activation claim until exact-head CI and review complete.  
**Roadmap:** issue #45.  
**Base:** D-0093 block-only catch-up and D-0200–D-0206 networked consensus.

## Problem

D-0093 lets a late node pull bounded finalized blocks over the existing
encrypted channel and reapply every block through `LedgerChain::
apply_finalized_block`. Its first slice retained an unbounded in-memory vector,
lost all catch-up history on restart, and could not help a node older than the
retained suffix. A hosted checkpoint service would solve availability by
creating a new trust and censorship point. D-0207 keeps recovery local and
makes any serving peer an untrusted byte source.

## Implemented mechanism

1. `mini-economy::MonetaryLedgerSnapshot` exports/imports exact issuance and
   vesting state under count, subject, epoch, and checked-supply invariants.
2. `mini-execution::LedgerState::{to_snapshot_bytes,from_snapshot_bytes}`
   provides one canonical, byte-bounded complete-state codec. Decode enforces
   ordered maps, account/rejection tags, monetary invariants, supply
   conservation, balance totals, and byte-exact re-encoding.
3. `LedgerChain::from_finalized_snapshot` accepts state only when the receiver's
   own static `ValidatorSet`/KEL oracle verifies the QC, the QC names the exact
   header, the logical timestamp is deterministic, the settlement-network id
   matches, and the header state root equals the decoded state commitment.
4. `mini-consensus::ConsensusSnapshot` binds that header, QC, and state. A peer
   or disk file that merely parses receives no authority.
5. `StateSyncRequest/Response` carries either a contiguous finalized-block
   suffix, one authenticated snapshot plus a suffix, or an explicit wrong-
   network/unavailable result. Counts, fields, exact state, and the whole
   response are bounded before allocation.
6. `ConsensusArchive` is a local, optional, non-authoritative filesystem cache:
   OS-backed cross-process lock, regular-file/symlink checks, file and
   containing-directory sync on Unix, exact install journal, replay after
   interruption, periodic snapshots, a count/byte-bounded suffix, and pruning
   only after a replacement snapshot is durable.
7. `ConsensusNode::apply_state_sync` verifies the entire response on a cloned
   chain before changing live or persistent state. A failure in the last block
   leaves the node at its original height. Live commits are also journaled
   before the node swaps its chain.
8. `state_sync_over_tcp` / `serve_state_sync_over_tcp` reuse the anonymous,
   forward-secret `mini-bearer::Channel` with a state-sync-specific AAD domain.
   The transport protects bytes; the QC/state checks provide authority.
9. Compatibility `history_since` is retained but uses a capped deque. A node
   with an archive recovers by the same verified state-sync path on restart.

## Evidence

- canonical economy/execution state round trips byte-identically;
- truncation, oversize, malformed ordering/tags, wrong supply, and tampering
  fail closed;
- a structurally matching QC without quorum is rejected by local finality
  verification;
- snapshot wrong-network, wrong-root, and state tampering fail;
- a bad second block causes zero partial application;
- the compatibility history drops its oldest row at the declared cap;
- archive snapshots/prunes/reopens, rejects corrupt/symlinked/oversized state,
  and replays an interrupted exact install journal idempotently;
- response construction serializes a large snapshot base once, then accounts
  each candidate block once, rather than cloning an 8 MiB state per candidate;
- a real TCP test transfers snapshot plus suffix between independent nodes and
  the destination/reopened archive reaches the exact source height and state
  commitment.

## Authority and constitutional boundary

- No checkpoint operator, hard-coded trusted peer, hosted snapshot authority,
  admin key, majority-by-download rule, forced update, or central availability
  service exists.
- Objects/QCs and deterministic state commitments remain truth. The archive is
  deletable local acceleration/recovery state.
- Balance, payment, storage, bandwidth, provider revenue, and archive size have
  no output into validator weight, governance, personhood, ranking, review
  quorum, or constitutional legitimacy.
- No new cryptography. Existing finality, KEL verification, content/state
  commitments, and encrypted channels are composed without weakening them.

## Honest limits

- Validator membership is static in this slice. No historical validator-set
  transition proof, weak-subjectivity checkpoint, or long-range key-compromise
  solution is claimed.
- Exact state is transparent and capped at 8 MiB; one response must fit a
  roughly 16 MiB encrypted bearer frame. Larger state needs a future chunked,
  authenticated Merkle/state-proof protocol, not a raised unbounded limit.
- The stable retained block suffix is count- and byte-bounded; callers repeat
  requests when the retained tip is more than one response away.
- Peer discovery, endpoint authentication, retry/multi-peer selection, eclipse
  resistance, and background serving are not implemented here.
- A local attacker able to erase/roll back the entire archive can deny or roll
  back local availability. QCs prevent invention of an unfinalized state, but
  no hardware monotonic counter or external checkpoint is introduced.
- Full-state decode and snapshot creation remain proportional to state size;
  physical weakest-device CPU, memory, flash-wear, and pause measurements are
  not yet recorded.
- This is prototype consensus/state code, not external audit or real-value
  activation.

## Required follow-up

Issue #45 retains: chunked authenticated state transfer, historical dynamic
validator-set proofs and explicit long-range policy, weak-device benchmarks,
peer selection/retry/eclipsing tests, pruning policy across upgrades, and client
integration. None may introduce a mandatory checkpoint authority.

## Merge condition

The PR remains unmergeable until generated navigation is current, all exact-head
format/Clippy/workspace/dependency/governance/reproducibility/Android workflows
are green, review findings are resolved against the final SHA, and the
applicable repository approval rule is satisfied. AI output carries zero
approval weight.
''',
)

status_replacement = '''**State-sync/catch-up now has two layers.** D-0093's
  bounded block-only `CatchupRequest`/`CatchupResponse` remains a compatibility
  path and still re-verifies every block through `apply_finalized_block`.
  **Implemented in this proposal (D-0207):** canonical complete
  `LedgerState` snapshots bind settlement-network id, exact monetary/payment
  state, finalized header, state commitment, and QC; receivers verify the QC
  against their own static validator set/KEL oracle before replacing state.
  `ConsensusArchive` adds a local non-authoritative cross-process-locked,
  journaled filesystem checkpoint plus count/byte-bounded suffix, atomic
  replacement, restart recovery, and pruning only behind a durable snapshot.
  `ConsensusNode` applies a whole snapshot/suffix on a cloned chain (a bad late
  block changes nothing), caps compatibility history, journals live commits,
  and can reopen from the same verified archive path. Real TCP tests prove a
  long-offline independent node reaches the source's exact height/commitment
  over the existing encrypted `Channel`. No peer or archive becomes a trust
  anchor. Honest limits: static validator set only; no historical set-transition
  or weak-subjectivity/long-range rule; exact transparent state capped at 8 MiB
  and one response at one bearer frame; no chunked Merkle state proofs,
  discovery/retry/multi-peer/eclipse policy, external audit, or physical
  weakest-device measurements. '''
replace_regex(
    "docs/STATUS.md",
    r'''\*\*State-sync/catch-up is shipped\*\*
  \(D-0093\):.*?First slice: history is
  unbounded in-memory \(no pruning/persistence\), and no peer-selection/retry
  policy\. ''',
    status_replacement,
)

replace_once(
    "docs/design/networked-consensus.md",
    "# Networked BFT consensus — `mini-consensus` (D-0200 through D-0205)",
    "# Networked BFT consensus — `mini-consensus` (D-0200 through D-0207)",
)
insert_section = '''
### Persistent authenticated state sync (D-0207)

D-0093's block-only catch-up is now joined by a complete-state recovery path.
`ConsensusSnapshot` carries exact canonical `LedgerState` bytes together with
the finalized header and QC. A receiver reconstructs a chain only after its own
static validator set/KEL oracle verifies the QC and the decoded state's
commitment equals the header root; the serving peer and local archive have zero
checkpoint authority.

`ConsensusArchive` stores a durable snapshot plus a count/byte-bounded contiguous
suffix under an OS file lock and replayable exact install journal. Live commits
and peer installs are persisted before the node swaps state; interrupted
replacement is replayed on reopen. `ConsensusNode::apply_state_sync` verifies an
entire blocks-or-snapshot response on a cloned chain, preventing partial apply.
The compatibility in-memory history is capped. A real encrypted-TCP test proves
a node that starts at genesis reaches a source archive's exact snapshot+suffix
tip and reopens to the same state commitment.

'''
replace_once(
    "docs/design/networked-consensus.md",
    "## Honest limits (the part that is not built)\n",
    insert_section + "## Honest limits (the part that is not built)\n",
)
replace_regex(
    "docs/design/networked-consensus.md",
    r'''- \*\*No state-sync for a node that missed a whole height\.\*\*.*?snapshot/catch-up protocol
  is separate, later work\.\n''',
    '''- **State sync is static-set and one-frame, not a universal checkpoint
  protocol.** D-0207 persists QC-bound exact snapshots and a bounded suffix,
  but verifies them against the caller's current static validator set. It does
  not prove historical validator-set transitions, solve long-range key
  compromise/weak subjectivity, or transfer states larger than the 8 MiB exact
  state / one bearer-frame response caps. Chunked Merkle state proofs and
  physical weak-device measurements remain later work.
''',
)
replace_regex(
    "docs/design/networked-consensus.md",
    r'''## Next slices \(in priority order\)

1\. \*\*State-sync / catch-up\*\*.*?5\. \*\*Dynamic validator sets\.\*\*\n''',
    '''## Next slices (in priority order)

1. **Chunked authenticated large-state sync and weakest-device evidence** —
   replace the one-frame ceiling with independently verifiable Merkle chunks,
   bounded resumability, and real phone CPU/memory/flash-pause measurements.
2. **Dynamic validator sets and historical checkpoint verification** — prove
   every set transition and define the explicit long-range/weak-subjectivity
   rule without installing a privileged checkpoint operator.
3. **Peer discovery, retry, and eclipse resistance** — route state-sync and
   live topology through `mini-net`, query independently chosen peers, and keep
   peer availability distinct from finality authority.
4. **Act on equivocation** — consume verified evidence through future role
   transitions; also collect proposal equivocation.
5. **Deployment hardening** — reconnect/background serving, pruning across
   upgrades, and multi-machine hostile-network drills.
''',
)
replace_once(
    "docs/design/networked-consensus.md",
    '''None of these change what "final" means (frozen in `mini-chain`); they add the
security, robustness, and operational machinery layered around it. View-change
''',
    '''None of these change what "final" means (frozen in `mini-chain`); they add the
security, robustness, and operational machinery layered around it. Persistent
QC-bound state sync and bounded pruning are **implemented in proposal D-0207**.
View-change
''',
)

# Append one band-correct decision. D-02xx is reserved for consensus/networking.
decision_path = "docs/DECISION_LOG.md"
decision_text = read(decision_path)
if "### D-0207 —" in decision_text:
    raise SystemExit("D-0207 already exists; refusing duplicate append")
decision_entry = '''

### D-0207 — QC-bound ledger snapshots, persistent catch-up, and bounded local pruning  ·  *Proposed*

**Date:** 2026-08-03 · **Refs:** D-0093, D-0200–D-0206, roadmap #45,
Directives 2/4/5/6/11/16, M1–M3.

**Decision:** extend block-only catch-up with a versioned exact execution-state
snapshot bound to one finalized block header and quorum certificate. A receiver
accepts a snapshot only after its own static `ValidatorSet` and KEL oracle verify
the QC, the settlement-network id matches, and the decoded state's canonical
commitment equals the header state root. Persist recovery state in an optional,
local, non-authoritative `ConsensusArchive`: replayable exact journal,
cross-process lock, regular-file/symlink refusal, synced atomic replacement,
count/byte-bounded finalized suffix, periodic snapshots, and pruning only after
a durable replacement checkpoint. Apply peer responses all-or-nothing on a
cloned chain and reuse the existing encrypted channel with a dedicated AAD
domain. No peer, archive, hosted service, or downloaded-majority count becomes a
checkpoint authority.

**Reason:** D-0093 correctly reused finalized blocks and local finality checks,
but its unbounded in-memory history vanished on restart and could not serve a
long-offline weak device. Local QC-bound snapshots close that operational gap
without outsourcing truth to a checkpoint operator.

**Constitutional impact:** strengthens deterministic ownership/finality
(Directives 4/5 and M1–M3), failure recovery (Directive 6), weak-device bounds
(Directive 11), self-hosting/no mandatory provider (Directive 2), and the
voice/value wall (Directive 16). Snapshot/archive size, service, balance, and
bandwidth confer no validator, governance, personhood, ranking, or review
weight. No frozen rule or cryptographic primitive is changed.

**Implementation status:** implemented and adversarially tested in proposed PR
#289: canonical monetary/execution snapshot codecs; locally verified
`ConsensusSnapshot`; bounded state-sync framing; journaled filesystem archive;
bounded compatibility history; all-or-nothing node adoption; encrypted real-TCP
snapshot+suffix convergence and restart proof.

**Failure point:** the construction assumes the caller supplies the correct
static validator set/KEL history. It does not verify historical dynamic-set
transitions or solve long-range key compromise/weak subjectivity. Exact state is
transparent, capped at 8 MiB, and one response must fit one roughly 16 MiB
bearer frame. There is no chunked Merkle state proof, peer discovery/retry/
multi-peer eclipse policy, hardware monotonic rollback anchor, physical
weak-device benchmark, external audit, or production/value activation.

**Required follow-up:** roadmap #45 retains chunked authenticated state transfer,
dynamic-set transition/checkpoint rules, long-range policy, weakest-device
benchmarks, peer selection/retry/eclipse tests, pruning across upgrades, and
client/background-serving integration. None may introduce a mandatory trusted
checkpoint service.

**Supersedes / superseded by:** builds on and does not supersede D-0093 or
D-0200–D-0206. It supersedes only those decisions' statements that catch-up
history is necessarily unbounded and memory-only.
'''
write(decision_path, decision_text.rstrip() + decision_entry + "\n")

# Remove one-shot machinery before navigation and exact checks.
SELF.unlink()
WORKFLOW.unlink()

run("cargo", "fmt", "--all")
run("rustup", "target", "add", "wasm32-wasip2")
run(
    "cargo",
    "clippy",
    "--workspace",
    "--all-targets",
    "--all-features",
    "--",
    "-D",
    "warnings",
)
run("cargo", "test", "--workspace", "--all-features")
run("python3", "-m", "unittest", "discover", "-s", "tools", "-p", "test_*.py")
run("python3", "tools/check_governance.py", "--mode", "baseline", "--candidate-activation")
run("python3", "tools/work_claims.py", "validate")
run("python3", "tools/mininet_nav.py", "build")

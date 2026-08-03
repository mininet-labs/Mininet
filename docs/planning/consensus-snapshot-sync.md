# Consensus authenticated snapshots, persistent catch-up, and bounded pruning (D-0207)

**Status:** implementation complete in draft PR #289; no merge, production,
release, or activation claim until exact-head CI and review complete.  
**Review remediation:** five inline findings are now bound to permanent regression
work: preflight-before-journal, validate-before-write archive plans, race-free
no-follow opens, a truthful suffix-limit error, and peer-facing gap/duplicate/
reordering tests. The PR stays draft until those changes pass on the integrated
exact head.  
**Runner state:** the integrated remediation commit is now prepared; the
repository-wide exact-head checks remain the engineering gate before review
threads can be resolved.  
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
   leaves the node at its original height. Verified live block rows are
   persisted before chain swap using atomic rows and snapshot-before-prune;
   peer snapshot/suffix replacement uses the replayable exact install journal.
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
  removes an orphaned interrupted block temp, refuses a missing/gapped or
  wrong-parent suffix before appending, and replays an interrupted exact install
  journal idempotently;
- exact-state and state-sync response encoders calculate aggregate bounds
  before allocating their final output buffers; response selection serializes a
  large snapshot base once rather than cloning an 8 MiB state per candidate;
- selected and accepted state-sync peers are subject to connect/read/write
  deadlines, so one silent peer can delay only until the local timeout;
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

# Consensus snapshots, persistent catch-up, and bounded pruning (D-0432)

**Status:** work claimed; implementation in progress. Do not duplicate this
slice while the associated draft PR is open.

**Issue:** #45 — a weak device or long-offline node must reach chain tip without
re-executing all history from genesis.

## Existing foundation

D-0093 already ships bounded finalized-block catch-up over the encrypted
`mini-bearer::Channel`. Every received block is applied through the same
`LedgerChain::apply_finalized_block` path used by live consensus, so a serving
peer is not a trust anchor. The remaining gap is structural: finalized history
is an unbounded in-memory `Vec`, disappears on restart, cannot be pruned safely,
and cannot bootstrap a node whose requested height is older than the serving
peer's retained suffix.

## Claimed deliverable

1. **Authenticated checkpoint snapshot.** A versioned snapshot binds a finalized
   `BlockHeader`, its `QuorumCertificate`, the exact serialized `LedgerState`,
   the resulting state commitment, and the settlement-network identifier. Import
   verifies the certificate against the caller-supplied validator set/KEL oracle,
   recomputes the state commitment, and rejects any mismatch before adoption.
2. **Persistent local consensus store.** A filesystem-backed store atomically
   persists the latest authenticated snapshot plus a bounded contiguous suffix of
   finalized blocks. Files are size-capped, versioned, checksummed for local
   corruption detection, symlink-safe, and recoverable after interrupted writes.
   The checksum is not authentication; QC and state-root verification remain the
   trust mechanism.
3. **Restart recovery.** `ConsensusNode` can be constructed from a verified local
   snapshot and replay its retained suffix through ordinary finalized-block
   validation, restoring height, tip hash, state, and catch-up service without
   trusting local bytes.
4. **Snapshot-aware catch-up.** A peer request receives either a contiguous block
   suffix or one authenticated snapshot followed by a bounded suffix. The
   requester verifies the snapshot and then every later block. A peer may supply
   bytes; it cannot choose canonical state.
5. **Safe pruning.** Retention is explicit and bounded. A snapshot is durably
   installed before history below it is removed. At least one verified checkpoint
   and a configured recent suffix remain available. Pruning never deletes the
   canonical live state and never changes finality.
6. **Encrypted transport composition.** Snapshot exchange reuses the existing
   `Channel` catch-up transport and receives strict frame, snapshot, state,
   certificate, block-count, and total-byte limits before allocation.
7. **Adversarial evidence.** Permanent tests cover tampered state, wrong state
   root, forged/insufficient QC, wrong network, gap/duplicate/out-of-order suffix,
   corrupted/truncated/oversized files, interrupted replacement, reopen/recovery,
   pruning boundaries, a server whose suffix no longer reaches genesis, and a
   real-TCP late node reaching the exact cluster state from snapshot plus suffix.

## Authority and constitutional boundaries

- No checkpoint operator, hosted snapshot server, admin key, hardcoded trusted
  peer, or majority-by-download rule is introduced.
- The validator set and KEL oracle supplied by the node remain the authority used
  by ordinary finality verification; a snapshot provider has zero authority.
- Balance, storage, bandwidth, snapshot volume, and service availability confer
  no vote, validator weight, governance standing, ranking, or personhood status.
- Snapshots are local/peer-served acceleration objects, not consensus truth and
  not a new mint, governance, or update path.
- No new cryptography. Existing hashes, object codecs, signatures, QCs, and
  encrypted channels are composed; D-0047 remains untouched.

## Explicit non-goals

- Dynamic validator-set epochs and long-range/weak-subjectivity checkpoint
  governance. The current validator set is static; snapshot verification is
  scoped honestly to that model.
- Peer discovery, multi-peer selection, reputation, automatic trust-on-first-use,
  background daemon operation, or a public snapshot CDN.
- Full historical/archive queries after pruning.
- Production genesis, issuance activation, real-money use, or mainnet claims.

## Failure conditions

The proposal fails if a node can adopt state without independently verifying a
QC and recomputed state commitment; if one server becomes required; if pruning
can remove the last recoverable verified checkpoint; if corrupted local files
silently become canonical; if an untrusted frame can force unbounded allocation;
or if snapshot service affects voice, identity, ranking, or ownership rules.

## Merge floor

The PR remains draft until implementation, exact-state documentation, generated
navigation, focused adversarial tests, full workspace CI, dependency checks,
governance checks, reproducibility, Android jobs, and exact-head review are all
complete. AI work carries zero approval weight.
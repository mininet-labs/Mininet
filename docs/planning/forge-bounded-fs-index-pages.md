# Forge Batch 5: bounded filesystem metadata index and stable pages (D-0430)

**Status:** implementation complete in proposed PR #287; no merge or release
claim until exact-head CI and human review complete.  
**Base:** `main` after merged D-0428 / PR #285.  
**Non-collision:** PR #286 owns the social/intake publication path. This work is
limited to `mini-store` ordered time-index queries, tests, and truth-sync.

## Problem

Batch 5 names local object indexing at scale as unfinished. Before this change,
`MemoryBackend` could answer `Store::recent(limit)` in `O(log n + limit)`, but
`FsBackend` recursively read and sorted every metadata row under `idx/time/`.
`Store::since(cursor)` also returned an unbounded suffix with no stable
continuation cursor. Long-lived devices therefore paid work proportional to all
history for a small forge/feed page.

Solving that with a hosted index would create a new mandatory authority. The
solution here stays local, reconstructible, non-authoritative, and removable.

## Implemented mechanism

1. `FsBackend` maintains a local `ordered/time-v1` side index under an OS-backed
   cross-process file lock.
2. Authoritative state remains the existing content-addressed object plus
   `meta/idx/time/<timestamp>/<object-id>` row. The side index is acceleration
   only; every emitted row is rechecked against authoritative metadata.
3. The side index contains one immutable sorted fixed-width base plus an append
   delta capped at 1,024 records. Queries binary-search the base and inspect at
   most the bounded delta. Compaction merges delta into a new base.
4. A checksummed manifest names the exact current base and counts base/delta
   records. A missing, truncated, malformed, or inconsistent index rebuilds
   deterministically from authoritative metadata.
5. A one-entry write-ahead journal covers the critical order: journal intent →
   persist authoritative time row → append side-index record → advance manifest
   → clear journal. Recovery handles interruption before metadata, after
   metadata, and after delta append but before manifest advancement.
6. `Backend::list_meta_prefix_page` adds ordered forward pages. `MemoryBackend`
   uses a bounded `BTreeMap` range; `FsBackend` uses the side index for the exact
   `idx/time/` prefix and preserves the safe full-scan fallback elsewhere.
7. `Store::since_page(start_ms, cursor, limit)` binds its cursor to both the
   20-digit timestamp and object ID, so equal timestamps cannot be skipped or
   duplicated. `Store::recent` and `since_page` reject pages over 1,024.
8. The legacy `Store::since` API remains for compatibility and is explicitly
   documented as unbounded. No object bytes, signatures, index-key format,
   synchronization, feed ordering, governance rule, or authority class changes.

## Evidence

- Memory and filesystem pages agree exactly with equal and out-of-order
  timestamps.
- Two-item pages cover every object exactly once and survive reopen.
- Independent filesystem writers converge under the OS lock.
- Deleting the side index triggers deterministic legacy rebuild.
- Missing manifested bases and partial delta tails rebuild safely.
- Journal recovery commits a durable delta append exactly once.
- Manual compaction preserves sorted unique rows.
- Hostile cursors, zero limits, excessive limits, and symlinked index paths fail
  safely.
- Marker, manifest, journal, and authoritative time-row values are size-checked
  before allocation; oversized local files rebuild or fail closed.
- Focused `mini-store` tests and strict Clippy pass before the permanent commit;
  full exact-head repository CI remains the merge gate.

## Hard boundaries and honest limits

- No SQLite, hosted search, mandatory daemon, network call, or trusted index
  operator.
- No new cryptography. The fixed-record checksum detects accidental/local index
  corruption; it is not authentication. Objects and metadata remain truth.
- Steady-state page work is bounded by page size, a 1,024-record delta, and
  logarithmic fixed-record base seeks. The index directory and every control-file
  or metadata-value allocation are capped; unknown entries fail closed.
- One-time migration/rebuild and periodic compaction are `O(total time-index
  rows)`, hold the local index lock, and are not claimed to be page-bounded.
- The compatibility `Store::since` full-suffix API is still unbounded.
- Only chronological `idx/time/` pages are accelerated. Author/type/link compound
  queries remain separate future work.
- Timestamps are author claims used for deterministic display ordering, never
  freshness, arrival, consensus, or trust evidence. `since_page` is stable for
  a fixed local index view, not a lossless live-sync frontier: a later-arriving
  object with an older claimed timestamp can sort before the current cursor.
  Lossless discovery remains the content/want-list synchronization layer.
- The side index is not signed, replicated, consensus state, or a source of
  governance/ranking authority. Deleting it must never delete objects.
- File contents are synced before rename, but this preserves the existing
  `FsBackend` durability model; parent-directory fsync and a transaction across
  every object index remain future hardening.
- Physical weakest-device latency and compaction-pause benchmarks are not yet
  recorded.

## Required follow-up

Adopt `since_page` in forge/feed clients that currently request unbounded history;
benchmark page and compaction latency on the weakest supported device; and add
bounded compound indexes only when a measured caller requires them. Do not move
this function to a central indexing service.

## Merge condition

The PR remains unmergeable until generated navigation is current, all exact-head
workflows are green, and the repository's required independent human approvals
review the final SHA. AI output carries zero approval weight.

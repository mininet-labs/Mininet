# Forge Batch 5: bounded filesystem metadata index and stable pages (D-0430)

**Status:** implementation complete in PR #287 and integrated with current
`main` after merged PRs #286 and #288; no merge or release claim until the new
exact-head workflows and human review complete.  
**Integrated base:** `5c93364e307013891bf934fafe6240b80c97b7de`.  
**Scope isolation:** the merged social/intake path remains outside this change.
This work is limited to `mini-store` ordered time-index queries, tests, and
truth-sync.

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
- Eight independently spawned test processes write one shared filesystem store
  concurrently and preserve every canonical time row under the OS lock.
- Deleting the side index triggers deterministic legacy rebuild.
- Missing manifested bases and partial delta tails rebuild safely.
- Journal recovery commits a durable delta append exactly once.
- Manual compaction preserves sorted unique rows, including a key present in
  both the immutable base and append delta.
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
- Completeness assumes local chronological metadata writes use this version's
  `FsBackend::put_meta` path. A concurrently running older binary, downgrade,
  or manual/out-of-band filesystem mutation can add an authoritative time row
  without updating the side index; the next bounded page cannot discover that
  omission without an unbounded scan. Stop mixed-version writers and run
  `FsBackend::rebuild_time_index()` after downgrade/out-of-band repair.
- New base/control-file contents are synced before rename and the containing
  directory is synced after rename on Unix. Non-Unix directory-durability
  semantics and a transaction across every object index remain future hardening.
- Physical weakest-device latency and compaction-pause benchmarks are not yet
  recorded.

## Required follow-up

Adopt `since_page` in forge/feed clients that currently request unbounded history;
benchmark page and compaction latency on the weakest supported device; and add
bounded compound indexes only when a measured caller requires them. Do not move
this function to a central indexing service.

## Merge condition

The PR remains unmergeable until generated navigation is current, all exact-head
workflows are green, and the applicable repository governance review requirement
is satisfied for the final SHA. AI output carries zero approval weight.

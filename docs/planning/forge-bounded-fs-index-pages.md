# Forge Batch 5: bounded filesystem metadata index and stable pages (D-0430)

**Status:** work claimed; implementation in progress. Do not duplicate this
slice while PR #287 is open.

**Base:** current `main` after merged D-0428 / PR #285.

**Non-collision:** PR #286 owns the social/intake publication path. This work is
limited to `mini-store` ordered metadata queries, their tests, and exact status /
decision-log truth-sync. It does not touch `mini-social`, `mini-intake*`,
`mini-desktop`, or the new `mini intake` commands.

## Problem

Batch 5 names local object indexing at scale as unfinished. The in-memory
backend can answer `Store::recent(limit)` in `O(log n + limit)`, but
`FsBackend` still inherits a fallback that recursively reads and sorts every
metadata row under `idx/time/`. `Store::since(cursor)` also has no bounded,
stable continuation cursor. A forge/feed client on a long-lived device therefore
pays work proportional to total history for an operation whose result is only a
small page.

Moving this cost into a centralized index service would violate the project
rather than solve it. The index must remain local, reconstructible from the
content-addressed store, non-authoritative, and safe to delete/rebuild.

## Intended implementation

1. Add a local `idx/time/` ordered side index for `FsBackend`, maintained under
   an OS-backed cross-process lock.
2. Keep the side index explicitly **non-authoritative**. Object and metadata
   files remain source-of-truth; the side index only accelerates enumeration.
3. Use a sorted immutable base plus a bounded append delta. Queries read only a
   bounded delta and seek fixed-width base records; periodic compaction absorbs
   the delta. A one-time legacy-store rebuild may scan existing `idx/time/`
   rows and is reported honestly as migration work, not a bounded query.
4. Journal an in-flight metadata insertion so a crash cannot leave a committed
   metadata row missing from the side index or a side-index row pretending a
   metadata row exists.
5. Override `FsBackend::list_meta_prefix_last("idx/time/", limit)` with the
   ordered index while retaining the existing safe fallback for other prefixes.
6. Add a forward `Backend::list_meta_prefix_page` primitive. `MemoryBackend`
   uses its `BTreeMap`; `FsBackend` uses the ordered time index for `idx/time/`
   and the documented fallback elsewhere.
7. Add `Store::since_page` with a stable `(timestamp_ms, object_id)` cursor so
   equal timestamps never create duplicates or omissions across pages.
8. Preserve the existing `since` and `recent` APIs. This is additive and does
   not change object bytes, signatures, index key format, sync, governance, or
   feed ordering.

## Hard boundaries

- No SQLite, remote database, hosted search service, or mandatory daemon.
- No new cryptography and no trust claim for the acceleration index.
- No silent loss of results after a crash; fail closed on malformed index files
  and provide a deterministic local rebuild path.
- No symlink/path-traversal regression in `FsBackend`.
- No balance, payment, reputation, or governance dependency.
- No claim that the one-time legacy rebuild or periodic compaction is bounded by
  page size. The promise is bounded steady-state query work, not free migration.

## Required adversarial evidence

- Memory and filesystem backends return identical ordering and page boundaries.
- Out-of-order timestamp insertion still produces canonical chronological pages.
- Multiple objects at one timestamp paginate without duplication or omission.
- Reopening the store preserves page results.
- A legacy store with no side index rebuilds deterministically.
- Simulated interruption before/after metadata persistence recovers one index
  entry, not zero or two semantic entries.
- Partial delta tails, malformed records, hostile cursors, symlinked index
  paths, zero limits, and page-limit abuse fail safely.
- Existing full workspace tests, governance checks, dependency checks,
  reproducibility, and Android jobs remain green.

## Merge condition

This document is not the deliverable. The PR remains draft until code, tests,
truth-sync, generated navigation, and exact-head CI all exist. Human review must
inspect the local-index format and crash protocol; AI output carries no approval
weight.
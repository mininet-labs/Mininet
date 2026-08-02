#!/usr/bin/env python3
"""Finalize PR #287's bounded local filesystem time index.

This one-shot script applies the remaining crash, concurrency, bounded-work,
test, and truth-sync changes. It removes itself and its temporary workflow
before running the focused checks, so the pushed commit contains only permanent
project files.
"""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TIME_INDEX = ROOT / "crates/mini-store/src/time_index.rs"
BACKEND = ROOT / "crates/mini-store/src/backend.rs"
STORE = ROOT / "crates/mini-store/src/store.rs"
TESTS = ROOT / "crates/mini-store/tests/time_pages.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SPINE = ROOT / "docs/design/self-hosted-forge-spine.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-finalize.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def regex_once(path: Path, pattern: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: expected one regex target, found {count}: {pattern[:100]!r}")
    path.write_text(updated, encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def harden_time_index() -> None:
    replace_once(
        TIME_INDEX,
        "const MAX_DELTA_RECORDS: u64 = 1_024;\n",
        "const MAX_DELTA_RECORDS: u64 = 1_024;\n"
        "const MAX_FORWARD_QUERY_ROWS: usize = crate::MAX_TIME_PAGE_SIZE + 1;\n"
        "const MAX_INDEX_DIRECTORY_ENTRIES: usize = 16;\n",
    )

    replace_once(
        TIME_INDEX,
        "            self.recover_pending(&key, manifest, actual_delta)?;\n",
        "            if let Err(error) = self.recover_pending(&key, manifest, actual_delta) {\n"
        "                match error {\n"
        "                    StoreError::Corrupt | StoreError::LimitExceeded => {\n"
        "                        self.rebuild()?;\n"
        "                        return Ok(());\n"
        "                    }\n"
        "                    other => return Err(other),\n"
        "                }\n"
        "            }\n",
    )

    replace_once(
        TIME_INDEX,
        "        let mut manifest = index.read_manifest()?;\n"
        "        if !index.contains_key(&manifest, key)? {\n"
        "            index.append_delta(&mut manifest, key)?;\n"
        "        }\n",
        "        let mut manifest = index.read_manifest()?;\n"
        "        let indexed = match index.contains_key(&manifest, key) {\n"
        "            Ok(indexed) => indexed,\n"
        "            Err(StoreError::Corrupt) | Err(StoreError::LimitExceeded) => {\n"
        "                // The metadata row is already authoritative. Rebuild the\n"
        "                // disposable acceleration index rather than leaving a\n"
        "                // permanent pending journal that wedges later writes.\n"
        "                index.rebuild()?;\n"
        "                manifest = index.read_manifest()?;\n"
        "                index.contains_key(&manifest, key)?\n"
        "            }\n"
        "            Err(error) => return Err(error),\n"
        "        };\n"
        "        if !indexed {\n"
        "            index.append_delta(&mut manifest, key)?;\n"
        "        }\n",
    )

    replace_once(
        TIME_INDEX,
        "pub(crate) fn last(root: &Path, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {\n"
        "    if limit == 0 {\n",
        "pub(crate) fn last(root: &Path, limit: usize) -> Result<Vec<(String, Vec<u8>)>> {\n"
        "    if limit > crate::MAX_TIME_PAGE_SIZE {\n"
        "        return Err(StoreError::LimitExceeded);\n"
        "    }\n"
        "    if limit == 0 {\n",
    )

    regex_once(
        TIME_INDEX,
        r"pub\(crate\) fn page\(\n    root: &Path,\n    after: &str,\n    limit: usize,\n\) -> Result<Vec<\(String, Vec<u8>\)>> \{\n    if limit == 0 \{",
        "pub(crate) fn page(\n"
        "    root: &Path,\n"
        "    after: &str,\n"
        "    limit: usize,\n"
        ") -> Result<Vec<(String, Vec<u8>)>> {\n"
        "    if limit > MAX_FORWARD_QUERY_ROWS {\n"
        "        return Err(StoreError::LimitExceeded);\n"
        "    }\n"
        "    if limit == 0 {",
    )

    regex_once(
        TIME_INDEX,
        r"fn ensure_existing_or_create_directory\(path: &Path, label: &str\) -> Result<\(\)> \{.*?\n\}\n\nfn reject_symlink_or_non_file_if_present",
        '''fn ensure_existing_or_create_directory(path: &Path, label: &str) -> Result<()> {
    let validate_existing = || -> Result<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(StoreError::Io(format!("{label} is a symlink")))
            }
            Ok(metadata) if !metadata.is_dir() => {
                Err(StoreError::Io(format!("{label} is not a directory")))
            }
            Ok(_) => Ok(()),
            Err(error) => Err(error.into()),
        }
    };

    match fs::symlink_metadata(path) {
        Ok(_) => validate_existing(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => Ok(()),
                // Independent processes may both observe the directory as
                // absent before one creates it. Revalidate the winner rather
                // than turning that safe race into a write failure.
                Err(create_error)
                    if create_error.kind() == std::io::ErrorKind::AlreadyExists =>
                {
                    validate_existing()
                }
                Err(create_error) => Err(create_error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn reject_symlink_or_non_file_if_present''',
    )

    regex_once(
        TIME_INDEX,
        r"    fn cleanup_orphan_bases\(&self, current_generation: u64\) -> Result<\(\)> \{.*?\n    \}\n\}\n\npub\(crate\) fn put_time_meta",
        '''    fn cleanup_orphan_bases(&self, current_generation: u64) -> Result<()> {
        let current_name = format!("base-{current_generation:020}.idx");
        let permanent = [
            MARKER_FILE,
            LOCK_FILE,
            PENDING_FILE,
            MANIFEST_FILE,
            DELTA_FILE,
        ];
        let mut entries = 0usize;
        let mut base_files = 0usize;
        for entry in fs::read_dir(&self.index_root)? {
            entries = entries
                .checked_add(1)
                .ok_or(StoreError::LimitExceeded)?;
            if entries > MAX_INDEX_DIRECTORY_ENTRIES {
                return Err(StoreError::LimitExceeded);
            }

            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(StoreError::Io(
                    "symlink in time-index directory".to_string(),
                ));
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if name.starts_with("base-") && name.ends_with(".tmp") {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index base temporary".to_string(),
                    ));
                }
                fs::remove_file(path)?;
                continue;
            }
            if name.starts_with("base-") && name.ends_with(".idx") {
                base_files += 1;
                if base_files > 8 {
                    return Err(StoreError::LimitExceeded);
                }
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index base".to_string(),
                    ));
                }
                if name != current_name {
                    fs::remove_file(path)?;
                }
                continue;
            }
            if name.ends_with(".tmp-write") {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file time-index atomic temporary".to_string(),
                    ));
                }
                fs::remove_file(path)?;
                continue;
            }
            if permanent.contains(&name.as_ref()) {
                if !file_type.is_file() {
                    return Err(StoreError::Io(
                        "non-file permanent time-index entry".to_string(),
                    ));
                }
                continue;
            }
            return Err(StoreError::Io(format!(
                "unknown entry in time-index directory: {name}"
            )));
        }
        Ok(())
    }
}

pub(crate) fn put_time_meta''',
    )

    text = TIME_INDEX.read_text(encoding="utf-8")
    extra_tests = r'''

    #[test]
    fn a_journaled_delta_append_is_committed_exactly_once() {
        use std::io::Write as _;

        let root = temp_root("journal-delta");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let key = valid_key(8, 9);

        with_locked(&root, |index| {
            index.write_pending(&key)?;
            let metadata_path = root.join("meta").join(&key);
            fs::create_dir_all(metadata_path.parent().unwrap())?;
            fs::write(&metadata_path, b"")?;

            // Simulate loss of power after the delta record is durable but
            // before the manifest count is advanced.
            let manifest = index.read_manifest()?;
            let delta_path = index.index_root.join(DELTA_FILE);
            let mut delta = OpenOptions::new().append(true).open(delta_path)?;
            delta.write_all(&encode_record(&key, manifest.delta_count)?)?;
            delta.sync_all()?;
            Ok(())
        })
        .unwrap();

        let first = last(&root, 10).unwrap();
        let second = last(&root, 10).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, key);
        let manifest = with_locked(&root, |index| index.read_manifest()).unwrap();
        assert_eq!(manifest.base_count + manifest.delta_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manual_compaction_preserves_sorted_unique_rows() {
        let root = temp_root("compact");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let keys = [valid_key(30, 11), valid_key(10, 13), valid_key(30, 15)];

        with_locked(&root, |index| {
            let mut manifest = index.read_manifest()?;
            for key in &keys {
                let path = root.join("meta").join(key);
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(path, b"")?;
                index.append_delta(&mut manifest, key)?;
            }
            index.compact(&manifest)?;
            let compacted = index.read_manifest()?;
            assert_eq!(compacted.delta_count, 0);
            assert_eq!(compacted.base_count, 3);
            let (rows, stale) = index.query_forward(
                &compacted,
                "idx/time/00000000000000000000/",
                10,
            )?;
            assert!(!stale);
            let mut expected = keys.to_vec();
            expected.sort();
            assert_eq!(
                rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>(),
                expected
            );
            Ok(())
        })
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }
'''
    end = text.rfind("\n}\n")
    if end < 0 or "a_journaled_delta_append_is_committed_exactly_once" in text:
        raise SystemExit("could not append time-index unit tests exactly once")
    TIME_INDEX.write_text(text[:end] + extra_tests + text[end:], encoding="utf-8")


def harden_public_api_and_docs() -> None:
    regex_once(
        BACKEND,
        r"    /// this when a backend can genuinely stop scanning once `limit` results\n    /// are found,.*?\n    fn list_meta_prefix_last",
        '''    /// this when a backend can genuinely stop scanning once `limit` results
    /// are found. [`MemoryBackend`] does this for every prefix;
    /// [`FsBackend`] does it for the exact `idx/time/` prefix through a local,
    /// reconstructible ordered side index. Other filesystem prefixes retain
    /// the semantically correct full-scan fallback.
    fn list_meta_prefix_last''',
    )

    regex_once(
        STORE,
        r"    /// Ids of objects with `timestamp_ms >= cursor_ms`.*?\n    pub fn since\(",
        '''    /// Ids of objects with `timestamp_ms >= cursor_ms`, oldest first.
    ///
    /// This compatibility API intentionally returns the whole suffix and may
    /// scan/allocate proportional to it. New interactive callers should use
    /// [`Self::since_page`], whose stable `(timestamp, object id)` cursor and
    /// page-size ceiling provide bounded steady-state work on both shipped
    /// backends. Timestamps remain author-claimed ordering hints, never proof
    /// of freshness or arrival order.
    pub fn since(''',
    )

    replace_once(
        STORE,
        "    pub fn recent(&self, limit: usize) -> Result<Vec<ObjectId>> {\n"
        "        let mut out = Vec::new();\n",
        "    pub fn recent(&self, limit: usize) -> Result<Vec<ObjectId>> {\n"
        "        if limit > MAX_TIME_PAGE_SIZE {\n"
        "            return Err(StoreError::LimitExceeded);\n"
        "        }\n"
        "        let mut out = Vec::new();\n",
    )


def harden_tests() -> None:
    replace_once(
        TESTS,
        "    assert_eq!(\n"
        "        store.since_page(0, None, MAX_TIME_PAGE_SIZE + 1),\n"
        "        Err(StoreError::LimitExceeded)\n"
        "    );\n"
        "}\n\n#[cfg(unix)]",
        "    assert_eq!(\n"
        "        store.since_page(0, None, MAX_TIME_PAGE_SIZE + 1),\n"
        "        Err(StoreError::LimitExceeded)\n"
        "    );\n"
        "    assert_eq!(\n"
        "        store.recent(MAX_TIME_PAGE_SIZE + 1),\n"
        "        Err(StoreError::LimitExceeded)\n"
        "    );\n"
        "}\n\n"
        "#[test]\n"
        "fn a_partial_delta_tail_rebuilds_from_authoritative_metadata() {\n"
        "    use std::io::Write as _;\n\n"
        "    let root = temp_root(\"partial-delta\");\n"
        "    let mut store = Store::new(FsBackend::open(&root).unwrap());\n"
        "    for object in objects().into_iter().take(4) {\n"
        "        store.insert(&object).unwrap();\n"
        "    }\n"
        "    let expected = collect_pages(&store, 0, 2);\n"
        "    drop(store);\n\n"
        "    let mut delta = std::fs::OpenOptions::new()\n"
        "        .append(true)\n"
        "        .open(root.join(\"ordered/time-v1/delta\"))\n"
        "        .unwrap();\n"
        "    delta.write_all(&[0xff]).unwrap();\n"
        "    delta.sync_all().unwrap();\n"
        "    drop(delta);\n\n"
        "    let reopened = Store::new(FsBackend::open(&root).unwrap());\n"
        "    assert_eq!(collect_pages(&reopened, 0, 2), expected);\n"
        "    let _ = std::fs::remove_dir_all(root);\n"
        "}\n\n"
        "#[test]\n"
        "fn independent_filesystem_writers_preserve_every_time_row() {\n"
        "    let root = temp_root(\"concurrent-writers\");\n"
        "    let objects = objects();\n"
        "    std::thread::scope(|scope| {\n"
        "        for object in objects.iter().cloned() {\n"
        "            let root = &root;\n"
        "            scope.spawn(move || {\n"
        "                let mut store = Store::new(FsBackend::open(root).unwrap());\n"
        "                store.insert(&object).unwrap();\n"
        "            });\n"
        "        }\n"
        "    });\n\n"
        "    let store = Store::new(FsBackend::open(&root).unwrap());\n"
        "    let actual = collect_pages(&store, 0, 2);\n"
        "    let mut expected: Vec<(u64, String)> = objects\n"
        "        .iter()\n"
        "        .map(|object| (object.timestamp_ms, object.id().as_str().to_string()))\n"
        "        .collect();\n"
        "    expected.sort();\n"
        "    assert_eq!(\n"
        "        actual,\n"
        "        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()\n"
        "    );\n"
        "    let _ = std::fs::remove_dir_all(root);\n"
        "}\n\n"
        "#[cfg(unix)]",
    )


def truth_sync() -> None:
    PLANNING.write_text(
        '''# Forge Batch 5: bounded filesystem metadata index and stable pages (D-0430)

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
- Focused `mini-store` tests and strict Clippy pass before the permanent commit;
  full exact-head repository CI remains the merge gate.

## Hard boundaries and honest limits

- No SQLite, hosted search, mandatory daemon, network call, or trusted index
  operator.
- No new cryptography. The fixed-record checksum detects accidental/local index
  corruption; it is not authentication. Objects and metadata remain truth.
- Steady-state page work is bounded by page size, a 1,024-record delta, and
  logarithmic fixed-record base seeks. The index directory itself is capped and
  unknown entries fail closed.
- One-time migration/rebuild and periodic compaction are `O(total time-index
  rows)`, hold the local index lock, and are not claimed to be page-bounded.
- The compatibility `Store::since` full-suffix API is still unbounded.
- Only chronological `idx/time/` pages are accelerated. Author/type/link compound
  queries remain separate future work.
- Timestamps are author claims used for deterministic display ordering, never
  freshness, arrival, consensus, or trust evidence.
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
''',
        encoding="utf-8",
    )

    decision = '''

### D-0430 — Local non-authoritative ordered time index and stable filesystem pages  ·  *Proposed*

**Date:** 2026-08-03 · **Refs:** D-0066 (self-hosted forge resequencing),
D-0327 (`idx/time` first slice), D-0331 (`MemoryBackend` bounded recent),
`docs/design/self-hosted-forge-spine.md` Batch 5,
`docs/planning/forge-bounded-fs-index-pages.md`.

**Decision:** complete Batch 5's chronological local-index slice without a
hosted index authority. `FsBackend` maintains a disposable local ordered side
index for authoritative `idx/time/<timestamp>/<object-id>` rows: one immutable
sorted fixed-width base, a 1,024-record append delta, a checksummed manifest, an
OS-backed cross-process lock, and a one-entry crash-recovery journal. Every
returned row is rechecked against authoritative metadata. `Store::since_page`
uses a stable `(timestamp_ms, object_id)` cursor; `Store::recent` and
`since_page` reject limits above 1,024. The old full-suffix `Store::since`
remains compatible and explicitly unbounded.

**Reason:** a long-lived local forge must page its own history without reading
every row and without outsourcing discovery to GitHub, a hosted search service,
or another operator. A reconstructible local acceleration index gives bounded
steady-state reads while preserving object/metadata files as the sole source of
truth.

**Constitutional impact:** strengthens Directives 2 and 9 by removing pressure
for a mandatory hosted index while bounding weak-device work; constrained by
Directive 14 because no new cryptographic trust claim is made, and Directive 16
because index position, volume, and recency create no governance weight.

**Implementation status:** implemented and tested in this proposal. Memory and
filesystem backends produce identical stable pages; equal timestamps,
out-of-order writes, reopen, concurrent writers, legacy rebuild, partial tails,
missing bases, journal interruption, compaction, page limits, and symlinked index
paths are covered. No external dependency or object-wire change is added.

**Failure point:** the side index is not authority and may be deleted/rebuilt.
A first query after migration or detected corruption and each compaction can scan
all chronological metadata while holding the local lock. `Store::since` remains
an unbounded compatibility API; only `idx/time` is accelerated; author timestamps
are not freshness evidence; parent-directory fsync, cross-index transactionality,
and physical weakest-device latency remain unmeasured.

**Required follow-up:** migrate interactive forge/feed callers to `since_page`,
benchmark page/rebuild/compaction behavior on the weakest supported device, and
add other bounded compound indexes only for measured callers. Never replace this
local reconstructible facility with a mandatory hosted index.

**Supersedes / superseded by:** fulfills D-0331's named `FsBackend` and forward-
pagination limitations. It does not supersede D-0327's index-key contract or any
object, sync, feed, or governance decision.
'''
    decisions = DECISIONS.read_text(encoding="utf-8")
    if "### D-0430 —" in decisions:
        raise SystemExit("D-0430 already exists")
    DECISIONS.write_text(decisions.rstrip() + decision + "\n", encoding="utf-8")

    status_replacement = '''- **shipped in this proposal (D-0430)** — Batch 5's chronological local
  object-index slice is now bounded on both shipped backends. The existing
  authoritative `idx/time/<20-digit-timestamp>/<object-id>` rows remain
  unchanged; `FsBackend` adds a local, disposable `ordered/time-v1` side index
  consisting of one sorted fixed-width base plus a strictly bounded 1,024-row
  append delta. An OS-backed cross-process lock, checksummed manifest, and
  one-entry journal recover interrupted writes; missing/truncated/inconsistent
  files rebuild from authoritative metadata. Every query result is rechecked
  against its metadata row, so the side index never becomes authority.
  `Backend::list_meta_prefix_page` and `Store::since_page` add stable forward
  pagination using a `(timestamp_ms, object_id)` cursor; equal timestamps do not
  duplicate or disappear. `Store::recent` and `since_page` reject limits above
  1,024. Memory/Fs ordering parity, out-of-order writes, reopen, concurrent
  writers, rebuild, partial tails, missing bases, journal recovery, compaction,
  cursor/limit abuse, and symlink refusal are tested. Honest limits: legacy
  rebuild and compaction are full-history maintenance under a local lock;
  compatibility `Store::since` remains unbounded; only `idx/time` is accelerated;
  timestamps are ordering hints, not freshness; physical weakest-device and
  parent-directory-fsync behavior remain follow-up. No remote index, daemon,
  dependency, object-wire, sync, ranking, payment, or governance change.
'''
    regex_once(
        STATUS,
        r'- \*\*partial\*\* — Batch 5\'s "local object indexing at scale," first slice.*?(?=\n- \*\*)',
        status_replacement.rstrip(),
    )

    spine_replacement = '''**Local object indexing — bounded chronological filesystem slice shipped
in this proposal (D-0430), building on D-0327/D-0331.**
`mini_store::Store::since`/`Store::recent` retain the existing authoritative
`idx/time/<timestamp>/<id>` rows. `MemoryBackend` already had bounded reverse
ranges; `FsBackend` now has a local non-authoritative ordered side index with a
sorted fixed-width base, 1,024-row append delta, manifest, cross-process lock,
and crash-recovery journal. `Store::since_page` adds stable forward pages keyed
by `(timestamp_ms, object_id)`, and both interactive APIs cap pages at 1,024.
Malformed or missing acceleration data rebuilds from authoritative metadata;
results are rechecked against those rows. Migration/rebuild and compaction are
full-history maintenance, the old full-suffix `Store::since` remains unbounded,
and author/type/link compound pagination is still open. No hosted index or
mandatory daemon is introduced.
'''
    regex_once(
        SPINE,
        r'\*\*Local object indexing — first slice shipped.*?(?=\n\*\*Distributed build workers)',
        spine_replacement.rstrip() + "\n",
    )


def main() -> None:
    harden_time_index()
    harden_public_api_and_docs()
    harden_tests()
    truth_sync()

    SELF.unlink()
    WORKFLOW.unlink()

    run("cargo", "fmt", "--all")
    run("cargo", "test", "-p", "mini-store", "--all-features")
    run(
        "cargo",
        "clippy",
        "-p",
        "mini-store",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    )
    run("python3", "-m", "unittest", "discover", "-s", "tools", "-p", "test_*.py")
    run("python3", "tools/mininet_nav.py", "build")
    run("python3", "tools/mininet_nav.py", "check")


if __name__ == "__main__":
    main()

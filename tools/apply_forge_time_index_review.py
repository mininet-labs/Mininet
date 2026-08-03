#!/usr/bin/env python3
"""Apply the exact-head review fixes for PR #287, test, and self-remove."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TIME_INDEX = ROOT / "crates/mini-store/src/time_index.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-review-fixes.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"{path}: expected exactly one replacement target, found {count}: {old[:160]!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_in_function(
    path: Path,
    function_name: str,
    old: str,
    new: str,
) -> None:
    text = path.read_text(encoding="utf-8")
    marker = f"    fn {function_name}"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"{path}: function not found: {function_name}")
    end = text.find("\n    fn ", start + len(marker))
    if end < 0:
        end = len(text)
    segment = text[start:end]
    count = segment.count(old)
    if count != 1:
        raise SystemExit(
            f"{path}:{function_name}: expected one target, found {count}: {old[:160]!r}"
        )
    segment = segment.replace(old, new, 1)
    path.write_text(text[:start] + segment + text[end:], encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


replace_once(
    TIME_INDEX,
    """        fs::rename(&self.temp_path, &self.final_path)?;
        Ok((self.final_path, self.count))
""",
    """        fs::rename(&self.temp_path, &self.final_path)?;
        sync_parent_directory(&self.final_path)?;
        Ok((self.final_path, self.count))
""",
)

replace_in_function(
    TIME_INDEX,
    "query_forward",
    """        let extra = delta.len();
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
""",
    """        let extra = delta.len();
        // `candidates` already contains `extra` delta rows. A total budget of
        // `limit + extra` therefore reads at most `limit` base rows; the extra
        // slots cover keys duplicated between base and delta before dedup.
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
""",
)
replace_in_function(
    TIME_INDEX,
    "query_forward",
    """        while index < base.count && candidates.len() < base_budget.saturating_add(extra) {
""",
    """        while index < base.count && candidates.len() < base_budget {
""",
)

replace_in_function(
    TIME_INDEX,
    "query_reverse",
    """        let extra = delta.len();
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
""",
    """        let extra = delta.len();
        // Same accounting as the forward scan: the delta rows already occupy
        // `extra` candidate slots, so only `limit` base rows are read.
        let base_budget = limit.checked_add(extra).ok_or(StoreError::LimitExceeded)?;
        let mut candidates = delta;
""",
)
replace_in_function(
    TIME_INDEX,
    "query_reverse",
    """        while remaining > 0 && candidates.len() < base_budget.saturating_add(extra) {
""",
    """        while remaining > 0 && candidates.len() < base_budget {
""",
)

replace_once(
    TIME_INDEX,
    """fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
""",
    """/// Make an already-completed rename durable in the containing directory
/// where the standard library exposes a directory file descriptor. The side
/// index remains reconstructible authority-free state, but returning success
/// should not knowingly leave Unix rename persistence to a later unrelated
/// write.
fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::Io("time-index path has no containing directory".to_string())
    })?;
    #[cfg(unix)]
    {
        File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        // `std` does not expose a portable directory-fsync operation on every
        // target. Those platforms retain the reconstructible-index fallback;
        // the limitation is recorded in D-0430 rather than hidden.
        let _ = parent;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
""",
)
replace_once(
    TIME_INDEX,
    """    fs::rename(temp, path)?;
    Ok(())
""",
    """    fs::rename(temp, path)?;
    sync_parent_directory(path)?;
    Ok(())
""",
)

compaction_anchor = """    #[test]
    fn an_oversized_pending_journal_is_rebuilt_without_unbounded_allocation() {
"""
compaction_test = """    #[test]
    fn compaction_collapses_a_key_present_in_both_base_and_delta() {
        let root = temp_root("compact-existing-duplicate");
        fs::create_dir_all(root.join("blobs")).unwrap();
        fs::create_dir_all(root.join("meta")).unwrap();
        let existing = valid_key(20, 21);
        let delta_only = valid_key(30, 23);

        let existing_path = root.join("meta").join(&existing);
        fs::create_dir_all(existing_path.parent().unwrap()).unwrap();
        fs::write(existing_path, b"").unwrap();
        assert_eq!(rebuild(&root).unwrap(), 1);

        with_locked(&root, |index| {
            let mut manifest = index.read_manifest()?;
            assert_eq!(manifest.base_count, 1);
            assert_eq!(manifest.delta_count, 0);

            // Exercise the defensive equality branch directly. Normal writes
            // call `contains_key` first, but recovery/corruption hardening must
            // still collapse a key that appears in both sorted inputs.
            index.append_delta(&mut manifest, &existing)?;
            let delta_path = root.join("meta").join(&delta_only);
            fs::create_dir_all(delta_path.parent().unwrap())?;
            fs::write(delta_path, b"")?;
            index.append_delta(&mut manifest, &delta_only)?;

            index.compact(&manifest)?;
            let compacted = index.read_manifest()?;
            assert_eq!(compacted.delta_count, 0);
            assert_eq!(compacted.base_count, 2);
            let (rows, stale) =
                index.query_forward(&compacted, "idx/time/00000000000000000000/", 10)?;
            assert!(!stale);
            assert_eq!(
                rows.into_iter().map(|(key, _)| key).collect::<Vec<_>>(),
                vec![existing.clone(), delta_only.clone()]
            );
            Ok(())
        })
        .unwrap();
        let _ = fs::remove_dir_all(root);
    }

""" + compaction_anchor
replace_once(TIME_INDEX, compaction_anchor, compaction_test)

replace_once(
    PLANNING,
    """- File contents are synced before rename, but this preserves the existing
  `FsBackend` durability model; parent-directory fsync and a transaction across
  every object index remain future hardening.
""",
    """- New base/control-file contents are synced before rename and the containing
  directory is synced after rename on Unix. Non-Unix directory-durability
  semantics and a transaction across every object index remain future hardening.
""",
)
replace_once(
    DECISIONS,
    """Parent-directory fsync, cross-index transactionality, and physical weakest-
device latency remain unmeasured.
""",
    """Containing-directory fsync after side-index renames is implemented on Unix;
cross-index transactionality, non-Unix directory-durability semantics, and
physical weakest-device latency remain unmeasured.
""",
)
replace_once(
    STATUS,
    """  physical weakest-device and parent-directory-fsync behavior remain follow-up.
""",
    """  physical weakest-device measurements, non-Unix directory-durability
  semantics, and cross-index transactions remain follow-up.
""",
)

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
run("python3", "tools/mininet_nav.py", "build")

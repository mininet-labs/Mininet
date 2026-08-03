#!/usr/bin/env python3
"""Integrate current main into PR #287, apply review fixes, test, self-remove."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TIME_INDEX = ROOT / "crates/mini-store/src/time_index.rs"
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/merge-pr287-after-286.yml"

D0429_MARKER = "### D-0429 — `mini-social`: canonical bounded"
D0430_MARKER = "### D-0430 — Local non-authoritative ordered time index"
D0431_MARKER = "### D-0431 — Correcting D-0407's status:"
STATUS_D0430_MARKER = "- **shipped in this proposal (D-0430)**"
STATUS_D0430_END = "- **shipped** — Git SHA-256 export bridge"


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        args,
        cwd=ROOT,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    if check and result.returncode != 0:
        raise SystemExit(
            f"command failed ({result.returncode}): {' '.join(args)}\n{result.stdout}"
        )
    return result


def replace_once_or_already(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    if new in text and old not in text:
        return
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"{path}: expected exactly one replacement target, found {count}: "
            f"{old[:160]!r}"
        )
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def insert_once(path: Path, anchor: str, insertion: str, sentinel: str) -> None:
    text = path.read_text(encoding="utf-8")
    if sentinel in text:
        return
    count = text.count(anchor)
    if count != 1:
        raise SystemExit(
            f"{path}: expected exactly one insertion anchor, found {count}: "
            f"{anchor[:160]!r}"
        )
    path.write_text(text.replace(anchor, insertion + anchor, 1), encoding="utf-8")


def extract_tail(path: Path, marker: str) -> str:
    text = path.read_text(encoding="utf-8")
    if text.count(marker) != 1:
        raise SystemExit(f"{path}: expected exactly one marker {marker!r}")
    return text[text.index(marker) :].rstrip() + "\n"


def extract_block(path: Path, start_marker: str, end_marker: str) -> str:
    text = path.read_text(encoding="utf-8")
    if text.count(start_marker) != 1 or text.count(end_marker) != 1:
        raise SystemExit(f"{path}: expected one start/end marker")
    start = text.index(start_marker)
    end = text.index(end_marker, start)
    return text[start:end].rstrip() + "\n"


def has_merge_marker(text: str) -> bool:
    """Recognize Git conflict-marker lines without flagging document rulers."""

    for line in text.splitlines():
        if line == "=======" or line.startswith("<<<<<<< ") or line.startswith(">>>>>>> "):
            return True
    return False


def apply_exact_head_review_fixes() -> None:
    """Apply every actionable finding from review 4840152615."""

    replace_once_or_already(
        TIME_INDEX,
        """        fs::rename(&self.temp_path, &self.final_path)?;
        Ok((self.final_path, self.count))
""",
        """        fs::rename(&self.temp_path, &self.final_path)?;
        sync_parent_directory(&self.final_path)?;
        Ok((self.final_path, self.count))
""",
    )

    replace_once_or_already(
        TIME_INDEX,
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
    replace_once_or_already(
        TIME_INDEX,
        """        while index < base.count && candidates.len() < base_budget.saturating_add(extra) {
""",
        """        while index < base.count && candidates.len() < base_budget {
""",
    )

    replace_once_or_already(
        TIME_INDEX,
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
    replace_once_or_already(
        TIME_INDEX,
        """        while remaining > 0 && candidates.len() < base_budget.saturating_add(extra) {
""",
        """        while remaining > 0 && candidates.len() < base_budget {
""",
    )

    replace_once_or_already(
        TIME_INDEX,
        """fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
""",
        """/// Sync a completed rename in its containing directory where the
/// platform exposes directory fsync. The ordered index is reconstructible, but
/// returning success should not knowingly leave supported rename persistence to
/// an unrelated later write.
fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        StoreError::Io("time-index path has no containing directory".to_string())
    })?;
    #[cfg(unix)]
    {
        let directory = File::open(parent)?;
        match directory.sync_all() {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput | std::io::ErrorKind::Unsupported
                ) =>
            {
                // Some Unix filesystems do not implement directory fsync. The
                // side index remains non-authoritative and rebuildable there.
            }
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
""",
    )
    replace_once_or_already(
        TIME_INDEX,
        """    fs::rename(temp, path)?;
    Ok(())
""",
        """    fs::rename(temp, path)?;
    sync_parent_directory(path)?;
    Ok(())
""",
    )

    anchor = """    #[test]
    fn an_oversized_pending_journal_is_rebuilt_without_unbounded_allocation() {
"""
    test = """    #[test]
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

            // Normal writes call `contains_key` first, but recovery and
            // corruption hardening must still collapse equality between the
            // independently sorted base and delta inputs.
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

"""
    insert_once(
        TIME_INDEX,
        anchor,
        test,
        "fn compaction_collapses_a_key_present_in_both_base_and_delta()",
    )

    replace_once_or_already(
        PLANNING,
        """- File contents are synced before rename, but this preserves the existing
  `FsBackend` durability model; parent-directory fsync and a transaction across
  every object index remain future hardening.
""",
        """- New base/control-file contents are synced before rename and the containing
  directory is synced after rename where the platform supports directory fsync.
  Unsupported directory-fsync semantics and a transaction across every object
  index remain future hardening.
""",
    )
    replace_once_or_already(
        DECISIONS,
        """Parent-directory fsync, cross-index transactionality, and physical weakest-
device latency remain unmeasured.
""",
        """Containing-directory fsync after side-index renames is attempted wherever the
platform supports it; cross-index transactionality, unsupported directory-fsync
semantics, and physical weakest-device latency remain unmeasured.
""",
    )
    replace_once_or_already(
        STATUS,
        """  physical weakest-device and parent-directory-fsync behavior remain follow-up.
""",
        """  physical weakest-device measurements, unsupported directory-fsync
  semantics, and cross-index transactions remain follow-up.
""",
    )


# Preserve the proposal-specific truth before current main's documentation is
# selected during conflict resolution.
d0430_decision = extract_tail(DECISIONS, D0430_MARKER)
d0430_status = extract_block(STATUS, STATUS_D0430_MARKER, STATUS_D0430_END)

run("git", "fetch", "origin", "main")
merge = run(
    "git",
    "merge",
    "--no-commit",
    "--no-ff",
    "origin/main",
    check=False,
)
if merge.returncode not in (0, 1):
    raise SystemExit(merge.stdout)

conflict_output = run("git", "diff", "--name-only", "--diff-filter=U").stdout.strip()
conflicts = {line for line in conflict_output.splitlines() if line}
allowed_conflicts = {
    "docs/DECISION_LOG.md",
    "docs/STATUS.md",
    "docs/_generated/REPO_INDEX.json",
    "docs/_generated/REPO_INDEX.jsonl",
    "docs/_generated/REPO_MAP.md",
}
unexpected = conflicts - allowed_conflicts
if unexpected:
    raise SystemExit(f"unexpected merge conflicts: {sorted(unexpected)}")

# Main owns already-merged decisions and generated files. Reapply D-0430 from
# this proposal, then regenerate navigation from the combined permanent tree.
for path in sorted(conflicts):
    run("git", "checkout", "--theirs", "--", path)
    run("git", "add", "--", path)

decision_text = DECISIONS.read_text(encoding="utf-8")
if decision_text.count(D0429_MARKER) != 1:
    raise SystemExit("merged decision log lost or duplicated D-0429")
if D0430_MARKER not in decision_text:
    if decision_text.count(D0431_MARKER) == 1:
        decision_text = decision_text.replace(
            D0431_MARKER,
            d0430_decision.rstrip() + "\n\n" + D0431_MARKER,
            1,
        )
    elif D0431_MARKER not in decision_text:
        decision_text = decision_text.rstrip() + "\n\n" + d0430_decision
    else:
        raise SystemExit("merged decision log contains duplicate D-0431 entries")
    DECISIONS.write_text(decision_text, encoding="utf-8")
elif decision_text.count(D0430_MARKER) != 1:
    raise SystemExit("combined decision log contains duplicate D-0430 entries")

status_text = STATUS.read_text(encoding="utf-8")
if STATUS_D0430_MARKER not in status_text:
    if status_text.count(STATUS_D0430_END) != 1:
        raise SystemExit("STATUS D-0430 insertion anchor is missing or ambiguous")
    status_text = status_text.replace(
        STATUS_D0430_END,
        d0430_status + STATUS_D0430_END,
        1,
    )
    STATUS.write_text(status_text, encoding="utf-8")
elif status_text.count(STATUS_D0430_MARKER) != 1:
    raise SystemExit("combined STATUS contains duplicate D-0430 blocks")
if "D-0429" not in STATUS.read_text(encoding="utf-8"):
    raise SystemExit("merged STATUS lost D-0429")

apply_exact_head_review_fixes()

# The permanent PR must not contain orchestration machinery.
SELF.unlink()
WORKFLOW.unlink()

# Regenerate and verify the exact post-merge, post-review-fix working tree.
run("cargo", "fmt", "--all")
run("cargo", "fmt", "--all", "--", "--check")
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
run("git", "diff", "--check")

for path in (DECISIONS, STATUS):
    text = path.read_text(encoding="utf-8")
    if has_merge_marker(text):
        raise SystemExit(f"merge marker survived in {path}")
if DECISIONS.read_text(encoding="utf-8").count(D0430_MARKER) != 1:
    raise SystemExit("D-0430 is not singular after integration")

#!/usr/bin/env python3
"""Run and pre-commit PR #289 review remediation for the legacy CI attempt.

The original remediation workflow merges `main` immediately after invoking this
path. This wrapper therefore commits the permanent source changes and removes
all temporary orchestration before returning, so Git has a clean worktree for
that merge. The final PR diff contains neither this wrapper nor the copied base
helper.
"""

from pathlib import Path
import runpy
import subprocess

base = Path("tools/apply_consensus_snapshot_review_fixes_base.py")
runpy.run_path(str(base), run_name="__main__")

path = Path("crates/mini-consensus/src/store.rs")
text = path.read_text(encoding="utf-8")
obsolete = """    fn write_snapshot_locked(&self, snapshot: &ConsensusSnapshot) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &snapshot.to_wire_bytes()?)
    }

"""
if text.count(obsolete) != 1:
    raise SystemExit("obsolete write_snapshot_locked method was not found exactly once")
path.write_text(text.replace(obsolete, "", 1), encoding="utf-8")

subprocess.run(
    [
        "git",
        "rm",
        "--ignore-unmatch",
        ".github/workflows/consensus-snapshot-finalizer.yml",
        ".github/workflows/consensus-snapshot-stage4.yml",
        ".github/workflows/consensus-snapshot-stage5.yml",
        "tools/apply_consensus_snapshot_stage4.py",
        "tools/apply_consensus_snapshot_review_fixes_base.py",
        "tools/apply_consensus_snapshot_review_fixes.py",
    ],
    check=True,
)
subprocess.run(["git", "add", "-A"], check=True)
subprocess.run(
    [
        "git",
        "commit",
        "-m",
        "fix: stage PR 289 review remediation before main integration",
    ],
    check=True,
)

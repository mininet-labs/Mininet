#!/usr/bin/env python3
"""Run and pre-commit PR #289 review remediation for the legacy CI attempt.

The original remediation workflow merges `main` immediately after invoking this
path and then performs one unconditional commit. This wrapper commits the source
changes and temporary-file removals first, then leaves one legitimate planning
truth-sync change for that workflow-owned commit.
"""

from pathlib import Path
import runpy
import subprocess

base = Path("tools/apply_consensus_snapshot_review_fixes_base.py")
runpy.run_path(str(base), run_name="__main__")

# The snapshot writer is a deliberate test-only crash-injection hook. Mark it as
# such so the normal library target has no dead-code warning while the archive
# interruption test can still construct the precise partial state it verifies.
store = Path("crates/mini-consensus/src/store.rs")
text = store.read_text(encoding="utf-8")
writer = """    fn write_snapshot_locked(&self, snapshot: &ConsensusSnapshot) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &snapshot.to_wire_bytes()?)
    }

"""
annotated_writer = """    #[cfg(test)]
    fn write_snapshot_locked(&self, snapshot: &ConsensusSnapshot) -> Result<()> {
        atomic_write(&self.root.join(SNAPSHOT_FILE), &snapshot.to_wire_bytes()?)
    }

"""
if text.count(writer) != 1:
    raise SystemExit("test-only write_snapshot_locked method was not found exactly once")
store.write_text(text.replace(writer, annotated_writer, 1), encoding="utf-8")

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

# Leave one permanent, review-relevant documentation change uncommitted. The
# legacy workflow stages and commits it before merging main, avoiding an empty
# unconditional commit while preserving an honest audit trail.
plan = Path("docs/planning/consensus-snapshot-sync.md")
text = plan.read_text(encoding="utf-8")
old = """**Review remediation:** five inline findings are now bound to permanent regression
work: preflight-before-journal, validate-before-write archive plans, race-free
no-follow opens, a truthful suffix-limit error, and peer-facing gap/duplicate/
reordering tests. The PR stays draft until those changes pass on the integrated
exact head.  
"""
new = old + """**Runner state:** the integrated remediation commit is now prepared; the
repository-wide exact-head checks remain the engineering gate before review
threads can be resolved.  
"""
if text.count(old) != 1:
    raise SystemExit("planning review-remediation paragraph was not found exactly once")
plan.write_text(text.replace(old, new, 1), encoding="utf-8")

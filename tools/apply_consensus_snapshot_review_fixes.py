#!/usr/bin/env python3
"""Run the PR #289 review remediation and remove one refactor remnant.

Temporary branch-local wrapper. The validating remediation commit deletes this
file and its copied base helper before pushing the permanent tree.
"""

from pathlib import Path
import runpy

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
base.unlink()

#!/usr/bin/env python3
"""Merge current main (including PR #286) into PR #287, verify, self-remove."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/merge-pr287-after-286.yml"

D0430_MARKER = "### D-0430 — Local non-authoritative ordered time index"
D0429_MARKER = "### D-0429 — `mini-social`: canonical bounded"
STATUS_D0430_MARKER = "- **shipped in this proposal (D-0430)**"
STATUS_D0430_END = "- **shipped** — Git SHA-256 export bridge"


def run(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args,
        cwd=ROOT,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


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


# Preserve the proposal-specific truth before main's D-0429 documentation is
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

conflict_output = run(
    "git", "diff", "--name-only", "--diff-filter=U"
).stdout.strip()
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

# Main owns D-0429 and the generated files. Reapply D-0430 from the proposal,
# then regenerate navigation from the combined permanent tree.
for path in sorted(conflicts):
    run("git", "checkout", "--theirs", "--", path)
    run("git", "add", "--", path)

# D-0430 is the final proposal entry after D-0429.
decision_text = DECISIONS.read_text(encoding="utf-8")
if D0429_MARKER not in decision_text:
    raise SystemExit("merged decision log lost D-0429")
if D0430_MARKER not in decision_text:
    DECISIONS.write_text(
        decision_text.rstrip() + "\n\n" + d0430_decision,
        encoding="utf-8",
    )
elif decision_text.count(D0430_MARKER) != 1:
    raise SystemExit("combined decision log contains duplicate D-0430 entries")

# STATUS edits are in distinct sections and may merge automatically. If conflict
# resolution selected main, reinsert the D-0430 forge block at its stable anchor.
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

# The permanent PR must not contain the orchestration machinery.
SELF.unlink()
WORKFLOW.unlink()

# Regenerate and verify against the exact post-merge working tree.
run("cargo", "fmt", "--all")
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

# No conflict marker may survive, and both decisions must remain singular.
for path in (DECISIONS, STATUS):
    text = path.read_text(encoding="utf-8")
    if "<<<<<<<" in text or ">>>>>>>" in text or "=======" in text:
        raise SystemExit(f"merge marker survived in {path}")
if DECISIONS.read_text(encoding="utf-8").count(D0430_MARKER) != 1:
    raise SystemExit("D-0430 is not singular after integration")

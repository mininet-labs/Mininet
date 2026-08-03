#!/usr/bin/env python3
"""Truth-sync the ordered-index mixed-version/out-of-band writer boundary."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLANNING = ROOT / "docs/planning/forge-bounded-fs-index-pages.md"
DECISIONS = ROOT / "docs/DECISION_LOG.md"
STATUS = ROOT / "docs/STATUS.md"
SPINE = ROOT / "docs/design/self-hosted-forge-spine.md"
SELF = Path(__file__)
WORKFLOW = ROOT / ".github/workflows/forge-time-index-mixed-writers.yml"


def replace_once(path: Path, old: str, new: str) -> None:
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one target, found {count}: {old[:120]!r}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def regex_once(path: Path, pattern: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    updated, count = re.subn(pattern, replacement, text, count=1, flags=re.DOTALL)
    if count != 1:
        raise SystemExit(f"{path}: expected one regex target, found {count}: {pattern[:120]!r}")
    path.write_text(updated, encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


replace_once(
    PLANNING,
    "- The side index is not signed, replicated, consensus state, or a source of\n"
    "  governance/ranking authority. Deleting it must never delete objects.\n",
    "- The side index is not signed, replicated, consensus state, or a source of\n"
    "  governance/ranking authority. Deleting it must never delete objects.\n"
    "- Completeness assumes local chronological metadata writes use this version's\n"
    "  `FsBackend::put_meta` path. A concurrently running older binary, downgrade,\n"
    "  or manual/out-of-band filesystem mutation can add an authoritative time row\n"
    "  without updating the side index; the next bounded page cannot discover that\n"
    "  omission without an unbounded scan. Stop mixed-version writers and run\n"
    "  `FsBackend::rebuild_time_index()` after downgrade/out-of-band repair.\n",
)

replace_once(
    DECISIONS,
    "frontier. Parent-directory fsync, cross-index transactionality, and physical\n"
    "weakest-device latency remain unmeasured.\n",
    "frontier. Completeness also assumes every local chronological metadata write\n"
    "uses this version's `FsBackend` path: a concurrently running older binary,\n"
    "downgrade, or manual filesystem mutation can bypass the side index and requires\n"
    "an explicit `rebuild_time_index()` before bounded pages are trusted complete.\n"
    "Parent-directory fsync, cross-index transactionality, and physical weakest-\n"
    "device latency remain unmeasured.\n",
)

replace_once(
    STATUS,
    "  physical weakest-device and parent-directory-fsync behavior remain follow-up.\n"
    "  No remote index, daemon,\n",
    "  physical weakest-device and parent-directory-fsync behavior remain follow-up.\n"
    "  Completeness assumes all local time-row writers use this version's `FsBackend`;\n"
    "  an older concurrent binary, downgrade, or manual filesystem mutation can bypass\n"
    "  the side index and requires `rebuild_time_index()`. No remote index, daemon,\n",
)

regex_once(
    SPINE,
    r"Pages are stable over a\nfixed view but are not a lossless sync frontier: later/backdated arrivals can\nsort before an issued author-timestamp cursor\. No hosted index or mandatory\ndaemon is introduced\.",
    "Pages are stable over a\nfixed view but are not a lossless sync frontier: later/backdated arrivals can\nsort before an issued author-timestamp cursor. Completeness also assumes local\ntime-row writers use this version's `FsBackend`; mixed-version/out-of-band\nwrites require an explicit side-index rebuild. No hosted index or mandatory\ndaemon is introduced.",
)

SELF.unlink()
WORKFLOW.unlink()
run("python3", "tools/mininet_nav.py", "build")

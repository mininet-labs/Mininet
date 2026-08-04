#!/usr/bin/env python3
"""Reconcile PR #297's accepted storage-fraud truth into PR #296 after merging main."""

from pathlib import Path
import subprocess


def show_main(path: str) -> str:
    return subprocess.check_output(
        ["git", "show", f"origin/main:{path}"], text=True, encoding="utf-8"
    )


def extract_section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        raise SystemExit(f"missing section heading in canonical main: {heading!r}")
    next_heading = text.find("\n### ", start + len(heading))
    end = len(text) if next_heading < 0 else next_heading + 1
    return text[start:end].rstrip() + "\n\n"


def extract_block(text: str, start_marker: str, end_marker: str) -> str:
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"missing block start in canonical main: {start_marker!r}")
    end = text.find(end_marker, start)
    if end < 0:
        raise SystemExit(f"missing block end in canonical main: {end_marker!r}")
    return text[start:end].rstrip() + "\n"


# D-0437 belongs to merged PR #297; D-0438 belongs to this PR. Preserve both,
# with accepted D-0437 immediately before proposed D-0438.
decision_path = Path("docs/DECISION_LOG.md")
decision = decision_path.read_text(encoding="utf-8")
if "### D-0437 — Cross-identity storage-fraud collision evidence" not in decision:
    canonical_decision = show_main("docs/DECISION_LOG.md")
    d0437 = extract_section(
        canonical_decision,
        "### D-0437 — Cross-identity storage-fraud collision evidence (`mini-storage-fraud`)  ·  *Accepted*",
    )
    insertion = "### D-0438 — Authenticated transport runtime convergence and peer-bound F6 provenance"
    index = decision.find(insertion)
    if index < 0:
        raise SystemExit("missing D-0438 insertion point")
    decision = decision[:index] + d0437 + decision[index:]
    decision_path.write_text(decision, encoding="utf-8")

# Add PR #297's exact shipped-status block to Storage without replacing this
# PR's networking/privacy additions.
status_path = Path("docs/STATUS.md")
status = status_path.read_text(encoding="utf-8")
if "- **shipped (D-0437, roadmap #42)** — `mini-storage-fraud`" not in status:
    canonical_status = show_main("docs/STATUS.md")
    storage_fraud = extract_block(
        canonical_status,
        "- **shipped (D-0437, roadmap #42)** — `mini-storage-fraud`",
        "\n\n## 8. Networking",
    )
    insertion = "\n## 8. Networking"
    index = status.find(insertion)
    if index < 0:
        raise SystemExit("missing Networking insertion point")
    status = status[:index].rstrip() + "\n" + storage_fraud + status[index:]
    status_path.write_text(status, encoding="utf-8")

print("PR #297 storage-fraud truth reconciled into PR #296")

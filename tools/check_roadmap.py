#!/usr/bin/env python3
"""Keep the release roadmap consistent with itself and with the README.

A roadmap nobody validates drifts, and a drifted roadmap is worse than none —
it reports progress that did not happen. This checks the mechanical parts:

- every item has a status from the allowed set;
- every `done` item cites a decision that actually exists in the decision log,
  so "done" can never be a claim without a record behind it;
- every `blocked` item names items that exist;
- the README's summary counts match the detail document.

What it deliberately does NOT check is whether a row is *honest* — whether
`ready` really is unblocked, or whether `done` really finished the work. No
tool can, and pretending otherwise would recreate the "green check that means
nothing" problem D-0441 exists to prevent. That stays a review question.

Standard library only, like every other validator in this tree.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROADMAP_PATH = Path("docs/ROADMAP_TO_RELEASE.md")
README_PATH = Path("README.md")
DECISION_LOG_PATH = Path("docs/DECISION_LOG.md")

STATUSES = {"done", "active", "ready", "blocked", "outside"}

ITEM = re.compile(r"^### (R\d+) — (.+?) · `([a-z]+)`\s*$", re.M)
SUMMARY_BLOCK = re.compile(
    r"<!-- ROADMAP-SUMMARY-BEGIN -->(.*?)<!-- ROADMAP-SUMMARY-END -->",
    re.S,
)
DECISION_REF = re.compile(r"\bD-\d{4}\b")
BLOCKED_BY = re.compile(r"^\*\*Blocked by:\*\*\s*(.+?)\s*$", re.M)
TOTALS = re.compile(
    r"\*\*(\d+) items: (\d+) active, (\d+) ready, (\d+) blocked, (\d+) outside\.\*\*"
)


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def item_bodies(text: str) -> list[tuple[str, str, str, str]]:
    """Return (id, title, status, body) for each roadmap item, in order."""
    matches = list(ITEM.finditer(text))
    out = []
    for index, match in enumerate(matches):
        start = match.end()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        out.append((match.group(1), match.group(2), match.group(3), text[start:end]))
    return out


def check(root: Path, errors: list[str], warnings: list[str]) -> None:
    roadmap_path = root / ROADMAP_PATH
    if not roadmap_path.is_file():
        fail(errors, f"missing {ROADMAP_PATH.as_posix()}")
        return
    text = roadmap_path.read_text(encoding="utf-8")
    items = item_bodies(text)
    if not items:
        fail(errors, "roadmap contains no items — expected '### Rn — title · `status`'")
        return

    decision_text = ""
    decision_log = root / DECISION_LOG_PATH
    if decision_log.is_file():
        decision_text = decision_log.read_text(encoding="utf-8")

    seen: dict[str, str] = {}
    counts = {status: 0 for status in STATUSES}

    for item_id, title, status, body in items:
        if item_id in seen:
            fail(errors, f"{item_id} is defined twice — one id must name one item")
        seen[item_id] = title

        if status not in STATUSES:
            fail(
                errors,
                f"{item_id} has status '{status}', expected one of "
                f"{', '.join(sorted(STATUSES))}",
            )
            continue
        counts[status] += 1

        if status == "done":
            # "done" without a citation is the one claim this file must never
            # be able to make. A decision number that does not exist in the
            # log is the same thing wearing a reference.
            referenced = DECISION_REF.findall(body)
            if not referenced:
                fail(
                    errors,
                    f"{item_id} is marked done but cites no D-number — "
                    "done must point at the decision that closed it",
                )
            for decision in referenced:
                if f"### {decision}" not in decision_text:
                    fail(
                        errors,
                        f"{item_id} is marked done citing {decision}, which has "
                        "no entry in docs/DECISION_LOG.md",
                    )

        if status == "blocked":
            blockers = BLOCKED_BY.search(body)
            if not blockers:
                fail(
                    errors,
                    f"{item_id} is blocked but names no blocker — "
                    "add '**Blocked by:** Rn'",
                )

    # Blockers must reference real items, checked after every id is known.
    for item_id, _title, status, body in items:
        if status != "blocked":
            continue
        blockers = BLOCKED_BY.search(body)
        if not blockers:
            continue
        named = re.findall(r"\bR\d+\b", blockers.group(1))
        if not named:
            fail(errors, f"{item_id}'s 'Blocked by' names no roadmap item")
        for blocker in named:
            if blocker not in seen:
                fail(errors, f"{item_id} is blocked by {blocker}, which does not exist")
            if blocker == item_id:
                fail(errors, f"{item_id} is blocked by itself")

    readme_path = root / README_PATH
    if not readme_path.is_file():
        fail(errors, "missing README.md")
        return
    readme = readme_path.read_text(encoding="utf-8")
    block = SUMMARY_BLOCK.search(readme)
    if block is None:
        fail(
            errors,
            "README.md has no ROADMAP-SUMMARY block — the roadmap must be "
            "visible on the front page, not only in docs/",
        )
        return

    totals = TOTALS.search(block.group(1))
    if totals is None:
        fail(
            errors,
            "README roadmap summary has no totals line — expected "
            "'**N items: A active, B ready, C blocked, D outside.**'",
        )
        return

    declared_total, active, ready, blocked, outside = (int(g) for g in totals.groups())
    expected = {
        "total": len(items),
        "active": counts["active"],
        "ready": counts["ready"],
        "blocked": counts["blocked"],
        "outside": counts["outside"],
    }
    actual = {
        "total": declared_total,
        "active": active,
        "ready": ready,
        "blocked": blocked,
        "outside": outside,
    }
    for key, value in expected.items():
        if actual[key] != value:
            fail(
                errors,
                f"README says {actual[key]} {key}, roadmap has {value} — "
                "update the README summary in the same change",
            )

    if counts["done"]:
        # Not an error: done items legitimately leave the five summary
        # categories. Worth saying out loud so the front page can be updated
        # to celebrate them rather than silently dropping them.
        warnings.append(
            f"{counts['done']} roadmap item(s) are done and are not counted in "
            "the README summary line; consider surfacing completed work there"
        )

    for item_id in sorted(seen, key=lambda value: int(value[1:])):
        if item_id not in readme and item_id not in block.group(1):
            warnings.append(f"{item_id} is not mentioned in the README summary")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    args = parser.parse_args()
    root = Path(args.root).resolve()

    errors: list[str] = []
    warnings: list[str] = []
    check(root, errors, warnings)

    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    if errors:
        return 1
    print(f"roadmap ok: {len(item_bodies((root / ROADMAP_PATH).read_text('utf-8')))} items")
    return 0


if __name__ == "__main__":
    sys.exit(main())

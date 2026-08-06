#!/usr/bin/env python3
"""Structural checks on the decision registry.

Every rule here was written after a specific incident, and the incident is
named in the rule's own docstring. Nothing is checked because it seemed like
good practice; each one cost real rework at least once.

The decision log is append-only history, and the work-claims registry is how
parallel tracks avoid colliding on the next number. Both are enforced socially
today: someone has to notice. These are the parts a machine can notice first.

Grants no authority. Reports structural facts about files; it decides nothing
about whether the work those files describe is correct or permitted.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

DECISION_LOG = Path("docs/DECISION_LOG.md")
WORK_CLAIMS = Path("governance/work-claims.json")

# `### D-0123 — Title  ·  *Status*`
HEADING = re.compile(r"^### (D-\d{4})\b(.*)$", re.MULTILINE)

# Statuses that mean a claim is still holding its ground.
OPEN_STATUSES = {"active", "in_review", "in_progress", "proposed"}


def decisions_in(text: str) -> dict[str, str]:
    """Map every decision id in a log to the full body of its entry."""
    entries: dict[str, str] = {}
    matches = list(HEADING.finditer(text))
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        entries[match.group(1)] = text[match.start() : end].rstrip()
    return entries


def heading_order(text: str) -> list[str]:
    return [match.group(1) for match in HEADING.finditer(text)]


def read_canonical(root: Path, canonical: str, path: Path) -> str | None:
    """Read `path` from the canonical baseline.

    `canonical` is either a directory holding a separately checked-out baseline
    tree, or a git ref. The directory form is preferred and is what CI uses:
    `actions/checkout` is shallow by default, so a ref like the PR's base SHA is
    frequently not in the local object store and `git show` would fail for a
    reason that has nothing to do with the decision log.
    """
    as_dir = Path(canonical)
    if as_dir.is_dir():
        candidate = as_dir / path
        if candidate.is_file():
            return candidate.read_text(encoding="utf-8")
        return None
    try:
        return subprocess.check_output(
            ["git", "-C", str(root), "show", f"{canonical}:{path.as_posix()}"],
            text=True,
            encoding="utf-8",
            stderr=subprocess.DEVNULL,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def check_no_duplicate_headings(text: str, errors: list[str]) -> None:
    """One number, one decision.

    Two entries under one id make the log ambiguous about which one a later
    "supersedes D-xxxx" refers to, and make every downstream citation
    ambiguous with it. The tree carries a pre-existing duplicate at D-0372;
    this stops the next one, and that one is reported as a warning by
    `--report-existing` rather than failing every unrelated build.
    """
    order = heading_order(text)
    seen: set[str] = set()
    for identifier in order:
        if identifier in seen:
            errors.append(
                f"{DECISION_LOG}: duplicate heading for {identifier} — "
                "one decision number must name exactly one entry"
            )
        seen.add(identifier)


def check_append_only(
    root: Path, canonical: str, text: str, errors: list[str], warnings: list[str]
) -> None:
    """A merged decision may be superseded, never deleted.

    PR #296 would have removed D-0437 from the log outright: the branch
    predated that entry, and merging it silently dropped a decision that was
    already permanent history. Nothing failed — the merge was clean, the tests
    passed, and the entry was simply gone. A reviewer had to notice.

    Deletion is an error because it is unambiguous: there is no legitimate
    reason to remove a merged entry, and the loss is silent and permanent.

    In-place modification only warns, and the distinction is deliberate. The
    log's rule is to supersede rather than edit, but the tree does carry
    legitimate truth-sync edits — "complete in draft PR #292" becoming "merged
    through PR #292" is a fact catching up with reality, not a rewritten
    decision. Failing on those would make the check something people learn to
    skip, and a check that is routinely bypassed protects nothing. So the diff
    is surfaced for a human to glance at, and only the unambiguous failure
    blocks.
    """
    canonical_text = read_canonical(root, canonical, DECISION_LOG)
    if canonical_text is None:
        errors.append(
            f"cannot read {DECISION_LOG} at {canonical!r}; append-only check did not run"
        )
        return

    before = decisions_in(canonical_text)
    after = decisions_in(text)

    for identifier in sorted(set(before) - set(after)):
        errors.append(
            f"{DECISION_LOG}: {identifier} exists at {canonical} but not here — "
            "a merged decision may be superseded by a new entry, never deleted"
        )

    modified = [i for i in sorted(set(before) & set(after)) if before[i] != after[i]]
    for identifier in modified:
        warnings.append(
            f"{DECISION_LOG}: {identifier}'s entry differs from {canonical} — "
            "confirm this is a factual status/truth sync and not a rewritten decision"
        )


def open_claims(registry: dict) -> list[dict]:
    return [
        claim
        for claim in registry.get("claims", [])
        if str(claim.get("status", "")).lower() in OPEN_STATUSES
    ]


def check_claimed_numbers(
    root: Path, canonical: str | None, text: str, registry: dict, errors: list[str]
) -> None:
    """A claimed decision number must be free, and claimed by one branch only.

    Three branches raced on D-0438/D-0439 in a single afternoon. Each checked
    the open PRs before claiming, each was right at the moment it looked, and
    each collided anyway — the numbers are claimed at PR-open time with no
    reservation, so "unclaimed when I checked" and "unclaimed when I merge" are
    different statements. Every collision surfaced at review, after the work
    was done, and cost a renumber across the log, STATUS, the audit documents
    and the registry.

    Two rules catch it at PR-open instead:

    1. A number claimed by an open claim must not already exist in the
       canonical baseline (someone else merged it first).
    2. Two open claims must not name the same number.
    """
    claims = open_claims(registry)

    by_number: dict[str, list[dict]] = {}
    for claim in claims:
        for identifier in claim.get("decision_ids") or []:
            by_number.setdefault(str(identifier), []).append(claim)

    for identifier, holders in sorted(by_number.items()):
        if len(holders) > 1:
            branches = ", ".join(sorted(str(c.get("branch")) for c in holders))
            errors.append(
                f"{WORK_CLAIMS}: {identifier} is claimed by more than one open claim ({branches}) — "
                "the second to merge must renumber, so settle it now rather than at review"
            )

    if canonical is None:
        return

    canonical_text = read_canonical(root, canonical, DECISION_LOG)
    if canonical_text is None:
        errors.append(
            f"cannot read {DECISION_LOG} at {canonical!r}; claim-collision check did not run"
        )
        return

    merged = set(decisions_in(canonical_text))
    here = decisions_in(text)
    for identifier, holders in sorted(by_number.items()):
        if identifier not in merged:
            continue
        # Already merged upstream. That is only legitimate if this worktree is
        # the branch that merged it -- i.e. the entry is byte-identical here.
        canonical_entry = decisions_in(canonical_text)[identifier]
        if here.get(identifier) == canonical_entry:
            continue
        branches = ", ".join(sorted(str(c.get("branch")) for c in holders))
        errors.append(
            f"{WORK_CLAIMS}: {identifier} is claimed as open by {branches} but already exists "
            f"at {canonical}. Either that claim's work is merged and the claim should be "
            f"closed, or another branch took the number first and this one must renumber — "
            f"both cost a review round if left until then"
        )


def check_claims_have_entries(text: str, registry: dict, warnings: list[str]) -> None:
    """An open claim reserving a number the log never defines is a dangling
    reservation: it blocks the number for everyone and documents nothing."""
    defined = set(decisions_in(text))
    for claim in open_claims(registry):
        for identifier in claim.get("decision_ids") or []:
            if str(identifier) not in defined:
                warnings.append(
                    f"{WORK_CLAIMS}: open claim on branch {claim.get('branch')!r} reserves "
                    f"{identifier}, which {DECISION_LOG} does not define yet"
                )


def run(root: Path, canonical: str | None, report_existing: bool) -> int:
    errors: list[str] = []
    warnings: list[str] = []

    log_path = root / DECISION_LOG
    if not log_path.is_file():
        print(f"error: {DECISION_LOG} not found under {root}", file=sys.stderr)
        return 2
    text = log_path.read_text(encoding="utf-8")

    claims_path = root / WORK_CLAIMS
    registry = json.loads(claims_path.read_text(encoding="utf-8")) if claims_path.is_file() else {}

    if canonical is None:
        # Without a baseline, only same-file rules are meaningful. Duplicate
        # detection still runs, but pre-existing duplicates are not this
        # change's fault, so they are warnings unless explicitly asked for.
        duplicate_errors: list[str] = []
        check_no_duplicate_headings(text, duplicate_errors)
        (errors if report_existing else warnings).extend(duplicate_errors)
    else:
        canonical_text = read_canonical(root, canonical, DECISION_LOG)
        pre_existing: set[str] = set()
        if canonical_text is not None:
            order = heading_order(canonical_text)
            pre_existing = {name for name in order if order.count(name) > 1}
        duplicate_errors = []
        check_no_duplicate_headings(text, duplicate_errors)
        for message in duplicate_errors:
            inherited = any(name in message for name in pre_existing)
            (warnings if inherited and not report_existing else errors).append(
                message + (" (pre-existing upstream)" if inherited else "")
            )
        check_append_only(root, canonical, text, errors, warnings)

    check_claimed_numbers(root, canonical, text, registry, errors)
    check_claims_have_entries(text, registry, warnings)

    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    if not errors:
        total = len(decisions_in(text))
        print(f"decision registry ok: {total} entries, no collisions, no history loss")
    return 1 if errors else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="worktree to check")
    parser.add_argument(
        "--canonical",
        help="git ref to compare against for append-only and collision checks "
        "— either a directory holding a checked-out baseline tree (preferred; CI's "
        "checkout is shallow) or a git ref. Without it, only same-file rules run.",
    )
    parser.add_argument(
        "--report-existing",
        action="store_true",
        help="treat pre-existing upstream problems as errors too, for cleanup passes",
    )
    args = parser.parse_args()
    return run(Path(args.root), args.canonical, args.report_existing)


if __name__ == "__main__":
    raise SystemExit(main())

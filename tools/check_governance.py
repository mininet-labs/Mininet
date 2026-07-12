#!/usr/bin/env python3
"""Reference Mininet bootstrap governance validator.

Standard-library only by design. It validates repository policy artifacts and,
when provided, a proposal body and changed-path list. It does not infer human
identity, reviewer competence, or constitutional legitimacy.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import sys
from pathlib import Path

REQUIRED_HEADINGS = [
    "Change class", "Exact state", "Summary", "Founder directives",
    "Invariants", "Decision log", "Evidence and tests", "AI assistance",
    "Security and privacy", "Dependencies", "Release and adoption",
    "Compensation", "Reviewer attestations",
]
PROTECTED_PREFIXES = (
    "docs/FOUNDER_DIRECTIVES.md", "docs/INVARIANTS.md", "docs/DECISION_LOG.md",
    "crates/mini-crypto/", "crates/mini-value/", "crates/mini-treasury/",
    "crates/mini-identity/", "crates/mini-consensus/", "crates/mini-chain/",
    "crates/mini-forge/", "crates/mini-provenance/", "crates/mini-update/",
    "crates/mini-installer/", ".github/workflows/", "deny.toml",
)
TIER_F_PREFIXES = (
    "docs/FOUNDER_DIRECTIVES.md", "docs/INVARIANTS.md",
)
PROHIBITED_CLAIMS = {
    "forced update": re.compile(r"\b(force(?:d)? update|mandatory activation)\b", re.I),
    "permanent owner/admin": re.compile(r"\b(permanent (?:owner|admin)|admin key|owner key)\b", re.I),
    "money buys governance": re.compile(r"\b(balance|stake|payment|wealth).{0,40}\b(vote|quorum|governance weight)\b", re.I | re.S),
}


def fail(errors: list[str], message: str) -> None:
    errors.append(message)


def read_optional(path: str | None) -> str:
    if not path:
        return ""
    return Path(path).read_text(encoding="utf-8")


def validate_baseline(root: Path, errors: list[str], warnings: list[str]) -> None:
    required = [
        root / "governance/policy.yml",
        root / "governance/exceptions.yml",
        root / "governance/document-summary.schema.json",
        root / ".github/pull_request_template.md",
    ]
    for path in required:
        if not path.is_file():
            fail(errors, f"missing required governance artifact: {path.relative_to(root)}")

    schema = root / "governance/document-summary.schema.json"
    if schema.is_file():
        try:
            json.loads(schema.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            fail(errors, f"invalid JSON schema: {exc}")

    exceptions = root / "governance/exceptions.yml"
    if exceptions.is_file():
        text = exceptions.read_text(encoding="utf-8")
        for match in re.finditer(r"expires:\s*(\d{4}-\d{2}-\d{2})", text):
            expiry = dt.date.fromisoformat(match.group(1))
            if expiry < dt.date.today():
                fail(errors, f"expired governance exception: {expiry}")
        if "exceptions:" not in text:
            fail(errors, "exceptions.yml lacks an exceptions list")

    codeowners = root / ".github/CODEOWNERS"
    template = root / ".github/CODEOWNERS.template"
    if not codeowners.exists() and not template.exists():
        warnings.append("no CODEOWNERS or CODEOWNERS.template found")


def validate_proposal(body: str, changed: list[str], errors: list[str], warnings: list[str]) -> None:
    if not body.strip():
        fail(errors, "proposal body is empty or unavailable")
        return
    for heading in REQUIRED_HEADINGS:
        if not re.search(rf"^##+\s+{re.escape(heading)}\s*$", body, re.I | re.M):
            fail(errors, f"missing proposal heading: {heading}")

    if "REPLACE_WITH_FINAL_DIGEST" in body:
        fail(errors, "exact state still contains the template placeholder")

    protected = any(any(path == p or path.startswith(p) for p in PROTECTED_PREFIXES) for path in changed)
    tier_f = any(any(path == p or path.startswith(p) for p in TIER_F_PREFIXES) for path in changed)
    if protected and not re.search(r"protocol-critical|cryptography-sensitive|constitutional|Tier-F", body, re.I):
        fail(errors, "protected paths changed without a sensitive change classification")
    if tier_f:
        if not re.search(r"\bINV-[A-Z0-9-]+\b", body):
            fail(errors, "Tier-F path changed without an invariant identifier")
        if not re.search(r"\bD-\d{4}\b", body):
            fail(errors, "Tier-F path changed without a decision identifier")

    for name, pattern in PROHIBITED_CLAIMS.items():
        if pattern.search(body) and not re.search(r"reject|forbid|must not|no path|does not", body, re.I):
            warnings.append(f"proposal may contain prohibited constitutional claim: {name}")

    if re.search(r"AI assisted", body, re.I) and not re.search(r"persistent.*identity|human|pseudonymous", body, re.I | re.S):
        fail(errors, "AI-assisted proposal lacks a persistent submitting/authorizing identity declaration")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--mode", choices=("baseline", "proposal", "strict"), default="baseline")
    parser.add_argument("--proposal-body")
    parser.add_argument("--changed-paths", help="newline-delimited changed-path file")
    args = parser.parse_args()

    root = Path(args.root).resolve()
    errors: list[str] = []
    warnings: list[str] = []
    validate_baseline(root, errors, warnings)

    if args.mode in {"proposal", "strict"}:
        body = read_optional(args.proposal_body) or os.getenv("PR_BODY", "")
        changed = read_optional(args.changed_paths).splitlines() if args.changed_paths else []
        validate_proposal(body, [p.strip() for p in changed if p.strip()], errors, warnings)

    for message in warnings:
        print(f"warning: {message}")
    for message in errors:
        print(f"error: {message}", file=sys.stderr)
    if args.mode == "strict" and warnings:
        return 1
    return 1 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""
constitution_registry.py - generate docs/CONSTITUTION_REGISTRY.json from
docs/FOUNDER_DIRECTIVES.md.

Founder review P0 item `constitution-registry` (D-0090): the review found
three different principle counts in play across this project's history
(SPEC-00's six, an external "v2" whitepaper/README's eleven, and this
repo's own committed seventeen Founder Directives) with no single
versioned, machine-readable identity. D-0090 settles that: the committed
Founder Directives are the one canonical set going forward. D-0352 later
added an eighteenth (the Edge Provider Doctrine, FD-18) via the same
process. This script gives each a stable ID and an exact digest of its
own canonical text, generated (not hand-maintained) so the registry can
never silently drift out of sync with the prose it mirrors.

Usage:
  python3 tools/constitution_registry.py build   # (re)write the registry
  python3 tools/constitution_registry.py check   # fail if registry is stale
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

SOURCE_PATH = Path("docs/FOUNDER_DIRECTIVES.md")
REGISTRY_PATH = Path("docs/CONSTITUTION_REGISTRY.json")

DIRECTIVE_HEADING_RE = re.compile(
    r"^## Directive (\d+) — (.+)$", re.M
)

# One-line, faithful distillations of each directive's own text below —
# not new claims, just short enough to scan in a table. The digest field
# is what actually binds a registry entry to the canonical prose; this
# statement is a human-readable label, not the source of truth.
STATEMENTS: dict[int, str] = {
    1: "Whenever technology and human freedom conflict, freedom wins; "
       "whenever profit conflicts with the Constitution, the Constitution "
       "wins.",
    2: "Every authority, dependency, server, and maintainer must be "
       "assumed temporary, compromisable, or mortal; removing any one of "
       "them must never destroy Mininet.",
    3: "The protocol must keep functioning without anyone's permission, "
       "even if every current contributor disappeared tomorrow.",
    4: "The ledger must always answer \"who owns what\" with certainty; "
       "two honest nodes reconciling to different answers means the "
       "design is wrong.",
    5: "Everything else may merge or disagree, but there is exactly one "
       "canonical settlement history, and ownership changes only through "
       "canonical consensus, never through offline-invented alternative "
       "truth.",
    6: "Assume every kind of outage, disaster, and compromise will "
       "eventually happen; the protocol must degrade gracefully and "
       "never panic.",
    7: "Anyone may fork the code, but a fork inherits no humanity, "
       "trust, or continuity — legitimacy is defined by continuous "
       "adherence to the Constitution and the verified-human community, "
       "not by a repository or trademark.",
    8: "Consensus, governance, economics, and recovery all trace back to "
       "verified humans, never governments, corporations, validators, "
       "token holders, or founders; anything drifting toward "
       "institutional trust must be removed.",
    9: "Privacy must come from mathematics, never from promises, "
       "policies, or ethics — a system that depends on trust to preserve "
       "privacy is already broken.",
    10: "Every shortcut trades something away (memory, security, "
        "decentralization, auditability, predictability, human "
        "equality); if the trade can't be clearly explained, reject it.",
    11: "Engineer for the old phone in a village, not the billionaire's "
        "server — a network that serves only powerful hardware serves "
        "only powerful people.",
    12: "AI may propose and discover; only humans decide and legitimize. "
        "No model, regardless of capability, may become a source of "
        "authority.",
    13: "Optimize for one hundred years, not the next quarter — choose "
        "the solution that survives longer whenever uncertain.",
    14: "Every line of code, protocol rule, and dependency is a "
        "liability; the strongest protocol is usually the one that "
        "removed the most unnecessary parts.",
    15: "Welcome those who come to build without trusting those who "
        "come to exploit; security should come from incentives, "
        "cryptography, and transparency, not suspicion of everyone.",
    16: "Money may buy storage, computation, bandwidth, and attention, "
        "but never governance — directly, indirectly, or accidentally; "
        "any proposal creating even a subtle path from wealth to "
        "political power must be rejected.",
    17: "Before every major decision, ask whether it makes a child born "
        "a century from now, who chose none of this and never knew the "
        "founders, more or less free — if less, the decision is wrong.",
    18: "The edge (banks, carriers, couriers, states, vendors, courts) may "
        "be convenient but the core may never depend on it — every "
        "provider must be replaceable and switchable by one human alone, "
        "and the core must survive the total disappearance of all of "
        "them.",
}

SUPERSEDED_SOURCES = [
    {
        "name": "SPEC-00",
        "principle_count": 6,
        "held": "external, not committed to this repository",
        "status": "superseded",
    },
    {
        "name": "v2 whitepaper/README",
        "principle_count": 11,
        "held": "external, not committed to this repository",
        "status": "superseded",
    },
]


def extract_directives(text: str) -> list[dict]:
    headings = list(DIRECTIVE_HEADING_RE.finditer(text))
    if len(headings) != 18:
        raise SystemExit(
            f"expected exactly 18 '## Directive N — Title' headings in "
            f"{SOURCE_PATH}, found {len(headings)}"
        )
    directives = []
    for i, match in enumerate(headings):
        number = int(match.group(1))
        title = match.group(2).strip()
        if number != i + 1:
            raise SystemExit(
                f"directive headings out of order: expected {i + 1}, "
                f"found {number}"
            )
        block_start = match.start()
        block_end = text.find("\n---", match.end())
        if block_end == -1:
            block_end = len(text)
        block_text = text[block_start:block_end].strip()
        digest = hashlib.sha256(block_text.encode("utf-8")).hexdigest()
        if number not in STATEMENTS:
            raise SystemExit(f"no STATEMENTS entry for Directive {number}")
        directives.append(
            {
                "id": f"FD-{number:02d}",
                "number": number,
                "title": title,
                "heading": match.group(0),
                "digest": f"sha256:{digest}",
                "statement": STATEMENTS[number],
            }
        )
    return directives


def build_registry(root: Path) -> dict:
    source = root / SOURCE_PATH
    text = source.read_text(encoding="utf-8")
    directives = extract_directives(text)
    return {
        "schema_version": 1,
        "canonical_source": str(SOURCE_PATH),
        "decision": "D-0090",
        "principle_count": len(directives),
        "superseded_sources": SUPERSEDED_SOURCES,
        "directives": directives,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["build", "check"])
    parser.add_argument("--root", default=".")
    args = parser.parse_args()

    root = Path(args.root)
    registry = build_registry(root)
    rendered = json.dumps(registry, indent=2, ensure_ascii=False) + "\n"

    target = root / REGISTRY_PATH
    if args.command == "build":
        target.write_text(rendered, encoding="utf-8")
        print(f"wrote {REGISTRY_PATH}")
        return 0

    # check
    if not target.is_file():
        print(f"error: {REGISTRY_PATH} does not exist; run `build`", file=sys.stderr)
        return 1
    current = target.read_text(encoding="utf-8")
    if current != rendered:
        print(
            f"error: {REGISTRY_PATH} is stale relative to {SOURCE_PATH}; "
            f"run `python3 tools/constitution_registry.py build`",
            file=sys.stderr,
        )
        return 1
    print(f"{REGISTRY_PATH} is up to date")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

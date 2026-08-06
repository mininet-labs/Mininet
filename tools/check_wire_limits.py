#!/usr/bin/env python3
"""Shared wire limits must be shared, not restated.

`did-mini` owns how many keys an identity may hold and how many signatures it
may produce. Seven crates independently wrote `const MAX_SIGNATURES: usize =
16` while `did-mini` permitted 64 — `mini-attest`, `mini-bridge`,
`mini-objects` (three modules), `mini-private-index`, `mini-relay`.

The failure is quiet and asymmetric. A threshold identity above the cap could
sign an object, verify it in memory, encode it, and then fail to decode its
own bytes. Nothing warns at compile time, no test covers it unless someone
thinks to build a 17-key identity, and the symptom at runtime reads as
corruption rather than as two crates disagreeing.

So the rule is mechanical: a decoder may reference `did_mini`'s constant, or
name a number at least as large, but it may not quietly restate a smaller one.

Grants no authority. It reads source text and reports mismatches.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

# `const MAX_SIGNATURES: usize = 16;` / `= did_mini::MAX_SIGNATURES;`
CONST = re.compile(
    r"const\s+(?P<name>MAX_SIGNATURES|MAX_KEYS|MAX_SIGNATURE_BYTES|MAX_DID_BYTES)"
    r"\s*:\s*usize\s*=\s*(?P<value>[^;]+);"
)

# The floors did-mini itself enforces. Kept here rather than parsed so this
# check still reports something useful if did-mini's own file is malformed.
FLOORS = {
    "MAX_SIGNATURES": 64,
    "MAX_KEYS": 32,
    "MAX_SIGNATURE_BYTES": 4096,
    "MAX_DID_BYTES": 256,
}

OWNER = Path("crates/did-mini")


def check(root: Path) -> int:
    errors: list[str] = []
    checked = 0

    for path in sorted((root / "crates").rglob("*.rs")):
        if OWNER in path.parents or "/target/" in path.as_posix():
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for match in CONST.finditer(text):
            name = match.group("name")
            value = match.group("value").strip()
            checked += 1
            if "did_mini::" in value:
                continue
            try:
                numeric = int(value.replace("_", ""), 0)
            except ValueError:
                # An expression rather than a literal; a human should look, but
                # it is not evidence of the restated-smaller-number bug.
                continue
            floor = FLOORS[name]
            if numeric < floor:
                relative = path.relative_to(root).as_posix()
                errors.append(
                    f"{relative}: {name} = {numeric}, below did-mini's {floor}. "
                    f"An identity did-mini accepts could sign an object this decoder "
                    f"cannot read. Use `did_mini::{name}`."
                )

    for error in errors:
        print(f"error: {error}")
    if not errors:
        print(f"wire limits ok: {checked} declaration(s), none below did-mini's floor")
    return 1 if errors else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".")
    args = parser.parse_args()
    root = Path(args.root)
    if not (root / "crates").is_dir():
        print(f"error: no crates/ directory under {root}", file=sys.stderr)
        return 2
    return check(root)


if __name__ == "__main__":
    raise SystemExit(main())

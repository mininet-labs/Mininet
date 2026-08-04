#!/usr/bin/env python3
"""Apply the one Clippy correction to the generated F6 validation code."""
from pathlib import Path

path = Path("crates/mini-search-federation-net/src/query.rs")
text = path.read_text(encoding="utf-8")
old = "if &result.ranking_profile != &requested_profile.id {"
new = "if result.ranking_profile != requested_profile.id {"
if text.count(old) != 1:
    raise SystemExit(f"expected one profile-comparison match, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("PR 296 final review Clippy correction applied")

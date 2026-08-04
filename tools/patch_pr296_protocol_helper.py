#!/usr/bin/env python3
"""Correct the DECISION_LOG line-wrap expected by the final protocol helper."""
from pathlib import Path

path = Path("tools/apply_pr296_final_protocol_boundaries.py")
text = path.read_text(encoding="utf-8")
old = '"""search-provider provenance; and wrong-purpose rejection. Focused\n"""'
new = '"""search-provider provenance; and wrong-purpose\nrejection. Focused\n"""'
if text.count(old) != 1:
    raise SystemExit(f"expected one stale decision-log matcher, found {text.count(old)}")
path.write_text(text.replace(old, new), encoding="utf-8")
print("PR 296 final protocol helper matcher corrected")

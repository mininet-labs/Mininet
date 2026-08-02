#!/usr/bin/env python3
"""Factor the time-index query row type before strict Clippy."""

from pathlib import Path


path = Path(__file__).resolve().parents[1] / "crates/mini-store/src/time_index.rs"
text = path.read_text(encoding="utf-8")
anchor = 'const PENDING_MAGIC: &[u8; 8] = b"MNTPND01";\n'
replacement = '''const PENDING_MAGIC: &[u8; 8] = b"MNTPND01";\n\ntype MetadataRow = (String, Vec<u8>);\ntype QueryRows = (Vec<MetadataRow>, bool);\n'''
if text.count(anchor) != 1:
    raise SystemExit("expected one time-index type-alias anchor")
text = text.replace(anchor, replacement, 1)
old = 'Result<(Vec<(String, Vec<u8>)>, bool)>'
if text.count(old) != 3:
    raise SystemExit(f"expected three complex query result types, found {text.count(old)}")
text = text.replace(old, 'Result<QueryRows>')
path.write_text(text, encoding="utf-8")
Path(__file__).unlink()

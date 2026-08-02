#!/usr/bin/env python3
"""Correct the finalizer's page-function match after rustfmt condensed it."""

from pathlib import Path


path = Path(__file__).with_name("finalize_forge_time_index.py")
text = path.read_text(encoding="utf-8")
old = r'''r"pub\(crate\) fn page\(\n    root: &Path,\n    after: &str,\n    limit: usize,\n\) -> Result<Vec<\(String, Vec<u8>\)>> \{\n    if limit == 0 \{"'''
new = r'''r"pub\(crate\) fn page\(root: &Path, after: &str, limit: usize\) -> Result<Vec<\(String, Vec<u8>\)>> \{\n    if limit == 0 \{"'''
if text.count(old) != 1:
    raise SystemExit(f"expected one formatted page matcher, found {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
Path(__file__).unlink()

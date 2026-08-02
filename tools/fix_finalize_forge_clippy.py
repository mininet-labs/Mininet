#!/usr/bin/env python3
"""Correct the finalizer's concurrent-writer fixture before strict Clippy."""

from pathlib import Path


path = Path(__file__).with_name("finalize_forge_time_index.py")
text = path.read_text(encoding="utf-8")
replacements = {
    '"        for object in objects.iter().cloned() {\\n"':
        '"        for object in objects.iter() {\\n"',
    '"                store.insert(&object).unwrap();\\n"':
        '"                store.insert(object).unwrap();\\n"',
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"expected one finalizer fixture target, found {text.count(old)}: {old}")
    text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")
Path(__file__).unlink()

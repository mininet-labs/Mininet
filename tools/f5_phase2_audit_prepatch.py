#!/usr/bin/env python3
"""Correct the queued audit hardener and existing numeric truth text."""

from pathlib import Path


root = Path(__file__).resolve().parents[1]
path = Path(__file__).with_name("f5_phase2_audit_harden.py")
text = path.read_text(encoding="utf-8")
old = '''    replace_count(
        MODEL,
        'model_commitment("delivery-evidence", base)',
        'model_commitment(EVIDENCE_DOMAIN, base)',
        2,
    )'''
new = '''    replace_once(
        MODEL,
        'evidence_commitment = model_commitment("delivery-evidence", base)',
        'evidence_commitment = model_commitment(EVIDENCE_DOMAIN, base)',
    )
    replace_once(
        MODEL,
        '''return self.evidence_commitment == model_commitment(
            "delivery-evidence",
            base,
        )''',
        '''return self.evidence_commitment == model_commitment(
            EVIDENCE_DOMAIN,
            base,
        )''',
    )'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one evidence transform, found {count}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")

model_doc = root / "docs" / "design" / "f5-phase2-settlement-model.md"
doc = model_doc.read_text(encoding="utf-8")
for old_value, new_value in (("`1,521 bytes`", "`1,523 bytes`"), ("`848` operations", "`849` operations")):
    count = doc.count(old_value)
    if count != 1:
        raise SystemExit(f"expected one numeric truth target {old_value}, found {count}")
    doc = doc.replace(old_value, new_value, 1)
model_doc.write_text(doc, encoding="utf-8")

Path(__file__).unlink()

#!/usr/bin/env python3
"""Correct the one-shot audit hardener's evidence-domain transform."""

from pathlib import Path


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
Path(__file__).unlink()

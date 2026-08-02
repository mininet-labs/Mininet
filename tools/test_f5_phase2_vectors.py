from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


TOOLS_ROOT = Path(__file__).resolve().parent
MODULE_PATH = TOOLS_ROOT / "f5_phase2_model.py"
VECTOR_PATH = TOOLS_ROOT / "fixtures" / "f5_phase2_report.jsonl"
SPEC = importlib.util.spec_from_file_location("f5_phase2_model_vectors", MODULE_PATH)
assert SPEC and SPEC.loader
MODEL = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODEL
SPEC.loader.exec_module(MODEL)


class FixedVectorTests(unittest.TestCase):
    def test_checked_in_report_matches_exact_model_output(self) -> None:
        expected = VECTOR_PATH.read_text(encoding="utf-8")
        actual = MODEL.render_report()
        self.assertEqual(
            actual,
            expected,
            "F5 Phase-2 output changed: review the semantic change before "
            "regenerating the fixed vector",
        )

    def test_model_check_mode_accepts_checked_in_vector(self) -> None:
        self.assertEqual(MODEL.main(["--check", str(VECTOR_PATH)]), 0)


if __name__ == "__main__":
    unittest.main()

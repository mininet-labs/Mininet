from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("f5_phase2_model.py")
SPEC = importlib.util.spec_from_file_location("f5_phase2_model_emit", MODULE_PATH)
assert SPEC and SPEC.loader
MODEL = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODEL
SPEC.loader.exec_module(MODEL)


class EmitFixedVectorReport(unittest.TestCase):
    def test_emit_report_for_vector_capture(self) -> None:
        print("F5_PHASE2_REPORT_BEGIN")
        print(MODEL.render_report(), end="")
        print("F5_PHASE2_REPORT_END")
        self.assertTrue(True)


if __name__ == "__main__":
    unittest.main()

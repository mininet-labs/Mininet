#!/usr/bin/env python3
"""Make the input hardener patch-only; the workflow runs checks after all transforms."""

from pathlib import Path


path = Path(__file__).with_name("harden_forge_time_index_input_bounds.py")
text = path.read_text(encoding="utf-8")
start = text.index('    SELF.unlink()\n    WORKFLOW.unlink()\n')
end = text.index('\n\n\nif __name__ == "__main__":', start)
replacement = '    SELF.unlink()\n    WORKFLOW.unlink()'
path.write_text(text[:start] + replacement + text[end:], encoding="utf-8")
Path(__file__).unlink()

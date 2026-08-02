#!/usr/bin/env python3
"""One-shot final cleanup for PR #285.

Removes temporary branch automation, fixes the final STATUS truth-sync, reruns
model/vector tests, and rebuilds generated navigation against the actual merge
candidate tree. This file removes itself before the commit is created.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
STATUS = ROOT / "docs" / "STATUS.md"
TEMPORARY_PATHS = (
    ROOT / ".github" / "workflows" / "f5-phase2-finalize.yml",
    ROOT / "tools" / "f5_phase2_finalize.py",
    ROOT / "tools" / "f5_phase2_cleanup.py",
)


def fix_status() -> None:
    text = STATUS.read_text(encoding="utf-8")
    old = '''  f5-phase2-settlement-model.md` and `docs/design/
  federated-search-exchange-f1-f2.md`.
  See `docs/design/federated-search-exchange-f1-f2.md`.'''
    new = '''  f5-phase2-settlement-model.md` and `docs/design/
  federated-search-exchange-f1-f2.md`.'''
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"docs/STATUS.md: expected one duplicate-link target, found {count}"
        )
    STATUS.write_text(text.replace(old, new, 1), encoding="utf-8")


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def main() -> None:
    fix_status()
    run(sys.executable, "-m", "unittest", "tools/test_f5_phase2_model.py")
    run(sys.executable, "-m", "unittest", "tools/test_f5_phase2_vectors.py")

    for path in TEMPORARY_PATHS:
        path.unlink(missing_ok=False)

    # Navigation must be generated only after temporary paths disappear, so
    # the checked-in index describes the exact tree a reviewer may merge.
    run(sys.executable, "tools/mininet_nav.py", "build")


if __name__ == "__main__":
    main()

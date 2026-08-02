#!/usr/bin/env python3
"""Remove a nonexistent mininet-nav check subcommand from the finalizer."""

from pathlib import Path


path = Path(__file__).with_name("finalize_forge_time_index.py")
text = path.read_text(encoding="utf-8")
old = '    run("python3", "tools/mininet_nav.py", "check")\n'
if text.count(old) != 1:
    raise SystemExit(f"expected one unsupported nav check, found {text.count(old)}")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
Path(__file__).unlink()

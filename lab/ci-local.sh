#!/usr/bin/env bash
# Mirrors the `check` job of .github/workflows/ci.yml plus the cheap repo gates.
# Usage: lab/ci-local.sh [--quick]   (--quick skips the full workspace test run)
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$LAB_ROOT"
step() { printf '\n== %s ==\n' "$*"; }
step fmt;     cargo fmt --all -- --check
step clippy;  cargo clippy --keep-going --all-targets --all-features -- -D warnings
if [ "${1:-}" != "--quick" ]; then step test; cargo test --all --all-features; fi
step "beta interchange adapters"
VENV="${TMPDIR:-/tmp}/mininet-lab-beta-venv"
[ -x "$VENV/bin/python" ] || python3 -m venv "$VENV"
"$VENV/bin/python" -m pip install -q -r tools/requirements-beta-interchange.txt
"$VENV/bin/python" -m unittest discover -s tools -p test_check_beta_payload.py
step "nav index"
python3 tools/mininet_nav.py build
git diff --quiet -- docs/_generated || { echo "FAIL: docs/_generated is stale; commit the regenerated index" >&2; exit 1; }
step "decisions (against origin/main)"
git fetch -q origin main
python3 tools/check_decisions.py --canonical origin/main
echo; echo "ci-local: OK"

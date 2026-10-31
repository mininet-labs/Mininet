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
step "beta interchange adapters"; python3 -m unittest discover -s tools -p test_check_beta_payload.py
step "nav index";  python3 tools/mininet_nav.py build
step "decisions";  python3 tools/check_decisions.py
git diff --quiet -- docs || echo "note: docs changed (nav index?) - commit it"
echo; echo "ci-local: OK"

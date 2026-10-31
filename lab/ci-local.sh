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
VENV="$(mktemp -d)"   # fresh, owner-private, never a shared predictable path
trap 'rm -rf "$VENV"' EXIT
python3 -m venv "$VENV"
"$VENV/bin/python" -m pip install -q -r tools/requirements-beta-interchange.txt
"$VENV/bin/python" -m unittest discover -s tools -p test_check_beta_payload.py
step "nav index"
# Index only tracked + untracked-but-not-ignored files (what a contributor can
# commit), so ignored build artifacts never make the check fail.
NAV="$(mktemp -d)"
trap 'rm -rf "$VENV" "$NAV"' EXIT
git ls-files -z --cached --others --exclude-standard \
    | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
    | tar --null -T - -cf - | tar -x -C "$NAV"
(cd "$NAV" && python3 tools/mininet_nav.py build >/dev/null)
if ! diff -r "$NAV/docs/_generated" docs/_generated >/dev/null; then
    echo "FAIL: docs/_generated is stale; run: python3 tools/mininet_nav.py build, then commit" >&2
    exit 1
fi
step "decisions (against origin/main)"
git fetch -q origin main
python3 tools/check_decisions.py --canonical origin/main
echo; echo "ci-local: OK"

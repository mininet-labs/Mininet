#!/usr/bin/env bash
# Run crate tests as REAL Windows binaries (x86_64-pc-windows-gnu) under wine.
# Usage: lab/windows-test.sh [-p crate]...   (default: mini-durable mini-app-protocol)
# Wine is not Windows: it catches path/prefix/IO-semantics bugs, but the
# windows-client CI job on real Windows remains authoritative.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"
cd "$LAB_ROOT"
[ -n "${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER:-}" ] || { echo "wine missing: lab/bootstrap.sh" >&2; exit 2; }
export DISPLAY="${DISPLAY:-}"
pkgs=("$@")
[ ${#pkgs[@]} -gt 0 ] || pkgs=(-p mini-durable -p mini-app-protocol)
exec cargo test --target x86_64-pc-windows-gnu "${pkgs[@]}"

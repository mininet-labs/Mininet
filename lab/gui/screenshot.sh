#!/usr/bin/env bash
# Launches mininet-desktop on a virtual X display and saves a screenshot, so a
# headless agent can see whether the GUI starts and renders. Output: PNG path.
# Usage: lab/gui/screenshot.sh [out.png] [seconds-to-wait]
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
cd "$LAB_ROOT"
OUT="${1:-$LAB_ROOT/target/lab-gui.png}"; WAIT="${2:-6}"
cargo build -p mini-desktop --bin mininet-desktop
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
# Pick a free display and verify our own server owns it.
for n in $(seq 90 130); do [ -e "/tmp/.X11-unix/X$n" ] || [ -e "/tmp/.X$n-lock" ] || break; done
D=:$n; Xvfb $D -screen 0 1280x800x24 >/dev/null 2>&1 & XV=$!
APP=""
trap '[ -z "$APP" ] || kill "$APP" 2>/dev/null; kill "$XV" 2>/dev/null || true' EXIT
sleep 1
kill -0 "$XV" 2>/dev/null || { echo "Xvfb failed to start on $D" >&2; exit 1; }
HOME_DIR="$(mktemp -d)"
DISPLAY=$D MININET_HOME="$HOME_DIR" LIBGL_ALWAYS_SOFTWARE=1 "$TARGET_DIR/debug/mininet-desktop" >"$HOME_DIR/app.log" 2>&1 & APP=$!
sleep "$WAIT"
kill -0 "$APP" 2>/dev/null || { echo "app exited early:" >&2; tail -20 "$HOME_DIR/app.log" >&2; exit 1; }
mkdir -p "$(dirname "$OUT")"
DISPLAY=$D import -window root "$OUT"
echo "$OUT"

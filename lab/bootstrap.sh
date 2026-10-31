#!/usr/bin/env bash
# Installs everything the lab needs on a Debian/Ubuntu machine (root or sudo).
# Idempotent. Nothing here touches the repository.
set -euo pipefail
SUDO=""; [ "$(id -u)" -eq 0 ] || SUDO="sudo"
export DEBIAN_FRONTEND=noninteractive
$SUDO apt-get update -qq
$SUDO apt-get install -y --no-install-recommends \
    build-essential pkg-config cmake git curl ca-certificates python3 python3-venv \
    iproute2 netcat-openbsd \
    mingw-w64 wine64 \
    xvfb imagemagick xdotool x11-utils libgl1 libegl1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libxcursor1 libxi6 libxrandr2
if ! command -v rustup >/dev/null; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    . "$HOME/.cargo/env"
fi
cd "$(dirname "${BASH_SOURCE[0]}")/.."
rustup show >/dev/null                     # honours rust-toolchain.toml
rustup target add x86_64-pc-windows-gnu wasm32-wasip2
command -v cargo-deny >/dev/null || cargo install --locked cargo-deny || true
echo "lab bootstrap done; run lab/doctor.sh"

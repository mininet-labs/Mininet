#!/bin/sh
# Build a Mininet Windows client release: binaries, package container,
# readable manifest, self-contained setup executable, and SHA256SUMS.
#
# Runs on Linux, macOS, or Windows (Git Bash / WSL). The PowerShell script
# beside this one does the same thing for people who would rather not have a
# shell; both call the same `mini windows pack`, so neither can drift into
# producing a differently-shaped package.
#
# Reproducibility: the package's build timestamp comes from the git commit's
# own author date, not from the clock. Two people building the same commit
# therefore produce byte-identical containers, which is what lets them
# compare digests and conclude something — a script that stamped "now" would
# make every rebuild differ and quietly destroy that check. Pass
# --built-at-ms to override, for a build outside a git checkout.
#
# Usage:
#   packaging/windows/build-release.sh [options]
#
#   --target <triple>    default x86_64-pc-windows-msvc
#   --version <v>        default: read from crates/mini-desktop/Cargo.toml
#   --built-at-ms <ms>   default: git commit author date x 1000
#   --out <dir>          default: dist/windows
#   --host               build for the host instead of cross-compiling;
#                        for testing this pipeline where no Windows
#                        toolchain exists. Produces a package that is
#                        structurally real but not a Windows build.
#   --skip-setup-embed   stop after the container; do not rebuild
#                        mininet-setup with the payload embedded
#
# The MSI for managed deployment is built by the PowerShell script's -Msi
# switch, not here: WiX runs on Windows, and a cross-built MSI nobody can
# install is worse than no MSI.
#
# Exits non-zero on the first failure. Nothing here touches the network
# except cargo's own dependency fetching.

set -eu

TARGET="x86_64-pc-windows-msvc"
VERSION=""
BUILT_AT_MS=""
OUT_DIR=""
HOST_BUILD=0
SKIP_EMBED=0

while [ $# -gt 0 ]; do
    case "$1" in
        --target) TARGET="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --built-at-ms) BUILT_AT_MS="$2"; shift 2 ;;
        --out) OUT_DIR="$2"; shift 2 ;;
        --host) HOST_BUILD=1; shift ;;
        --skip-setup-embed) SKIP_EMBED=1; shift ;;
        -h|--help) sed -n '2,36p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "build-release.sh: unrecognized argument: $1" >&2; exit 2 ;;
    esac
done

REPO_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$REPO_ROOT"

if [ -z "$VERSION" ]; then
    VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/mini-desktop/Cargo.toml | head -1)
fi
if [ -z "$VERSION" ]; then
    echo "build-release.sh: could not read a version; pass --version" >&2
    exit 1
fi

if [ -z "$BUILT_AT_MS" ]; then
    if COMMIT_SECONDS=$(git log -1 --format=%at 2>/dev/null) && [ -n "$COMMIT_SECONDS" ]; then
        BUILT_AT_MS=$((COMMIT_SECONDS * 1000))
    else
        echo "build-release.sh: not a git checkout, so there is no commit date to" >&2
        echo "  use as a reproducible build timestamp. Pass --built-at-ms <ms>." >&2
        exit 1
    fi
fi

if [ "$HOST_BUILD" -eq 1 ]; then
    TARGET_ARGS=""
    BIN_DIR="target/release"
    EXE_SUFFIX=""
    echo "NOTE: --host build. The package is structurally real but its"
    echo "      executables are host binaries, not Windows ones."
else
    TARGET_ARGS="--target $TARGET"
    BIN_DIR="target/$TARGET/release"
    EXE_SUFFIX=".exe"
fi

if [ -z "$OUT_DIR" ]; then
    OUT_DIR="dist/windows"
fi
STAGE_DIR="$OUT_DIR/stage"
CONTAINER="$OUT_DIR/mininet-client-$VERSION-$TARGET.mnpkg"
# An absolute form too: MININET_SETUP_PAYLOAD is read by a build script whose
# working directory is not this one, and --out may already be absolute.
case "$CONTAINER" in
    /*) CONTAINER_ABS="$CONTAINER" ;;
    *) CONTAINER_ABS="$REPO_ROOT/$CONTAINER" ;;
esac
SETUP_OUT="$OUT_DIR/mininet-setup-$VERSION-$TARGET$EXE_SUFFIX"

echo "== Mininet Windows release =="
echo "   version      $VERSION"
echo "   target       $TARGET"
echo "   built-at-ms  $BUILT_AT_MS"
echo "   out          $OUT_DIR"
echo

rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/docs"

echo "-- building client, cli, and setup"
# shellcheck disable=SC2086
cargo build --release $TARGET_ARGS \
    -p mini-desktop -p mini-cli -p mini-setup

echo "-- staging package files"
# Staged names always end in .exe: this is a Windows package, and the
# manifest's launch target and the setup program's own uninstall command are
# both looked up by that name. A --host build stages host binaries under the
# same names on purpose, so the pipeline under test is the real one.
cp "$BIN_DIR/mininet-desktop$EXE_SUFFIX" "$STAGE_DIR/mininet-desktop.exe"
cp "$BIN_DIR/mini$EXE_SUFFIX" "$STAGE_DIR/mini.exe"
cp "$BIN_DIR/mininet-setup$EXE_SUFFIX" "$STAGE_DIR/mininet-setup.exe"
cp docs/WINDOWS_CLIENT_SECURITY.md "$STAGE_DIR/docs/SECURITY.txt"
cp docs/guides/windows-install-guide.md "$STAGE_DIR/docs/INSTALL.txt"
cp crates/mini-desktop/README.md "$STAGE_DIR/docs/CLIENT.txt"
cp LICENSE "$STAGE_DIR/docs/LICENSE.txt"

# The setup program inside the package is deliberately the variant *without*
# an embedded payload: it is what Apps & features runs to uninstall, verify,
# or roll back an existing install, so it never needs to carry a copy of the
# package it came from. The distributable setup executable built below is the
# same code with the payload embedded.

echo "-- packing the container"
cargo run --release -q -p mini-cli -- windows pack \
    --source "$STAGE_DIR" \
    --out "$CONTAINER" \
    --version "$VERSION" \
    --target "$TARGET" \
    --built-at-ms "$BUILT_AT_MS" \
    --shortcut "mininet-desktop.exe=Mininet"

echo
echo "-- verifying the container reads back exactly"
cargo run --release -q -p mini-cli -- windows inspect "$CONTAINER"

if [ "$SKIP_EMBED" -eq 0 ]; then
    echo
    echo "-- rebuilding setup with the package embedded"
    # A fresh build with the payload variable set; build.rs reruns because it
    # declares rerun-if-env-changed for exactly this variable.
    # shellcheck disable=SC2086
    MININET_SETUP_PAYLOAD="$CONTAINER_ABS" \
        cargo build --release $TARGET_ARGS -p mini-setup
    cp "$BIN_DIR/mininet-setup$EXE_SUFFIX" "$SETUP_OUT"
    echo "   $SETUP_OUT"
fi

echo
echo "-- writing SHA256SUMS.txt"
( cd "$OUT_DIR" && \
  find . -maxdepth 1 -type f ! -name SHA256SUMS.txt -print \
  | sed 's|^\./||' | sort \
  | while read -r name; do
      if command -v sha256sum >/dev/null 2>&1; then
          sha256sum "$name"
      else
          shasum -a 256 "$name"
      fi
    done > SHA256SUMS.txt )
cat "$OUT_DIR/SHA256SUMS.txt"

echo
echo "Done. To install on Windows:"
echo "  $(basename "$SETUP_OUT")                 open the installer window"
echo "  $(basename "$SETUP_OUT") --silent        install with no window"
echo "  $(basename "$SETUP_OUT") --dry-run       print every change, make none"
echo
echo "This build is not code-signed. SmartScreen will warn on first run."
echo "Every file can be checked against $(basename "$CONTAINER" .mnpkg).manifest.txt"
echo "with Get-FileHash, needing no Mininet binary at all."

#!/usr/bin/env sh
set -eu

profile="${1:-debug}"
case "$profile" in
  debug|release) ;;
  *) echo "usage: build-rust.sh [debug|release]" >&2; exit 2 ;;
esac

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
output="$repo_root/app/android/app/src/main/jniLibs"

if ! command -v cargo-ndk >/dev/null 2>&1; then
  echo "cargo-ndk 4.1.2 is required: cargo install cargo-ndk --version 4.1.2 --locked" >&2
  exit 1
fi

set -- ndk -t arm64-v8a -t x86_64 -o "$output" build -p mini-ffi
if [ "$profile" = release ]; then
  set -- "$@" --release
fi

cd "$repo_root"
cargo "$@"

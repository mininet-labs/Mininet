#!/usr/bin/env bash
# Reports which lab capabilities work on THIS machine. Exit 0 always; read the table.
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"
ok()   { printf '  [ok]   %s\n' "$1"; }
no()   { printf '  [MISS] %-22s %s\n' "$1" "$2"; }
have() { command -v "$1" >/dev/null; }
echo "Mininet lab doctor"
have cargo && ok "cargo $(cargo --version | cut -d' ' -f2)" || no cargo "run lab/bootstrap.sh"
rustup target list --installed 2>/dev/null | grep -q x86_64-pc-windows-gnu && ok "rust windows-gnu target" || no windows-target "rustup target add x86_64-pc-windows-gnu"
have x86_64-w64-mingw32-gcc && ok "mingw-w64" || no mingw-w64 "apt install mingw-w64"
[ -n "${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER:-}" ] && ok "wine runner ($CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER)" || no wine "apt install wine64"
if have ip && have unshare; then
    if unshare -Urmn true 2>/dev/null; then ok "rootless namespaces (netlab, no sudo)"; else no netns "unprivileged user namespaces are disabled here"; fi
else no iproute2 "apt install iproute2 util-linux"; fi
have Xvfb && have import && ok "Xvfb + ImageMagick (gui)" || no gui "apt install xvfb imagemagick xdotool"
have docker && ok docker || no docker "optional (reproducibility jobs)"
have cargo-deny && ok cargo-deny || no cargo-deny "cargo install --locked cargo-deny"
have python3 && ok python3 || no python3 ""
df -h . | awk 'NR==2{print "  disk free: "$4}'

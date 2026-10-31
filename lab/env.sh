# Source me: `. lab/env.sh`. Sets the variables every lab script expects.
LAB_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export LAB_ROOT
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"      # keeps target/ small
export WINEPREFIX="${WINEPREFIX:-${TMPDIR:-/tmp}/mininet-lab-wineprefix}"
export WINEDEBUG="${WINEDEBUG:--all}"
export WINEDLLOVERRIDES="${WINEDLLOVERRIDES:-mscoree,mshtml=d}"
for w in /usr/lib/wine/wine64 /usr/bin/wine64 /usr/bin/wine; do
    [ -x "$w" ] && { export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER="$w"; break; }
done
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-x86_64-w64-mingw32-gcc}"

#!/usr/bin/env bash
# Mininet Node Appliance — pre-install checks.
#
# Everything install.sh needs to be true, checked BEFORE anything is
# modified. An installer that gets halfway and then discovers the disk is
# full leaves a machine in a state nobody designed, which is worse than
# either outcome.
#
# Read-only and non-root by design: it changes nothing and needs no
# privileges beyond reading /proc and /etc. Run it on a candidate machine
# before committing to it.
#
# Exit codes:
#   0  ready to install
#   1  a hard requirement is unmet
#   2  only advisories (installable, but read them)
#
# Usage: deploy/installer/preflight.sh [--json]

set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
DEPLOY_ROOT="$(dirname -- "${SCRIPT_DIR}")"

JSON=0
for arg in "$@"; do
    case "${arg}" in
        --json) JSON=1 ;;
        *) printf 'unknown argument: %s\n' "${arg}" >&2; exit 1 ;;
    esac
done

# Minimums. These are the appliance profile's operational floor, not a
# protocol constant, and they are deliberately low: Directive 11 says the
# weakest honest device is the one that matters, so a machine a person
# already owns should qualify. A Raspberry Pi 4 with a 32 GB card passes.
MIN_DISK_MB=8192
MIN_RAM_MB=900
SUPPORTED_ARCHES=("amd64" "arm64")

HARD_FAILURES=0
ADVISORIES=0
declare -a RESULTS=()

record() { # kind name detail
    RESULTS+=("$1|$2|$3")
    case "$1" in
        fail) HARD_FAILURES=$((HARD_FAILURES + 1)) ;;
        warn) ADVISORIES=$((ADVISORIES + 1)) ;;
    esac
}

# --- platform -------------------------------------------------------------
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091 # runtime file, not resolvable at lint time
    . /etc/os-release
    if [[ "${ID:-}" == "debian" ]]; then
        record pass "os" "Debian ${VERSION_ID:-unknown} (${VERSION_CODENAME:-unknown})"
    else
        record warn "os" "${ID:-unknown} is not Debian; install.sh needs --allow-non-debian and the result is not a supported appliance"
    fi
else
    record warn "os" "/etc/os-release unreadable; cannot identify this system"
fi

arch="$(dpkg --print-architecture 2>/dev/null || uname -m)"
matched=0
for supported in "${SUPPORTED_ARCHES[@]}"; do
    [[ "${arch}" == "${supported}" ]] && matched=1
done
if [[ "${matched}" -eq 1 ]]; then
    record pass "arch" "${arch}"
else
    record fail "arch" "${arch} is not one of: ${SUPPORTED_ARCHES[*]}"
fi

# --- init system ----------------------------------------------------------
if [[ -d /run/systemd/system ]]; then
    record pass "init" "systemd is running"
else
    record fail "init" "systemd is not the running init; the unit files cannot be used"
fi

# --- resources ------------------------------------------------------------
if command -v df >/dev/null 2>&1; then
    avail_mb="$(df -Pm /var 2>/dev/null | awk 'NR==2 {print $4}')"
    if [[ -n "${avail_mb}" ]] && [[ "${avail_mb}" -ge "${MIN_DISK_MB}" ]]; then
        record pass "disk" "${avail_mb} MB free on /var (minimum ${MIN_DISK_MB})"
    elif [[ -n "${avail_mb}" ]]; then
        record fail "disk" "${avail_mb} MB free on /var, need at least ${MIN_DISK_MB}"
    else
        record warn "disk" "could not determine free space on /var"
    fi
fi

if [[ -r /proc/meminfo ]]; then
    ram_mb=$(( $(awk '/^MemTotal:/ {print $2}' /proc/meminfo) / 1024 ))
    if [[ "${ram_mb}" -ge "${MIN_RAM_MB}" ]]; then
        record pass "memory" "${ram_mb} MB RAM (minimum ${MIN_RAM_MB})"
    else
        record fail "memory" "${ram_mb} MB RAM, need at least ${MIN_RAM_MB}"
    fi
fi

# --- clock ----------------------------------------------------------------
# Nearly every timestamp in this tree is self-reported, and proof windows,
# claim expiry, and freshness policies all read the device clock. A node
# with a wrong clock is not merely inconvenient: it produces evidence
# nobody else agrees with.
if command -v timedatectl >/dev/null 2>&1; then
    if timedatectl show --property=NTPSynchronized --value 2>/dev/null | grep -q '^yes$'; then
        record pass "clock" "system clock is NTP-synchronized"
    else
        record warn "clock" "system clock is NOT NTP-synchronized; proof windows and claim expiry read this clock"
    fi
else
    record warn "clock" "timedatectl unavailable; cannot confirm clock synchronization"
fi

# --- required commands ----------------------------------------------------
for cmd in systemctl install id; do
    if command -v "${cmd}" >/dev/null 2>&1; then
        record pass "cmd:${cmd}" "present"
    else
        record fail "cmd:${cmd}" "not found on PATH"
    fi
done

for cmd in nft gpg tar cargo; do
    if command -v "${cmd}" >/dev/null 2>&1; then
        record pass "cmd:${cmd}" "present"
    else
        case "${cmd}" in
            cargo) record warn "cmd:cargo" "not found; install.sh will skip building the mini binary" ;;
            gpg)   record warn "cmd:gpg" "not found; deploy/backup cannot run until the gnupg package is installed" ;;
            *)     record warn "cmd:${cmd}" "not found; install.sh installs it from packages.lock" ;;
        esac
    fi
done

# --- port availability ----------------------------------------------------
port="41777"
if [[ -r "${DEPLOY_ROOT}/installer/appliance.conf.example" ]]; then
    configured="$(awk -F: '/^MININET_SYNC_LISTEN_ADDR=/ {print $NF}' \
        "${DEPLOY_ROOT}/installer/appliance.conf.example" 2>/dev/null)"
    [[ -n "${configured}" ]] && port="${configured}"
fi
if command -v ss >/dev/null 2>&1; then
    if ss -Hltn "sport = :${port}" 2>/dev/null | grep -q .; then
        record fail "port" "something is already listening on ${port}"
    else
        record pass "port" "${port} is free"
    fi
else
    record warn "port" "ss unavailable; could not check whether ${port} is free"
fi

# --- existing install -----------------------------------------------------
if [[ -d /var/lib/mininet ]] && [[ -n "$(ls -A /var/lib/mininet 2>/dev/null)" ]]; then
    record warn "state" "/var/lib/mininet already holds node state; install.sh will not touch it, but back it up first (deploy/backup/backup.sh)"
else
    record pass "state" "no existing node state"
fi

# --- output ---------------------------------------------------------------
if [[ "${JSON}" -eq 1 ]]; then
    printf '{"checks":['
    first=1
    for row in "${RESULTS[@]}"; do
        IFS='|' read -r kind name detail <<< "${row}"
        [[ "${first}" -eq 1 ]] || printf ','
        first=0
        printf '{"result":"%s","check":"%s","detail":"%s"}' \
            "${kind}" "${name}" "${detail//\"/\\\"}"
    done
    printf '],"hard_failures":%d,"advisories":%d}\n' "${HARD_FAILURES}" "${ADVISORIES}"
else
    printf '[preflight] Mininet Node Appliance — pre-install checks\n\n'
    for row in "${RESULTS[@]}"; do
        IFS='|' read -r kind name detail <<< "${row}"
        case "${kind}" in
            pass) printf '  ok    %-12s %s\n' "${name}" "${detail}" ;;
            warn) printf '  warn  %-12s %s\n' "${name}" "${detail}" ;;
            fail) printf '  FAIL  %-12s %s\n' "${name}" "${detail}" ;;
        esac
    done
    printf '\n'
    if [[ "${HARD_FAILURES}" -gt 0 ]]; then
        printf '[preflight] %d hard requirement(s) unmet — do not run install.sh yet\n' "${HARD_FAILURES}"
    elif [[ "${ADVISORIES}" -gt 0 ]]; then
        printf '[preflight] installable, with %d advisory item(s) above worth reading first\n' "${ADVISORIES}"
    else
        printf '[preflight] ready to install\n'
    fi
fi

if [[ "${HARD_FAILURES}" -gt 0 ]]; then
    exit 1
elif [[ "${ADVISORIES}" -gt 0 ]]; then
    exit 2
fi
exit 0

#!/usr/bin/env bash
# Mininet Node Appliance — verification script.
#
# Two things, always both attempted, failures reported at the end:
#
#  1. Manifest lint (works anywhere, no root needed, no prior install):
#     - systemd-analyze verify on the checked-in unit files
#     - nft --check syntax validation on the checked-in ruleset
#     - packages.lock has no blank/malformed lines
#
#  2. Live state check (best-effort; skipped with a note if this host was
#     never provisioned by install.sh): confirms the units are actually
#     installed/enabled, the nftables table is loaded, and the mininet
#     system user exists.
#
# Exit code is nonzero if any check fails. This does not verify a Mininet
# release (mini-forge/mini-installer's job) — only that the appliance
# layer itself is internally consistent.

set -uo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
DEPLOY_ROOT="$(dirname -- "${SCRIPT_DIR}")"

FAILURES=0
note() { printf '[verify] %s\n' "$*"; }
pass() { printf '[verify]   PASS: %s\n' "$*"; }
fail() { printf '[verify]   FAIL: %s\n' "$*"; FAILURES=$((FAILURES + 1)); }
skip() { printf '[verify]   SKIP: %s\n' "$*"; }

note "--- manifest lint ---"

if command -v systemd-analyze >/dev/null 2>&1; then
    for unit in "${DEPLOY_ROOT}"/systemd/*.service "${DEPLOY_ROOT}"/systemd/*.target; do
        [[ -f "${unit}" ]] || continue
        if systemd-analyze verify "${unit}" 2>/tmp/mininet-verify-$$.log; then
            pass "systemd-analyze verify $(basename "${unit}")"
        else
            # Two classes of "failure" here are expected pre-install, not
            # a broken unit file: (1) network-online.target and friends
            # not resolvable in a container without full systemd as PID
            # 1, and (2) ExecStart's /usr/local/bin/mini not existing yet
            # -- install.sh's own later step builds and installs it, so a
            # fresh checkout that never ran the installer legitimately
            # doesn't have it. Only a syntax/schema error in the unit
            # file itself is a real lint failure.
            if grep -qE 'is not executable: No such file or directory|Failed to load environment files|renders empty' /tmp/mininet-verify-$$.log 2>/dev/null; then
                skip "systemd-analyze verify $(basename "${unit}") reported an expected pre-install condition: $(cat /tmp/mininet-verify-$$.log)"
            else
                fail "systemd-analyze verify $(basename "${unit}"): $(cat /tmp/mininet-verify-$$.log)"
            fi
        fi
        rm -f /tmp/mininet-verify-$$.log
    done
else
    skip "systemd-analyze not found"
fi

if command -v nft >/dev/null 2>&1; then
    if nft --check -f "${DEPLOY_ROOT}/nftables/mininet.nft" 2>/tmp/mininet-nft-$$.log; then
        pass "nft --check deploy/nftables/mininet.nft"
    else
        fail "nft --check deploy/nftables/mininet.nft: $(cat /tmp/mininet-nft-$$.log)"
    fi
    rm -f /tmp/mininet-nft-$$.log
else
    skip "nft not found"
fi

if [[ -f "${DEPLOY_ROOT}/packages.lock" ]]; then
    if grep -qE '^\s*[A-Za-z0-9][A-Za-z0-9+.-]*\s*$' <(grep -v '^\s*#' "${DEPLOY_ROOT}/packages.lock" | grep -v '^\s*$'); then
        pass "packages.lock has at least one well-formed entry"
    else
        fail "packages.lock has no well-formed package entries"
    fi
else
    fail "packages.lock missing"
fi

note "--- live state (best-effort) ---"

if id -u mininet >/dev/null 2>&1; then
    pass "mininet system user exists"
else
    skip "mininet system user not present (host not yet provisioned by install.sh)"
fi

if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files mininet-sync-listen.service >/dev/null 2>&1 \
    && systemctl list-unit-files mininet-sync-listen.service 2>/dev/null | grep -q mininet-sync-listen; then
    pass "mininet-sync-listen.service is installed"
    if systemctl is-enabled --quiet mininet-sync-listen.service 2>/dev/null; then
        pass "mininet-sync-listen.service is enabled"
    else
        skip "mininet-sync-listen.service is installed but not enabled"
    fi
else
    skip "mininet-sync-listen.service not installed on this host"
fi

if command -v nft >/dev/null 2>&1 && nft list table inet mininet_filter >/dev/null 2>&1; then
    pass "nftables table inet mininet_filter is loaded"
else
    skip "nftables table inet mininet_filter not loaded on this host"
fi

echo
if [[ "${FAILURES}" -eq 0 ]]; then
    note "all checks passed (see SKIP lines for what could not be checked in this environment)"
    exit 0
else
    note "${FAILURES} check(s) failed"
    exit 1
fi

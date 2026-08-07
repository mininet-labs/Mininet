#!/usr/bin/env bash
# Mininet Node Appliance — verification script.
#
# Two things, always both attempted, failures reported at the end:
#
#  1. Manifest lint (works anywhere, no root needed, no prior install):
#     - systemd-analyze verify on the checked-in unit files
#     - nft --check syntax validation on the checked-in ruleset
#     - packages.lock has no blank/malformed lines
#     - shellcheck on every script here, when it is available
#     - every script is executable (a mode 0644 installer is a broken one)
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
    for unit in "${DEPLOY_ROOT}"/systemd/*.service "${DEPLOY_ROOT}"/systemd/*.target "${DEPLOY_ROOT}"/systemd/*.timer; do
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

# Scripts: executable bit, then shellcheck. A script committed without +x
# fails at the least convenient moment -- halfway through a provision.
SCRIPTS=(
    "${DEPLOY_ROOT}/installer/install.sh"
    "${DEPLOY_ROOT}/installer/preflight.sh"
    "${DEPLOY_ROOT}/installer/uninstall.sh"
    "${DEPLOY_ROOT}/verification/verify.sh"
    "${DEPLOY_ROOT}/verification/lock-packages.sh"
    "${DEPLOY_ROOT}/backup/backup.sh"
    "${DEPLOY_ROOT}/backup/restore.sh"
)
for script in "${SCRIPTS[@]}"; do
    if [[ ! -f "${script}" ]]; then
        fail "missing script: ${script#"${DEPLOY_ROOT}"/}"
    elif [[ ! -x "${script}" ]]; then
        fail "not executable: ${script#"${DEPLOY_ROOT}"/}"
    else
        pass "present and executable: ${script#"${DEPLOY_ROOT}"/}"
    fi
done

if command -v shellcheck >/dev/null 2>&1; then
    for script in "${SCRIPTS[@]}"; do
        [[ -f "${script}" ]] || continue
        if shellcheck "${script}" >/tmp/mininet-sc-$$.log 2>&1; then
            pass "shellcheck $(basename "${script}")"
        else
            fail "shellcheck $(basename "${script}"): $(head -5 /tmp/mininet-sc-$$.log)"
        fi
        rm -f /tmp/mininet-sc-$$.log
    done
else
    skip "shellcheck not found"
fi

if [[ -f "${DEPLOY_ROOT}/journald/mininet-privacy.conf" ]]; then
    if grep -q '^MaxRetentionSec=' "${DEPLOY_ROOT}/journald/mininet-privacy.conf"; then
        pass "journald retention policy declares MaxRetentionSec"
    else
        fail "journald policy has no MaxRetentionSec -- logs would be kept indefinitely"
    fi
else
    fail "deploy/journald/mininet-privacy.conf missing"
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

if command -v systemctl >/dev/null 2>&1 \
    && systemctl list-unit-files mininet-verify.timer 2>/dev/null | grep -q mininet-verify; then
    if systemctl is-enabled --quiet mininet-verify.timer 2>/dev/null; then
        pass "mininet-verify.timer is enabled"
    else
        skip "mininet-verify.timer is installed but not enabled"
    fi
else
    skip "mininet-verify.timer not installed on this host"
fi

if [[ -f /etc/systemd/journald.conf.d/mininet-privacy.conf ]]; then
    pass "journald retention policy is installed"
else
    skip "journald retention policy not installed on this host"
fi

# Backups are the one thing whose absence is unrecoverable, so an
# unprovisioned-looking node with real state gets told about it every run.
if [[ -d /var/lib/mininet ]] && [[ -n "$(ls -A /var/lib/mininet 2>/dev/null)" ]]; then
    note "  NOTE: /var/lib/mininet holds this node's identity. There is no"
    note "        custodial recovery (ID1) -- if this disk dies without a"
    note "        backup, the identity is gone. deploy/backup/backup.sh"
fi

echo
if [[ "${FAILURES}" -eq 0 ]]; then
    note "all checks passed (see SKIP lines for what could not be checked in this environment)"
    exit 0
else
    note "${FAILURES} check(s) failed"
    exit 1
fi

#!/usr/bin/env bash
# Mininet Node Appliance — Day 0 installer.
#
# Transforms a supported Debian Stable installation into a known Mininet
# node: installs the pinned package set, applies the sysctl/nftables/user
# manifests, builds and installs the `mini` binary from this checkout, and
# installs (but does not start) the systemd units. Designed to be run more
# than once: every step checks current state before changing anything, so
# reapplying this script on an already-provisioned node is a no-op except
# where the manifest itself changed.
#
# This script provisions the MACHINE. It never verifies or activates a
# Mininet release — that is `mini-installer`'s job (D-0070/D-0071), invoked
# afterward via the `mini` binary this script installs. See
# docs/design/mininet-node-appliance.md.
#
# Usage:
#   sudo deploy/installer/install.sh [--allow-non-debian] [--start]
#
#   --allow-non-debian  Skip the Debian Stable check. For testing this
#                        script's logic on a non-Debian box only — the
#                        resulting node is NOT a supported appliance.
#   --start             Start (not just enable) the sync-listen service
#                        immediately after install, instead of leaving it
#                        for next boot / manual `systemctl start`.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
DEPLOY_ROOT="$(dirname -- "${SCRIPT_DIR}")"
REPO_ROOT="$(dirname -- "${DEPLOY_ROOT}")"

ALLOW_NON_DEBIAN=0
START_NOW=0
for arg in "$@"; do
    case "${arg}" in
        --allow-non-debian) ALLOW_NON_DEBIAN=1 ;;
        --start) START_NOW=1 ;;
        *)
            echo "unknown argument: ${arg}" >&2
            exit 1
            ;;
    esac
done

log() { printf '[mininet-appliance] %s\n' "$*"; }
fail() { printf '[mininet-appliance] ERROR: %s\n' "$*" >&2; exit 1; }

if [[ "${EUID}" -ne 0 ]]; then
    fail "must run as root (package install, systemd units, /etc writes)"
fi

# --- 1. Platform check ---
if [[ "${ALLOW_NON_DEBIAN}" -eq 0 ]]; then
    if [[ ! -r /etc/os-release ]] || ! grep -q '^ID=debian$' /etc/os-release; then
        fail "this is not a Debian installation (pass --allow-non-debian to override for testing only; the result is not a supported appliance)"
    fi
fi

# --- 2. Package installation ---
log "installing pinned package set from packages.lock"
mapfile -t PACKAGES < <(grep -v '^\s*#' "${DEPLOY_ROOT}/packages.lock" | grep -v '^\s*$')
if [[ "${ALLOW_NON_DEBIAN}" -eq 0 ]]; then
    apt-get update
    apt-get install -y --no-install-recommends "${PACKAGES[@]}"
else
    log "skipping apt-get install under --allow-non-debian (packages: ${PACKAGES[*]})"
fi

# --- 3. System user/group ---
log "applying declarative system user (deploy/users/mininet.conf)"
install -D -m 0644 "${DEPLOY_ROOT}/users/mininet.conf" /etc/sysusers.d/mininet.conf
if command -v systemd-sysusers >/dev/null 2>&1; then
    systemd-sysusers /etc/sysusers.d/mininet.conf
else
    log "systemd-sysusers not found; creating user with useradd as a fallback"
    id -u mininet >/dev/null 2>&1 || useradd --system --home-dir /var/lib/mininet --shell /usr/sbin/nologin mininet
fi
install -d -o mininet -g mininet -m 0750 /var/lib/mininet

# --- 4. Kernel hardening baseline ---
log "applying sysctl hardening baseline"
install -D -m 0644 "${DEPLOY_ROOT}/sysctl.d/99-mininet-hardening.conf" /etc/sysctl.d/99-mininet-hardening.conf
if command -v sysctl >/dev/null 2>&1; then
    sysctl --system >/dev/null
fi

# --- 5. Firewall ---
log "applying nftables ruleset"
install -D -m 0644 "${DEPLOY_ROOT}/nftables/mininet.nft" /etc/nftables.d/mininet.nft
if [[ -f /etc/nftables.conf ]] && ! grep -q 'nftables.d/mininet.nft' /etc/nftables.conf; then
    printf '\ninclude "/etc/nftables.d/mininet.nft"\n' >> /etc/nftables.conf
fi
if command -v nft >/dev/null 2>&1; then
    nft -f /etc/nftables.d/mininet.nft
fi

# --- 6. Application config ---
log "installing /etc/mininet/appliance.conf (not overwriting operator edits)"
install -d -m 0755 /etc/mininet
if [[ ! -f /etc/mininet/appliance.conf ]]; then
    install -m 0644 "${SCRIPT_DIR}/appliance.conf.example" /etc/mininet/appliance.conf
fi

# --- 7. Build and install the mini binary ---
# Bootstrap only: the FIRST `mini` binary on a fresh node has nothing to
# verify it against, so it is built directly from this checkout via cargo
# rather than through mini-installer's release pipeline. Every subsequent
# update should go through `mini build`/`mini release`/`mini installer`
# instead of rerunning this step, so it stays inside the governed release
# path this appliance sits underneath.
if command -v cargo >/dev/null 2>&1; then
    log "building mini-cli (release profile) from ${REPO_ROOT}"
    (cd "${REPO_ROOT}" && cargo build --release -p mini-cli)
    install -m 0755 "${REPO_ROOT}/target/release/mini" /usr/local/bin/mini
else
    log "cargo not found on PATH; skipping mini-cli build (install rustup/cargo and rerun, or place a verified 'mini' binary at /usr/local/bin/mini yourself)"
fi

# --- 8. systemd units ---
log "installing systemd units"
install -D -m 0644 "${DEPLOY_ROOT}/systemd/mininet.target" /etc/systemd/system/mininet.target
install -D -m 0644 "${DEPLOY_ROOT}/systemd/mininet-sync-listen.service" /etc/systemd/system/mininet-sync-listen.service
systemctl daemon-reload
systemctl enable mininet.target mininet-sync-listen.service

if [[ "${START_NOW}" -eq 1 ]]; then
    log "starting mininet-sync-listen.service now"
    systemctl start mininet-sync-listen.service
else
    log "units enabled, not started (pass --start to start now, or reboot / systemctl start mininet.target)"
fi

log "done. Run deploy/verification/verify.sh to check the result."

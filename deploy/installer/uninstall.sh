#!/usr/bin/env bash
# Mininet Node Appliance — clean removal.
#
# # Why an appliance needs this
#
# "No off switch" (P3/U1) is about the *network*: nobody can remotely
# disable your node, and no update can be forced on you. It is not a claim
# that a person cannot remove software from their own machine. The opposite,
# in fact — Directive 1 puts sovereignty above convenience, and software you
# cannot uninstall is software that owns the machine rather than serving it.
#
# So removal is a first-class operation, and it is entirely local: this
# script tells nobody, revokes nothing, and asks no permission.
#
# # What it will not do without being told twice
#
# Node state (/var/lib/mininet) is **kept by default**. It holds the
# identity, and ID1 means a deleted identity is gone permanently — no
# custodial recovery exists, by design. Removing the software is reversible;
# deleting the keys is not, so the two are separate decisions and the
# irreversible one requires --purge-state and a typed confirmation.
#
# Usage:
#   sudo deploy/installer/uninstall.sh                  # software only, state kept
#   sudo deploy/installer/uninstall.sh --purge-state    # also delete the identity
#   sudo deploy/installer/uninstall.sh --keep-packages  # leave apt packages installed

set -euo pipefail

STATE_DIR="${MININET_STATE_DIR:-/var/lib/mininet}"

PURGE_STATE=0
KEEP_PACKAGES=1   # default: never remove packages other things may need
for arg in "$@"; do
    case "${arg}" in
        --purge-state) PURGE_STATE=1 ;;
        --keep-packages) KEEP_PACKAGES=1 ;;
        *) printf 'unknown argument: %s\n' "${arg}" >&2; exit 1 ;;
    esac
done

log() { printf '[mininet-uninstall] %s\n' "$*"; }
fail() { printf '[mininet-uninstall] ERROR: %s\n' "$*" >&2; exit 1; }

[[ "${EUID}" -eq 0 ]] || fail "must run as root"

# --- 1. Stop and disable units -------------------------------------------
log "stopping and disabling units"
for unit in mininet-verify.timer mininet-verify.service mininet-sync-listen.service mininet.target; do
    if systemctl list-unit-files "${unit}" >/dev/null 2>&1; then
        systemctl disable --now "${unit}" >/dev/null 2>&1 || true
    fi
done

# --- 2. Remove unit files -------------------------------------------------
log "removing unit files"
for unit in mininet.target mininet-sync-listen.service mininet-verify.service mininet-verify.timer; do
    rm -f "/etc/systemd/system/${unit}"
done
systemctl daemon-reload
systemctl reset-failed >/dev/null 2>&1 || true

# --- 3. Remove appliance-owned system configuration ----------------------
# Only files this appliance installed. /etc/nftables.conf and
# /etc/sysctl.conf belong to the operator and are left alone apart from
# removing the include line we added.
log "removing appliance system configuration"
rm -f /etc/sysctl.d/99-mininet-hardening.conf
rm -f /etc/sysusers.d/mininet.conf
rm -f /etc/nftables.d/mininet.nft
rmdir /etc/nftables.d 2>/dev/null || true

if [[ -f /etc/nftables.conf ]] && grep -q 'nftables.d/mininet.nft' /etc/nftables.conf; then
    log "removing the include line this installer appended to /etc/nftables.conf"
    # Drop our include and any blank line immediately preceding it.
    tmp="$(mktemp)"
    grep -v 'nftables.d/mininet.nft' /etc/nftables.conf > "${tmp}"
    install -m 0644 "${tmp}" /etc/nftables.conf
    rm -f "${tmp}"
    log "NOTE: the running nftables ruleset is unchanged until you reload it."
    log "      This node's rules were a default-deny policy; reloading now with"
    log "      no replacement may leave the machine more open than it was."
fi

if command -v sysctl >/dev/null 2>&1; then
    sysctl --system >/dev/null 2>&1 || true
fi

# --- 4. Remove the binary -------------------------------------------------
if [[ -f /usr/local/bin/mini ]]; then
    log "removing /usr/local/bin/mini"
    rm -f /usr/local/bin/mini
fi

# --- 5. Packages ----------------------------------------------------------
if [[ "${KEEP_PACKAGES}" -eq 1 ]]; then
    log "leaving apt packages installed (they are ordinary Debian packages other software may depend on)"
fi

# --- 6. State — the irreversible part ------------------------------------
if [[ "${PURGE_STATE}" -eq 1 ]]; then
    if [[ -d "${STATE_DIR}" ]]; then
        log ""
        log "About to permanently delete ${STATE_DIR}."
        log "This is the node's did:mini identity and key event log. There is no"
        log "custodial recovery anywhere in Mininet (ID1) — not by an operator,"
        log "not by the founder, not by any quorum. Once deleted it cannot be"
        log "restored except from a backup you already made."
        log ""
        read -r -p "[mininet-uninstall] Type 'delete my identity' to confirm: " confirm
        if [[ "${confirm}" == "delete my identity" ]]; then
            rm -rf -- "${STATE_DIR}"
            rm -rf -- /etc/mininet
            log "state deleted"
        else
            log "state NOT deleted (confirmation did not match)"
        fi
    fi
else
    log "node state left in place at ${STATE_DIR}"
    log "  the identity is intact; reinstalling picks up exactly where this left off"
    log "  pass --purge-state to delete it (irreversible — back it up first)"
fi

# --- 7. System user -------------------------------------------------------
# Kept whenever state is kept: removing the user while its files remain
# leaves them owned by a bare uid, which is how a later reinstall ends up
# unable to read its own state.
if [[ "${PURGE_STATE}" -eq 1 ]] && id -u mininet >/dev/null 2>&1 && [[ ! -d "${STATE_DIR}" ]]; then
    log "removing the mininet system user"
    userdel mininet 2>/dev/null || true
elif id -u mininet >/dev/null 2>&1; then
    log "leaving the mininet system user (it still owns ${STATE_DIR})"
fi

log "done"

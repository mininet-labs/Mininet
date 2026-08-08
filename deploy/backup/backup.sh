#!/usr/bin/env bash
# Mininet Node Appliance — node state backup.
#
# # Why this exists
#
# /var/lib/mininet holds this node's identity: its `did:mini` keys and its
# key event log. ID1 is explicit that keys never leave the device and there
# is no custodial recovery — which is the right property, and it has a
# consequence people discover too late. **If this directory is lost, the
# identity is gone permanently.** No operator, no founder, and no quorum can
# restore it. There is nobody to ask.
#
# So an appliance that can be installed but not backed up is an appliance
# that loses identities to dead disks. That is not a hypothetical for a
# network whose thesis is a thousand cheap machines: cheap machines are
# exactly the ones whose storage fails.
#
# # What this is, and is not
#
# It is a local archive of node state, encrypted with a passphrase the
# operator supplies and this script never stores. It is deliberately NOT:
#
#   - an upload to anywhere. Directive 2 says assume every service
#     disappears; a backup tool that needs a server has moved the failure,
#     not removed it. Where the archive goes afterward is the operator's
#     decision, and it is a real one — an unencrypted copy on a cloud drive
#     is a key handed to that provider.
#   - a recovery service. There is no key escrow here and there must never
#     be one, because escrow is custodial recovery wearing a different word.
#   - a substitute for did-mini's own pre-rotation and delegation. Those
#     handle *compromise*; this handles *loss*. Different failures.
#
# Usage:
#   deploy/backup/backup.sh /path/to/output-dir
#   MININET_BACKUP_PASSPHRASE=... deploy/backup/backup.sh /path --batch
#
# Restore with deploy/backup/restore.sh.

set -euo pipefail

STATE_DIR="${MININET_STATE_DIR:-/var/lib/mininet}"
CONFIG_FILE="/etc/mininet/appliance.conf"

BATCH=0
OUT_DIR=""
for arg in "$@"; do
    case "${arg}" in
        --batch) BATCH=1 ;;
        -*) printf 'unknown argument: %s\n' "${arg}" >&2; exit 1 ;;
        *) OUT_DIR="${arg}" ;;
    esac
done

log() { printf '[mininet-backup] %s\n' "$*"; }
fail() { printf '[mininet-backup] ERROR: %s\n' "$*" >&2; exit 1; }

[[ -n "${OUT_DIR}" ]] || fail "usage: backup.sh <output-dir> [--batch]"
[[ -d "${STATE_DIR}" ]] || fail "state directory ${STATE_DIR} does not exist — is this an appliance node?"

command -v tar >/dev/null 2>&1 || fail "tar not found"
command -v gpg >/dev/null 2>&1 || fail "gpg not found (install the gnupg package)"

mkdir -p "${OUT_DIR}"
# The archive contains key material. Anyone who can read the directory can
# attempt an offline passphrase attack against it, so the directory itself
# is restricted even though the file is encrypted.
chmod 0700 "${OUT_DIR}"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
archive="${OUT_DIR}/mininet-node-${stamp}.tar.gz.gpg"
manifest="${OUT_DIR}/mininet-node-${stamp}.manifest"

if [[ "${BATCH}" -eq 1 ]]; then
    [[ -n "${MININET_BACKUP_PASSPHRASE:-}" ]] \
        || fail "--batch requires MININET_BACKUP_PASSPHRASE in the environment"
fi

log "archiving ${STATE_DIR}$( [[ -r ${CONFIG_FILE} ]] && printf ' and %s' "${CONFIG_FILE}" )"

# --numeric-owner so a restore onto a machine where 'mininet' has a
# different uid still lands correctly; restore.sh fixes ownership by name.
tar_args=(--numeric-owner -czf - -C "$(dirname "${STATE_DIR}")" "$(basename "${STATE_DIR}")")
if [[ -r "${CONFIG_FILE}" ]]; then
    tar_args+=(-C /etc/mininet appliance.conf)
fi

if [[ "${BATCH}" -eq 1 ]]; then
    tar "${tar_args[@]}" \
        | gpg --batch --yes --symmetric --cipher-algo AES256 \
              --passphrase "${MININET_BACKUP_PASSPHRASE}" \
              --output "${archive}"
else
    log "you will be prompted for a passphrase; there is no way to recover it"
    tar "${tar_args[@]}" \
        | gpg --symmetric --cipher-algo AES256 --output "${archive}"
fi

chmod 0600 "${archive}"

# A digest of the *encrypted* archive, so a later restore can tell a
# corrupted file from a wrong passphrase. It is not a signature and proves
# nothing about origin.
if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${archive}" | awk '{print $1}' > "${manifest}"
    chmod 0600 "${manifest}"
fi

log "wrote ${archive}"
log ""
log "This archive IS the node's identity. Read this before moving on:"
log "  * The passphrase cannot be reset or recovered. Losing it loses the node."
log "  * Store at least one copy off this machine — a backup that dies with"
log "    the disk it protects is not a backup."
log "  * Anywhere you put it can attempt an offline attack on the passphrase."
log "    Choose it accordingly, and prefer somewhere you control."
log "  * Restoring this onto a SECOND machine while the first still runs gives"
log "    two nodes one identity. See restore.sh for why that is not harmless."

#!/usr/bin/env bash
# Two isolated network namespaces joined only by a veth pair. Neither has a
# default route or DNS, so GitHub (or anything else) is unreachable by
# construction. The operator publishes a Beta campaign; the tester discovers
# it over a real TCP `mini sync` across the veth.
#
# NO ROOT, NO SUDO: the whole drill runs inside a throwaway rootless user+mount+
# network namespace (`unshare -Urmn`). "Root" in there is the invoking user
# mapped to uid 0 and has no power over the host, so a hostile checkout cannot
# use this script to gain host privileges. Do not run it with sudo.
# Usage: lab/netlab/outage-drill.sh [path-to-mini]   (builds mini-cli if omitted)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MINI="${1:-}"
if [ "$(id -u)" -eq 0 ] && [ -z "${LAB_NETLAB_INNER:-}" ] && [ -z "${LAB_NETLAB_ALLOW_ROOT:-}" ]; then
    echo "refusing to run as host root: run as a normal user (no sudo)" >&2; exit 1
fi
if [ -z "${LAB_NETLAB_INNER:-}" ] && [ -z "$MINI" ]; then
    # Build the CLI (outside the namespaces, with network for crates if needed)
    # and honour Cargo's configured target directory.
    (cd "$ROOT" && cargo build -p mini-cli --bin mini)
    TARGET_DIR="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    MINI="$TARGET_DIR/debug/mini"
fi
[ -x "$MINI" ] || { echo "mini binary not found: $MINI" >&2; exit 1; }
if [ -z "${LAB_NETLAB_INNER:-}" ]; then
    command -v unshare >/dev/null || { echo "unshare (util-linux) required" >&2; exit 1; }
    LAB_NETLAB_INNER=1 exec unshare -Urmn "$0" "$MINI"
fi
# Inside the private namespaces: a private /run so `ip netns` needs no host files.
mount -t tmpfs tmpfs /run
mkdir -p /run/netns
ip link set lo up
W="$(mktemp -d)"
A=labop; B=labte; VA=vA; VB=vB
LISTENER=""
cleanup() { [ -z "$LISTENER" ] || { kill "$LISTENER" 2>/dev/null; wait "$LISTENER" 2>/dev/null; } || true; rm -rf "$W"; }
trap cleanup EXIT
ip netns add "$A"; ip netns add "$B"
ip link add "$VA" type veth peer name "$VB"
ip link set "$VA" netns "$A"; ip link set "$VB" netns "$B"
ip -n "$A" addr add 10.77.0.1/24 dev "$VA"; ip -n "$B" addr add 10.77.0.2/24 dev "$VB"
for n in "$A:$VA" "$B:$VB"; do ip -n "${n%%:*}" link set lo up; ip -n "${n%%:*}" link set "${n##*:}" up; done
step() { printf '\n== %s ==\n' "$*"; }
op() { ip netns exec "$A" "$MINI" --home "$W/op" --store "$W/op-store" "$@"; }
te() { ip netns exec "$B" "$MINI" --home "$W/te" --store "$W/te-store" "$@"; }
opj() { ip netns exec "$A" "$MINI" --json --home "$W/op" --store "$W/op-store" "$@"; }
tej() { ip netns exec "$B" "$MINI" --json --home "$W/te" --store "$W/te-store" "$@"; }

step "isolation: no route to the outside"
for ns in "$A" "$B"; do
    [ -z "$(ip -n "$ns" route show default)" ] || { echo "$ns has a default route" >&2; exit 1; }
    ! ip netns exec "$ns" nc -z -w1 1.1.1.1 443 2>/dev/null || { echo "$ns reached the internet" >&2; exit 1; }
done
echo "neither node has a default route or can reach 1.1.1.1 (expected)"

step "operator: identity, project, campaign"
op identity init >/dev/null
PROJECT="$(op repo init beta-drill | awk '{print $NF}')"
EPOCH=0101010101010101010101010101010101010101010101010101010101010101
CAMP="$(opj beta campaign create "$PROJECT" --epoch "$EPOCH" --title Drill \
    --instructions "Report what breaks" --route rust --starts-ms 100 --ends-ms 10000 \
    --testing-grant 1000 | sed -n 's/.*"campaign_id":"\([^"]*\)".*/\1/p')"
[ -n "$CAMP" ] || { echo "campaign id missing" >&2; exit 1; }
te identity init >/dev/null
BEFORE="$(tej beta campaign list)"
case "$BEFORE" in *"$CAMP"*) echo "tester should not know the campaign yet" >&2; exit 1;; esac
echo "tester knows nothing before sync"

step "replication over a real TCP connection inside the veth"
op sync listen --addr 10.77.0.1:7077 >/dev/null &
LISTENER=$!
# `sync listen` serves exactly one connection, so never probe the port: retry the real connect.
for i in $(seq 50); do
    te sync connect 10.77.0.1:7077 >/dev/null 2>&1 && break
    [ "$i" -lt 50 ] || { echo "sync connect never succeeded" >&2; exit 1; }
    sleep 0.2
done
wait "$LISTENER"
AFTER="$(tej beta campaign list)"
case "$AFTER" in *"$CAMP"*) echo "tester discovered the campaign with no GitHub and no internet";; *) echo "FAIL: campaign did not replicate" >&2; exit 1;; esac
step "PASS"

#!/usr/bin/env bash
# Two isolated network namespaces joined only by a veth pair. Neither has a
# default route or DNS, so GitHub (or anything else) is unreachable by
# construction. The operator publishes a Beta campaign; the tester discovers
# it over a real TCP `mini sync` across the veth. Requires root.
# Usage: sudo lab/netlab/outage-drill.sh [path-to-mini]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MINI="${1:-$ROOT/target/debug/mini}"
[ -x "$MINI" ] || { echo "build first: cargo build -p mini-cli --bin mini" >&2; exit 1; }
[ "$(id -u)" -eq 0 ] || { echo "needs root for ip netns" >&2; exit 1; }
# Root is needed only for `ip netns`; every worktree binary runs unprivileged.
RUN_UID="${SUDO_UID:-65534}"; RUN_GID="${SUDO_GID:-65534}"
W="$(mktemp -d)"; chmod 755 "$W"; chown "$RUN_UID:$RUN_GID" "$W"
 A=labop$$; B=labte$$
cleanup() { ip netns del "$A" 2>/dev/null || true; ip netns del "$B" 2>/dev/null || true; rm -rf "$W"; }
trap cleanup EXIT
ip netns add "$A"; ip netns add "$B"
VA=vA$$; VB=vB$$
ip link add "$VA" type veth peer name "$VB"
ip link set "$VA" netns "$A"; ip link set "$VB" netns "$B"
ip -n "$A" addr add 10.77.0.1/24 dev "$VA"; ip -n "$B" addr add 10.77.0.2/24 dev "$VB"
for n in "$A:$VA" "$B:$VB"; do ip -n "${n%%:*}" link set lo up; ip -n "${n%%:*}" link set "${n##*:}" up; done
step() { printf '\n== %s ==\n' "$*"; }
op() { ip netns exec "$A" setpriv --reuid="$RUN_UID" --regid="$RUN_GID" --clear-groups --no-new-privs "$MINI" --home "$W/op" --store "$W/op-store" "$@"; }
te() { ip netns exec "$B" setpriv --reuid="$RUN_UID" --regid="$RUN_GID" --clear-groups --no-new-privs "$MINI" --home "$W/te" --store "$W/te-store" "$@"; }
opj() { ip netns exec "$A" setpriv --reuid="$RUN_UID" --regid="$RUN_GID" --clear-groups --no-new-privs "$MINI" --json --home "$W/op" --store "$W/op-store" "$@"; }
tej() { ip netns exec "$B" setpriv --reuid="$RUN_UID" --regid="$RUN_GID" --clear-groups --no-new-privs "$MINI" --json --home "$W/te" --store "$W/te-store" "$@"; }

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
tej beta campaign list | grep -q "$CAMP" && { echo "tester should not know the campaign yet" >&2; exit 1; } || echo "tester knows nothing before sync"

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
tej beta campaign list | grep -q "$CAMP" && echo "tester discovered the campaign with no GitHub and no internet"
step "PASS"

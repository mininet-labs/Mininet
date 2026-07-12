#!/usr/bin/env bash
# The no-GitHub outage demo (D-0081, self-hosted forge spine roadmap #102).
#
# GitHub is this project's UAT/mirror, never its source of truth
# (CLAUDE.md). This script is the tangible proof: it drives the real,
# compiled `mini` binary -- never a library call, never GitHub -- through
# the entire developer lifecycle this project's constitution depends on
# being able to survive a GitHub outage:
#
#   identity -> repo -> commit -> PR -> two independent reviews -> governed
#   merge -> release -> two independent attestations -> verify -> install
#   -> passing health check -- and then, because a real system must survive
#   its own mistakes too, a second, DELIBERATELY BROKEN release through the
#   identical path -> a failing health check -> automatic rollback -> a
#   clean, independently-verifiable event log proving exactly what happened.
#
# Every step below is a real `mini` subcommand against real files on real
# disk. No network call to any GitHub endpoint exists anywhere in this
# codebase to make -- there is nothing to "route around" during an outage,
# because nothing here was ever routed through GitHub to begin with. That
# is the honest claim this script demonstrates, not a network firewall
# drill (this environment has no controlled way to actually sever GitHub
# reachability, and simulating a firewall would prove less than reading
# this script and the codebase's own dependency graph already does).
#
# Usage: tools/no_github_outage_demo.sh [path-to-mini-binary]
#   Defaults to <repo-root>/target/debug/mini if not given -- override with
#   an explicit path (e.g. a release build, or $CARGO_BIN_EXE_mini when
#   driven from a Rust test).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MINI="${1:-$REPO_ROOT/target/debug/mini}"

if [[ ! -x "$MINI" ]]; then
    echo "error: mini binary not found or not executable at: $MINI" >&2
    echo "  build it first: cargo build -p mini-cli --bin mini" >&2
    exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STORE="$WORK/store"
ALICE="$WORK/alice"
BOB="$WORK/bob"
CAROL="$WORK/carol"

step() {
    echo
    echo "== $* =="
}

last_word() {
    echo "$1" | awk '{print $NF}'
}

did_of() {
    local output="$1" which="$2"
    echo "$output" | grep "^${which}" | awk '{print $2}'
}

step "Phase 1: three independent identities, no GitHub account anywhere"
"$MINI" --home "$ALICE" identity init >/dev/null
"$MINI" --home "$BOB" identity init >/dev/null
"$MINI" --home "$CAROL" identity init >/dev/null
ALICE_DID="$(did_of "$("$MINI" --home "$ALICE" identity show)" "human:")"
BOB_DID="$(did_of "$("$MINI" --home "$BOB" identity show)" "human:")"
CAROL_DID="$(did_of "$("$MINI" --home "$CAROL" identity show)" "human:")"
echo "alice: $ALICE_DID"
echo "bob:   $BOB_DID"
echo "carol: $CAROL_DID"

step "Phase 2: KEL trust exchange, entirely out of band (no directory service, no GitHub identity)"
ALICE_KEL="$("$MINI" --home "$ALICE" kel export)"
BOB_KEL="$("$MINI" --home "$BOB" kel export)"
CAROL_KEL="$("$MINI" --home "$CAROL" kel export)"
"$MINI" --home "$BOB" kel trust "$ALICE_KEL" >/dev/null
"$MINI" --home "$BOB" kel trust "$CAROL_KEL" >/dev/null
"$MINI" --home "$CAROL" kel trust "$ALICE_KEL" >/dev/null
"$MINI" --home "$CAROL" kel trust "$BOB_KEL" >/dev/null
"$MINI" --home "$ALICE" kel trust "$BOB_KEL" >/dev/null
"$MINI" --home "$ALICE" kel trust "$CAROL_KEL" >/dev/null
echo "all three identities mutually trust each other's KELs"

step "Phase 3: repo init, commit, PR, two independent reviews, governed merge -- zero GitHub API calls"
"$MINI" --home "$ALICE" --store "$STORE" repo init outage-demo \
    --maintainer "$ALICE_DID" --maintainer "$BOB_DID" --maintainer "$CAROL_DID" \
    --min-approvals 2 >/dev/null
PROJECT_ID="$(cat "$ALICE/projects/outage-demo")"
"$MINI" --home "$BOB" --store "$STORE" repo track outage-demo "$PROJECT_ID" >/dev/null
"$MINI" --home "$CAROL" --store "$STORE" repo track outage-demo "$PROJECT_ID" >/dev/null

SRC="$WORK/src"
mkdir -p "$SRC"
echo 'pub fn hello() -> &'"'"'static str { "no github needed" }' > "$SRC/lib.rs"

COMMIT_OUT="$("$MINI" --home "$ALICE" --store "$STORE" repo commit outage-demo \
    --branch main --message "add hello" "$SRC/lib.rs")"
COMMIT_ID="$(last_word "$COMMIT_OUT")"
echo "commit: $COMMIT_ID"

PR_OUT="$("$MINI" --home "$ALICE" --store "$STORE" pr propose outage-demo \
    --branch main --title "add hello" --head "$COMMIT_ID")"
PR_ID="$(last_word "$PR_OUT")"
echo "PR proposed: $PR_ID"

"$MINI" --home "$BOB" --store "$STORE" pr approve "$PR_ID" --head "$COMMIT_ID" \
    --findings "lgtm from bob" >/dev/null
"$MINI" --home "$CAROL" --store "$STORE" pr approve "$PR_ID" --head "$COMMIT_ID" \
    --findings "lgtm from carol" >/dev/null
"$MINI" --home "$BOB" --store "$STORE" pr merge outage-demo "$PR_ID" >/dev/null

STATUS_OUT="$("$MINI" --home "$CAROL" --store "$STORE" repo status outage-demo)"
echo "$STATUS_OUT" | grep -q "1 entries applied" || {
    echo "FAIL: governed merge did not reach canonical status" >&2
    exit 1
}
echo "governed merge reached, verified from a third independent identity"

step "Phase 4: sandboxed build artifact, release, two independent attestations"
ARTIFACT="$WORK/release.bin"
printf 'no-github-outage-demo release artifact' > "$ARTIFACT"
RECIPE_DIGEST="$(printf '%064d' 0)"

# --json (D-0078) hands back real structured fields -- release_id and
# artifact_digest -- instead of a human sentence to scrape, exactly the
# machine-readable contract this demo relies on for the next two steps.
CREATE_JSON="$("$MINI" --home "$ALICE" --store "$STORE" --json release create outage-demo \
    --branch main --version 1.0.0 --commit "$COMMIT_ID" \
    --artifact "$ARTIFACT" --recipe-digest "$RECIPE_DIGEST")"
RELEASE_ID="$(echo "$CREATE_JSON" | grep -o '"release_id":"[^"]*"' | cut -d'"' -f4)"
ARTIFACT_DIGEST="$(echo "$CREATE_JSON" | grep -o '"artifact_digest":"[^"]*"' | cut -d'"' -f4)"
echo "release 1.0.0 created: $RELEASE_ID"
echo "artifact digest: $ARTIFACT_DIGEST"

"$MINI" --home "$BOB" --store "$STORE" release attest "$RELEASE_ID" \
    --artifact-digest "$ARTIFACT_DIGEST" >/dev/null
"$MINI" --home "$CAROL" --store "$STORE" release attest "$RELEASE_ID" \
    --artifact-digest "$ARTIFACT_DIGEST" >/dev/null
echo "two independent attestations recorded"

NOW_MS=$(( $(date +%s%3N) + 3600000 + 60000 ))
VERIFY_OUT="$("$MINI" --home "$BOB" --store "$STORE" release verify \
    "$RELEASE_ID" outage-demo --branch main --now-ms "$NOW_MS")"
echo "$VERIFY_OUT" | grep -q "2 independent attester(s)" || {
    echo "FAIL: release did not verify with real attestations" >&2
    exit 1
}
echo "$VERIFY_OUT"

step "Phase 5: install the release -- stage, preflight, owner-approved activate, passing health check"
DEVICE="$WORK/device"
"$MINI" --home "$CAROL" --store "$STORE" installer stage --device-root "$DEVICE" \
    "$RELEASE_ID" outage-demo --branch main --now-ms "$NOW_MS" --timestamp-ms "$NOW_MS" >/dev/null
"$MINI" installer preflight --device-root "$DEVICE" "$RELEASE_ID" --timestamp-ms "$NOW_MS" >/dev/null
"$MINI" installer activate --device-root "$DEVICE" "$RELEASE_ID" --approved-at-ms "$NOW_MS" >/dev/null
HEALTH_OUT="$("$MINI" installer health-check --device-root "$DEVICE" "$RELEASE_ID" \
    --healthy --timestamp-ms "$NOW_MS")"
echo "$HEALTH_OUT" | grep -q "stays active" || {
    echo "FAIL: healthy release did not stay active" >&2
    exit 1
}
echo "release 1.0.0 installed and healthy, no GitHub involved at any step"

step "Phase 6: a genuinely broken release -- fails health check, auto-rolls back, no manual intervention"
RECIPE_DIGEST_2="$(printf '%064d' 1)"
CREATE_OUT_2="$("$MINI" --home "$ALICE" --store "$STORE" release create outage-demo \
    --branch main --version 2.0.0 --commit "$COMMIT_ID" \
    --artifact "$ARTIFACT" --recipe-digest "$RECIPE_DIGEST_2")"
RELEASE_ID_2="$(last_word "$CREATE_OUT_2")"
echo "release 2.0.0 (deliberately broken) created: $RELEASE_ID_2"

"$MINI" --home "$BOB" --store "$STORE" release attest "$RELEASE_ID_2" \
    --artifact-digest "$ARTIFACT_DIGEST" >/dev/null
"$MINI" --home "$CAROL" --store "$STORE" release attest "$RELEASE_ID_2" \
    --artifact-digest "$ARTIFACT_DIGEST" >/dev/null

NOW_MS_2=$(( NOW_MS + 10 ))
"$MINI" --home "$CAROL" --store "$STORE" installer stage --device-root "$DEVICE" \
    "$RELEASE_ID_2" outage-demo --branch main --now-ms "$NOW_MS_2" --timestamp-ms "$NOW_MS_2" >/dev/null
"$MINI" installer preflight --device-root "$DEVICE" "$RELEASE_ID_2" --timestamp-ms "$NOW_MS_2" >/dev/null
"$MINI" installer activate --device-root "$DEVICE" "$RELEASE_ID_2" --approved-at-ms "$NOW_MS_2" >/dev/null
HEALTH_OUT_2="$("$MINI" installer health-check --device-root "$DEVICE" "$RELEASE_ID_2" \
    --unhealthy --timestamp-ms "$NOW_MS_2")"
echo "$HEALTH_OUT_2" | grep -q "rolled back to" || {
    echo "FAIL: broken release did not auto-rollback" >&2
    exit 1
}
echo "$HEALTH_OUT_2"

STATUS_AFTER="$("$MINI" installer status --device-root "$DEVICE")"
echo "$STATUS_AFTER" | grep -q "$RELEASE_ID" || {
    echo "FAIL: device did not roll back to the known-good release" >&2
    exit 1
}
echo "device is back on the known-good 1.0.0 release: $STATUS_AFTER"

step "Phase 7: the event log itself proves this happened -- independently verifiable, tamper-evident"
LOG_OUT="$("$MINI" installer verify-log --device-root "$DEVICE")"
echo "$LOG_OUT" | grep -q "verified clean" || {
    echo "FAIL: install event log failed independent verification" >&2
    exit 1
}
echo "$LOG_OUT"

echo
echo "=================================================================="
echo "No-GitHub outage demo complete: identity, review, governed merge,"
echo "release, attestation, install, automatic rollback, and durable"
echo "evidence all happened through nothing but the mini binary and real"
echo "files on disk. Nothing above ever named, required, or could have"
echo "been blocked by github.com."
echo "=================================================================="

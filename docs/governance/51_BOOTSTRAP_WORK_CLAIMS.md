# Bootstrap Work Claims

**Status:** Active bootstrap coordination rule
**Decision:** D-0100
**Registry:** `governance/work-claims.json`

## Purpose

Parallel AI contributors must not choose the same issue, path, or Decision
identifier by scanning prose and hoping no one else did the same thing.
The work-claim registry is the current GitHub-bootstrap coordination object
until Mininet Forge has a native proposal allocator.

The registry is coordination evidence only. A claim does not approve work,
grant authority, satisfy review, merge a proposal, or canonicalize anything.

## Claim Flow

1. Pick an open issue from the Project queue.
2. Add or renew one active claim in `governance/work-claims.json`.
3. Record contributor, branch, lease expiry, expected paths, and any Decision
   identifiers the branch will touch.
4. Run `python tools/check_governance.py --mode baseline`.
5. Create the branch named in the claim.
6. Open one PR that names the claimed issue and updates the claim with the PR
   number when available.
7. Move the claim to `in_review` while review is active.
8. Move the claim to `closed` after merge, or `expired`/`blocked` when the lane
   should be released.

## Collision Rules

- One issue may have only one active implementation claim.
- Active claims must not reserve the same Decision identifier.
- Active claims must not overlap expected paths unless the work is split into
  separate child issues first.
- Active claims must have a future lease expiry.
- Decision identifiers remain part of the append-only Decision Log. The claim
  registry allocates a working slot; it does not make the Decision accepted.

## Current Project Adapter

The GitHub Project named `Mininet Bootstrap Engineering` is the human-readable
queue. Its fields mirror this registry: contributor, branch/PR, dependencies,
risk class, external gate, lease expiry, paths expected, Decision IDs, and lease
state.

If Project state and `governance/work-claims.json` disagree, the JSON registry is
the CI-enforced source for the branch. The Project is an operational dashboard.

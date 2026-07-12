# Repository Integration Plan

**Status:** Owner execution plan  
**Version:** 0.4

## Phase A — Observe

1. Create GitHub teams without granting merge or administrative rights.
2. Install issue forms and the expanded proposal template.
3. Add governance policy and validator in non-blocking mode.
4. Record the current CI check names and baseline failures.
5. Confirm that anonymous/pseudonymous participation language is visible.

## Phase B — Enforce basic integrity

1. Require pull requests for `main` and `integration/*`.
2. Block force pushes and deletion.
3. Require format, Clippy, tests and governance-baseline checks.
4. Require stale approval dismissal and latest-state approval.
5. Activate CODEOWNERS review routing.

## Phase C — Enforce sensitive domains

1. Require two authorized approvals for protocol-critical and cryptography-sensitive changes.
2. Make RustSec and `cargo-deny` blocking after triaging the baseline.
3. Protect release tags and environments.
4. Require explicit AI disclosure and exact-state human/pseudonymous acceptance.
5. Require invariant and decision-log updates for Tier-F changes.

## Phase D — Scale

1. Introduce working-group teams and rotating integration maintainers.
2. Enable merge queue for independent changes after `merge_group` CI support.
3. Retain explicit integration branches for tightly coupled changes.
4. Audit team membership and bypass rights quarterly.
5. Publish machine-readable authority rosters without compulsory legal names.

## Phase E — Forge equivalence

1. Represent every proposal field as a signed Forge object.
2. Bind reviews and approvals to immutable proposal digests.
3. Execute path/domain policy inside Forge.
4. Reproduce CI evidence through signed build attestations.
5. Demonstrate development during a GitHub outage.
6. Govern the switch of canonical authority from GitHub to Forge.

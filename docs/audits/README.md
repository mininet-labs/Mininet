# Audits

Written deliverables for the review/audit-shaped issues in the
[master roadmap](../../issues/92). Each
file is named `issue-N-<short-name>.md` after the GitHub issue it closes
or substantially addresses, so the mapping between "what was asked" and
"what was delivered" stays traceable without needing GitHub itself.

| File | Closes | Verdict |
|---|---|---|
| [`issue-8-constitutional-audit.md`](issue-8-constitutional-audit.md) | [#8](../../issues/8) | 18 PASS, 7 PARTIAL, 1 not yet built, 0 FAIL |
| [`issue-10-frozen-invariants-review.md`](issue-10-frozen-invariants-review.md) | [#10](../../issues/10) | No current violation found; Sybil-cost economics flagged as the sharpest open "maybe" |
| [`issue-29-cid-integrity-review.md`](issue-29-cid-integrity-review.md) | [#29](../../issues/29) | PASS across all four layers reviewed (multihash, object id, store, chunked assembly) |
| [`issue-71-memory-safety-audit.md`](issue-71-memory-safety-audit.md) | [#71](../../issues/71) | PASS; surfaced a real `cargo-audit`/toolchain incompatibility, fixed in CI |
| [`issue-12-did-mini-security-audit.md`](issue-12-did-mini-security-audit.md) | [#12](../../issues/12) | Sound core; 3 findings fixed (threshold-rewrite, delegated-as-root, seed scrub); M3 witnesses the launch-blocking gap |
| [`issue-13-identity-recovery-audit.md`](issue-13-identity-recovery-audit.md) | [#13](../../issues/13) | Recovery path built (`recover_from_kel`) + threat-modeled per scenario; social recovery (M5) and KEL freshness (M3) remain open |
| [`issue-17-presence-attack-review.md`](issue-17-presence-attack-review.md) | [#17](../../issues/17) | Replay/binding/clone defended; active relay NOT (needs UWB distance-bounding) — safe as a weighted signal only |
| [`issue-18-sybil-social-graph-review.md`](issue-18-sybil-social-graph-review.md) | [#18](../../issues/18) | Propagation sound; farm-saturation bypass fixed (F1); "no longer cheap" claim UNPROVEN at production params — threat still open |
| [`external-full-codebase-audit-2026-08-05.md`](external-full-codebase-audit-2026-08-05.md) | — | Received external review; unverified reviewer, no tooling run. 7 findings confirmed and fixed, the rest tracked |
| [`external-full-codebase-audit-2026-08-05-response.md`](external-full-codebase-audit-2026-08-05-response.md) | — | Disposition record for the above. Closes no gate |

## Required scope header (D-0441)

Every audit added from 2026-08-05 onward must open with a scope table:

```markdown
**Audit scope**

| Field | Value |
|---|---|
| Reviewed at | `<branch>` @ `<full commit SHA>` |
| Workspace size at that commit | `<N>` crates, `<M>` resolved dependencies |
| Method | what was actually done: built? tested? tools run? read only? |
| Tool versions | exact versions, or "none" |
| Revalidation trigger | what change would invalidate this verdict |

```

The reason is specific. An audit that says "22/22 crates carry
`#![forbid(unsafe_code)]`" reads as whole-workspace coverage; against a
workspace that now holds 72 crates it is a historical statement about roughly a
third of it. Nothing was wrong when it was written — what was missing was the
document saying what it covered, so a later reader could tell. A verdict
without a scope is indistinguishable from a broader verdict than was earned.

**The twelve audits written before this rule do not have one, and have not been
back-filled.** Their commit SHAs, dependency counts, and tool versions were not
recorded at the time, and inventing them would make the documents look more
rigorous than they were. Read each one's coverage against the crate count it
states in its own text, and treat any count well below 72 as historical.

These are point-in-time documents, not living ones — if code changes in a
way that could affect a verdict above, open a new audit issue and file a
new dated entry rather than silently editing an old one. That's the same
append-only discipline `docs/DECISION_LOG.md` and `docs/FAILURE_BOOK.md`
already use, for the same reason: the reasoning trail matters as much as
the current state.

# Audits

Written deliverables for the review/audit-shaped issues in the
[master roadmap](https://github.com/britak420/Mininet/issues/92). Each
file is named `issue-N-<short-name>.md` after the GitHub issue it closes
or substantially addresses, so the mapping between "what was asked" and
"what was delivered" stays traceable without needing GitHub itself.

| File | Closes | Verdict |
|---|---|---|
| [`issue-8-constitutional-audit.md`](issue-8-constitutional-audit.md) | [#8](https://github.com/britak420/Mininet/issues/8) | 18 PASS, 7 PARTIAL, 1 not yet built, 0 FAIL |
| [`issue-10-frozen-invariants-review.md`](issue-10-frozen-invariants-review.md) | [#10](https://github.com/britak420/Mininet/issues/10) | No current violation found; Sybil-cost economics flagged as the sharpest open "maybe" |
| [`issue-29-cid-integrity-review.md`](issue-29-cid-integrity-review.md) | [#29](https://github.com/britak420/Mininet/issues/29) | PASS across all four layers reviewed (multihash, object id, store, chunked assembly) |
| [`issue-71-memory-safety-audit.md`](issue-71-memory-safety-audit.md) | [#71](https://github.com/britak420/Mininet/issues/71) | PASS; surfaced a real `cargo-audit`/toolchain incompatibility, fixed in CI |

These are point-in-time documents, not living ones — if code changes in a
way that could affect a verdict above, open a new audit issue and file a
new dated entry rather than silently editing an old one. That's the same
append-only discipline `docs/DECISION_LOG.md` and `docs/FAILURE_BOOK.md`
already use, for the same reason: the reasoning trail matters as much as
the current state.

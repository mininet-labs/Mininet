# Repository and Forge Operations

**Status:** Operational, adaptable to platform

## Branch/object classes

- `main` or canonical head: protected representation of canonical history.
- `integration/<batch>`: temporary candidate combining coupled work.
- `feature/<identity-or-alias>/<scope>`: contributor proposal branch.
- `release/<version>`: optional stabilization candidate; never a bypass around canonical governance.
- signed tags/release objects: governed release references.

Mininet Forge should eventually represent these as content-addressed proposal, branch-pointer, review, merge-decision, build-result, and release objects.

## Direct push rule

No routine contributor, AI agent, or maintainer pushes directly to canonical history. Emergency access is tightly restricted, logged, and retrospectively reviewed.

## Review binding

Approvals bind to the exact final digest. Material changes dismiss stale approvals. The author cannot count toward independent approval quorum. AI accounts never count as human quorum.

## Integration branch procedure

1. Create integration candidate from current canonical head.
2. Contributors create proposal branches from that candidate.
3. Each proposal receives domain review and CI.
4. Merge proposals into the integration candidate.
5. Run combined workspace, adversarial, migration, and reproducibility checks.
6. Open one final integration proposal to canonical history.
7. Obtain required independent approvals on the final digest.
8. Canonicalize through protected merge or governed merge decision.
9. Delete temporary branches after records are preserved.

## Merge methods

Squash merging is suitable for compact bootstrap history when the final commit preserves proposal references and authorship. Merge commits may be preferable for complex stacked work or when preserving exact signed commits matters. The constitutional requirement is evidence continuity, not a particular Git command.

## CODEOWNERS and future ownership

GitHub CODEOWNERS routes review during bootstrap. In Mininet Forge, signed working-group ownership policies replace it. Ownership grants review responsibility, not unilateral correctness or political privilege.

## Issue intake

Issue submission should be open to broad participation with forms for bugs, research, security, design, bounty proposals, and implementation tasks. Sensitive vulnerabilities use private reporting.

Anonymous Mininet Forge submissions should be accepted without compulsory linkage to a persistent identity. Spam resistance should use rate limits, proof of work/resource, reputation, local filtering, or optional moderation rather than mandatory public identity.

## Emergency changes

Emergency fixes must be minimal, reversible where possible, and limited to the active threat. They require preserved evidence, exact authorization, rapid independent review, and a mandatory retrospective. Emergency power cannot amend constitutional principles or force owner adoption.

## Dependency and CI policy

Security advisories and dependency policy checks should block merging unless a time-bounded, documented exception is approved. External CI actions should be pinned immutably. Build secrets must not be exposed to untrusted proposal code.

## Synchronization with Mininet Forge

During hybrid operation:

- every canonical GitHub merge is imported into the Forge with its evidence;
- every Forge-governed merge is mirrored to GitHub;
- mappings between Git and Mininet object IDs are preserved;
- disagreement triggers a halt and reconciliation, not silent preference for GitHub;
- the Forge becomes authoritative only after the transition gates in Document 09 are met.

## Canonical-surface rule

Exactly one surface is operationally canonical at a time. During hybrid operation, disagreement between GitHub and Forge halts canonicalization until reconciled. Dual-write convenience must never create two competing truths. The transition decision must identify the exact canonical checkpoint and recovery procedure.

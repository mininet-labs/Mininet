# Scalable Development Workflow

**Status:** Normative workflow; platform-neutral

## Design goal

The workflow must remain understandable with two contributors and viable with hundreds. Scaling is achieved by partitioning review responsibility, not lowering legitimacy requirements.

## Universal topology

`Canonical History <- Integration Candidate <- Change Proposals <- Contributor Workspaces`

No contributor works directly on canonical history. All work enters through a proposal bound to an exact artifact digest.

## Stage 0 — Founder only

The founder may author and canonicalize, but every change should still carry tests, traceability, and an explicit self-review record. AI should be used adversarially rather than only for generation.

## Stage 1 — Founder plus AI and occasional contributors

External contributors submit proposals. Founder remains canonicalization authority. AI cannot substitute for independent human review on protocol-critical changes.

## Stage 2 — Two engineers

Create a temporary integration candidate for each coherent batch.

- Engineer A and B branch from the same integration base.
- A reviews B; B reviews A.
- Each proposal passes its own checks.
- Both proposals are combined in the integration candidate.
- Full workspace and adversarial integration tests run on the combined state.
- A final integration proposal targets canonical history.
- Protocol-critical canonicalization requires the founder or a third independent reviewer in addition to the non-author engineer.

## Stage 3 — Three to ten maintainers

Introduce domain ownership and a rotating integration maintainer. Require two independent approvals for protocol-critical work. At least one reviewer should be outside the author's immediate implementation pair.

Use a merge queue only after CI handles combined candidate states correctly. The queue is an optimization; it does not replace explicit risk classification or cross-domain review.

## Stage 4 — Working groups

Create working groups for domains such as identity, consensus, networking, forge, updates, storage, value/treasury, applications, and governance.

Each group maintains:

- domain roadmap;
- reviewer roster;
- threat-model ownership;
- invariant mapping;
- on-call security response;
- integration representative.

A group may accept ordinary domain work. Cross-domain or constitutional work escalates to the Integration Council or network governance.

## Stage 5 — Hundreds of contributors

Contributors need not become maintainers. Most work flows through bounded proposals and automated evidence generation.

Review scales through:

- risk-based routing;
- CODEOWNERS/forge ownership rules;
- specialist review pools;
- AI adversarial triage;
- signed evidence bundles;
- integration representatives;
- merge queues for independent changes;
- dedicated integration branches for tightly coupled changes.

No single maintainer is expected to understand the whole codebase. The system must make cross-domain assumptions explicit.

## Proposal classes

### Independent

Can be merged through the queue after required checks and ownership review.

### Coupled

Multiple proposals share an integration candidate and are canonicalized as one batch.

### Stacked

Proposal B depends on A. B initially targets A's candidate, then is rebased or retargeted once A is integrated.

### Constitutional/security-critical

Requires stronger quorum, explicit traceability, adversarial review, and possibly external gates.

## Integration completeness

A feature is not complete until:

1. it works against the current canonical interfaces;
2. combined tests pass with concurrent proposals;
3. documentation and threat model are updated;
4. migration and rollback behavior are known;
5. release evidence can bind to the integrated state.

## Contributor access

Anonymous contributors may submit through Mininet Forge without public identity. During GitHub bootstrap, GitHub account requirements are a platform limitation, not a constitutional requirement. Alternative encrypted submission or relay processes should be provided for sensitive anonymous contributions where practical.

## Compensation workflow

A bounty defines objective acceptance conditions before work begins where possible. On acceptance, the resulting canonical proposal references a claim object and privacy-preserving payment destination. Payment does not grant future authority.

## Scaling invariant

Contributor count changes routing, not legitimacy. Two contributors and two hundred contributors use the same rules: exact-state review, author exclusion from independent quorum, combined integration evidence, scoped authority, and protected canonicalization. At scale, working groups and merge queues distribute work; they do not lower the evidence required for sensitive changes.

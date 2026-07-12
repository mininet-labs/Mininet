# RFC-0003: Working-Group Governance

**Status:** Proposed

## Abstract

This RFC defines bounded domain governance for Mininet. Working groups organize expertise and exercise explicitly delegated authority without owning protocol domains or gaining political weight through size, wealth, employer, or funding.

## Core objects

- WorkingGroupCharter;
- GroupLifecycleDecision;
- ReviewerDelegation;
- MaintainerDelegation;
- IntegrationRepresentativeDelegation;
- Suspension;
- Revocation;
- GroupReport;
- Split/MergePlan;
- CrossGroupIntegrationDecision.

## Invariants

1. all authority expires;
2. authority is scoped and revocable;
3. public legal identity is not required by default;
4. AI does not satisfy current authority quorum;
5. one key lineage cannot masquerade as independent reviewers;
6. working groups cannot amend frozen invariants alone;
7. payment or employer does not buy governance weight;
8. group reorganization cannot orphan security or integration obligations;
9. cross-domain decisions require combined evidence;
10. founder/bootstrap authority decreases only when replacement continuity is demonstrated.

## Lifecycle

Proposed, Incubating, Active, Mature, Suspended, Splitting, Merging, Retired.

## Security considerations

Threats include employer capture, inactive authority, hidden common control, review cartels, boundary neglect, security-information leakage, and emergency powers becoming permanent. Terms, diversity evidence, revocation, appeals, and cross-group integration reduce but do not eliminate these risks.

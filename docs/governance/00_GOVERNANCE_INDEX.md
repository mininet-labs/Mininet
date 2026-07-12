# Mininet Governance Index

**Status:** Normative governance map for the bootstrap period  
**Version:** 0.3  
**Applies to:** GitHub bootstrap, Mininet Forge, and successor collaboration systems  
**Canonical precedence:** This pack does not replace `SPEC-00`, `docs/INVARIANTS.md`, or accepted entries in `docs/DECISION_LOG.md`.

## Purpose

This pack defines how Mininet development should be organized, reviewed, compensated, canonicalized, released, and eventually migrated from GitHub into Mininet Forge. It is designed to remain useful when the collaboration platform changes.

GitHub is current bootstrap infrastructure. It is not the permanent source of legitimacy. Mininet's durable legitimacy comes from constitutional continuity, valid authorization, preserved evidence, governed canonical history, and voluntary owner adoption.

## Normative hierarchy

When two sources disagree, use this precedence until an accepted constitutional amendment changes it:

1. `SPEC-00`, including its canonical constitutional register.
2. Frozen invariants in `docs/INVARIANTS.md`; if that mirror disagrees with `SPEC-00`, `SPEC-00` wins.
3. Accepted decisions in the append-only `docs/DECISION_LOG.md`, within the limits imposed by higher authority.
4. Technical specifications, threat models, and externally required audit gates.
5. This governance pack and accepted governance RFCs.
6. Platform-specific procedures, repository settings, templates, and checklists.
7. Implementation, tests, attestations, and releases as evidence of what is actually enforced.

`docs/FOUNDER_DIRECTIVES.md` supplies the reasoning compass behind the constitutional rules. It should guide interpretation but must not be used to silently override the canonical register.

## Documents

| File | Function |
|---|---|
| `000_GOVERNANCE_ARCHITECTURE.md` | Layered architecture defining where governance rules, enforcement, and evidence belong. |
| `01_DEVELOPMENT_CONSTITUTION.md` | Development-governance principles subordinate to the canonical Constitution. |
| `02_LEGITIMACY_MODEL.md` | State machine by which work may become canonical. |
| `03_DIRECTIVE_TRACEABILITY.md` | Required traceability from directive and invariant to implementation evidence. |
| `04_AI_HUMAN_COLLABORATION_WORKBOOK.md` | Repeatable human/AI authoring, attack, review, and acceptance workflows. |
| `05_SCALABLE_DEVELOPMENT_WORKFLOW.md` | Contribution workflow for two contributors through hundreds. |
| `06_REPOSITORY_AND_FORGE_OPERATIONS.md` | Branching, integration, emergency, and GitHub/Forge synchronization rules. |
| `07_RELEASE_AND_OWNER_ADOPTION.md` | Governed releases, provenance, installation, and voluntary adoption. |
| `08_FOUNDER_BOOTSTRAP_AND_HANDOFF.md` | Temporary founder duties and reduction of exceptional authority. |
| `09_TRANSITION_TO_SELF_GOVERNANCE.md` | Evidence-based phase gates for authority transfer. |
| `10_GITHUB_DECOMMISSION_PLAN.md` | Conditions for mirror, archive, or shutdown status. |
| `11_WORKING_GROUPS_AND_MAINTAINERS.md` | Domain governance for a large contributor population. |
| `12_ANONYMOUS_CONTRIBUTION_AND_COMPENSATION.md` | Anonymous/pseudonymous contribution, reputation, bounties, and settlement. |
| `13_REPOSITORY_OWNER_SETUP_GUIDE.md` | Current owner's non-file-push setup procedure. |
| `14_CANONICAL_VOCABULARY.md` | One required vocabulary for all governance documents. |
| `15_CONSISTENCY_MATRIX.md` | Cross-document rule and authority matrix. |
| `16_PROTOCOL_ONTOLOGY.md` | Formal definitions for actors, artifacts, authority, legitimacy, release, and compensation. |
| `17_NORMATIVE_LANGUAGE_AND_SPEC_TEMPLATE.md` | Requirement language and standard form for governance specifications. |
| `18_GOVERNANCE_STATE_MACHINES.md` | Formal lifecycle transitions for proposals, reviews, releases, bounties, delegations, and amendments. |
| `19_GOVERNANCE_DECISION_TABLE.md` | Actor permissions, independence rules, and risk classes. |
| `20_FAILURE_MODES_AND_CONTINUITY.md` | Threat catalogue and continuity requirements. |
| `21_GOVERNANCE_TEST_SUITE.md` | Positive, adversarial, recovery, and scaling scenarios. |
| `22_MACHINE_READABLE_SUMMARIES.md` | Experimental structured summary schema and validation direction. |
| `RFC-0001_PROTOCOL_GOVERNANCE.md` | Platform-independent governance protocol proposal. |
| `CHANGELOG.md` | Versioned changes to this pack. |

## Universal lifecycle

Every governed artifact uses the following conceptual lifecycle:

`Research -> Proposal -> Implementation -> Evidence -> Adversarial Review -> Authorized Review -> Integration -> Canonicalization -> Release -> Owner Adoption`

Not every artifact reaches every state. A GitHub pull request is one implementation of a Change Proposal. A merge into `main` is the present bootstrap implementation of canonicalization. Neither GitHub nor a branch name is constitutional authority by itself.

## Interpretation rules

When a novel situation is not explicitly covered, prefer the interpretation that:

1. preserves frozen invariants and objective protocol truth;
2. increases participant sovereignty;
3. minimizes mandatory identity disclosure and institutional trust;
4. keeps money separate from political voice;
5. makes authority scoped, reviewable, and revocable;
6. keeps forks technically free without counterfeiting continuity;
7. preserves explicit owner choice over software adoption;
8. describes current implementation honestly, especially personhood and cryptographic maturity.

## Current hard honesty requirements

- A verified `did:mini` root is not yet proof of one unique human.
- Passing tests are not an external cryptography audit.
- GitHub remains operationally canonical until Forge transition gates are actually satisfied.
- A governed release may be offered, but only an owner may activate it.
- Anonymous contribution is permitted; higher-risk authority may require persistent cryptographic continuity without requiring public legal identity.


## v0.5–v0.6 specifications

- `28_FORGE_NATIVE_GOVERNANCE_OBJECTS.md`
- `29_ANONYMOUS_BOUNTY_LIFECYCLE.md`
- `30_COMPENSATION_PRIVACY_AND_SETTLEMENT.md`
- `31_DISPUTES_APPEALS_AND_RESTITUTION.md`
- `32_PSEUDONYMOUS_REPUTATION_AND_KEY_CONTINUITY.md`
- `33_WORKING_GROUP_CHARTER_AND_LIFECYCLE.md`
- `34_MAINTAINER_DELEGATION_AND_ROTATION.md`
- `35_CROSS_GROUP_INTEGRATION_COUNCIL.md`
- `36_SCALING_FROM_TWO_TO_THOUSANDS.md`
- `37_GITHUB_TO_FORGE_AUTHORITY_MAPPING.md`
- `38_V05_V06_IMPLEMENTATION_BACKLOG.md`
- `RFC-0002_FORGE_GOVERNANCE_OBJECTS.md`
- `RFC-0003_WORKING_GROUP_GOVERNANCE.md`


## Version 1 completion documents

- [39 — Constitutional Amendment Protocol](39_CONSTITUTIONAL_AMENDMENT_PROTOCOL.md)
- [40 — Governance Simulation and Stress Testing](40_GOVERNANCE_SIMULATION_AND_STRESS_TESTING.md)
- [41 — External Review and Public Challenge](41_EXTERNAL_REVIEW_AND_PUBLIC_CHALLENGE.md)
- [42 — Governance v1 Conformance Standard](42_GOVERNANCE_V1_CONFORMANCE_STANDARD.md)
- [43 — Succession and Founder Disappearance](43_SUCCESSION_AND_FOUNDER_DISAPPEARANCE.md)
- [44 — Right to Fork, Exit, and Restart](44_RIGHT_TO_FORK_EXIT_AND_RESTART.md)
- [45 — Governance Security and Privacy Model](45_GOVERNANCE_SECURITY_AND_PRIVACY_MODEL.md)
- [46 — Implementation Conformance Map](46_IMPLEMENTATION_CONFORMANCE_MAP.md)
- [47 — Activation, Deployment, and Migration](47_ACTIVATION_DEPLOYMENT_AND_MIGRATION.md)
- [48 — Post-v1 Evolution and Open Research](48_POST_V1_EVOLUTION_AND_OPEN_RESEARCH.md)
- [49 — v1 Release Audit and Sign-off](49_V1_RELEASE_AUDIT_AND_SIGNOFF.md)
- [RFC-0004 — Constitutional Amendments](RFC-0004_CONSTITUTIONAL_AMENDMENTS.md)
- [RFC-0005 — Forge Cutover and Platform Exit](RFC-0005_FORGE_CUTOVER_AND_PLATFORM_EXIT.md)
